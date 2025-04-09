use crate::mini_mno::GridCell;
use las::Reader;
use pasture_core::containers::VectorBuffer;
use pasture_core::layout::attributes;
use pasture_core::math::AABB;
use pasture_core::nalgebra::{Point3, Vector3};
use pasture_io::las::LasPointFormat0;
use point_cloud_viewer::navigation::Matrices;
use point_cloud_viewer::renderer::backends::glium::GliumRenderOptions;
use point_cloud_viewer::renderer::settings::{
    BaseRenderSettings, CategoricalAttributeColoring, ColorPalette, PointCloudRenderSettings,
    PointColor, PointShape, PointSize,
};
use point_cloud_viewer::renderer::viewer::{PointCloudId, RenderThreadBuilderExt, Window};
use std::collections::{HashMap, HashSet};

type Point = LasPointFormat0;

/// An extremely simplified implementation of a modifiable nested octree (MNO).
/// (Strictly speaking, this "nested" octree is not even nested.)
mod mini_mno {
    use super::Point;
    use pasture_core::math::AABB;
    use pasture_core::nalgebra::{Point3, Vector3, Vector4, distance_squared};
    use point_cloud_viewer::navigation::Matrices;
    use std::collections::hash_map::Entry;
    use std::collections::{HashMap, HashSet};
    use std::mem;
    use std::sync::Arc;

    /// A level of detail, corresponding to a certain depth in the octree.
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
    pub struct LodLevel(u16);

    /// Position of a cell within a 3d grid
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    pub struct GridPosition {
        x: i32,
        y: i32,
        z: i32,
    }

    /// Uniquely identifies a node in the octree.
    #[derive(Debug, Clone, Eq, PartialEq, Hash)]
    pub struct GridCell {
        lod: LodLevel,
        pos: GridPosition,
    }

    /// The contents of a node in the octree.
    #[derive(Clone)]
    pub struct NodeData {
        points: HashMap<GridPosition, Point>,
    }

    /// Queries the view frustum
    pub struct Query {
        pub camera: Matrices,
        pub min_screen_point_distance: f64,
    }

    pub struct Octree {
        nodes: HashMap<GridCell, Arc<NodeData>>,
        roots: HashSet<GridCell>,
        dirty: HashSet<GridCell>,
    }

    impl LodLevel {
        /// Coarsest lod level (Level "0")
        pub fn base() -> Self {
            LodLevel(0)
        }

        /// Returns the next finer LOD level
        pub fn finer(&self) -> Self {
            LodLevel(self.0 + 1)
        }

        /// Returns the size (side length) of the [GridCell]s at this level.
        pub fn grid_cell_size(&self) -> f64 {
            const BASE_SIZE: f64 = 1024.0;
            let factor = 0.5_f64.powi(self.0 as i32);
            BASE_SIZE * factor
        }

        /// Returns the distance between neighbouring points at this LOD level
        /// (The "Point Cell Size" for grid center sampling or the minimal distance between any two
        /// points for poisson disk sampling)
        pub fn point_distance(&self) -> f64 {
            const BASE_DIST: f64 = 8.0;
            let factor = 0.5_f64.powi(self.0 as i32);
            BASE_DIST * factor
        }
    }

    impl GridCell {
        pub fn root_at_point(point: &Point) -> Self {
            Self::root_at_position(&point.position.into())
        }

        pub fn root_at_position(position: &Point3<f64>) -> Self {
            let lod = LodLevel::base();
            let pos = GridPosition {
                x: (position.x / lod.grid_cell_size()) as i32,
                y: (position.y / lod.grid_cell_size()) as i32,
                z: (position.z / lod.grid_cell_size()) as i32,
            };
            GridCell { pos, lod }
        }

        pub fn bounds(&self) -> AABB<f64> {
            let cell_size = self.lod.grid_cell_size();
            let min = Point3::new(
                self.pos.x as f64 * cell_size,
                self.pos.y as f64 * cell_size,
                self.pos.z as f64 * cell_size,
            );
            let max = Point3::new(min.x + cell_size, min.y + cell_size, min.z + cell_size);
            AABB::from_min_max_unchecked(min, max)
        }

        pub fn children(&self) -> [GridCell; 8] {
            let lod = self.lod.finer();
            let x = self.pos.x * 2;
            let y = self.pos.y * 2;
            let z = self.pos.z * 2;
            [
                GridCell {
                    lod,
                    pos: GridPosition { x, y, z },
                },
                GridCell {
                    lod,
                    pos: GridPosition { x, y, z: z + 1 },
                },
                GridCell {
                    lod,
                    pos: GridPosition { x, y: y + 1, z },
                },
                GridCell {
                    lod,
                    pos: GridPosition {
                        x,
                        y: y + 1,
                        z: z + 1,
                    },
                },
                GridCell {
                    lod,
                    pos: GridPosition { x: x + 1, y, z },
                },
                GridCell {
                    lod,
                    pos: GridPosition {
                        x: x + 1,
                        y,
                        z: z + 1,
                    },
                },
                GridCell {
                    lod,
                    pos: GridPosition {
                        x: x + 1,
                        y: y + 1,
                        z,
                    },
                },
                GridCell {
                    lod,
                    pos: GridPosition {
                        x: x + 1,
                        y: y + 1,
                        z: z + 1,
                    },
                },
            ]
        }

        pub fn split_children(&self, points: Vec<Point>) -> [(GridCell, Vec<Point>); 8] {
            let lod = self.lod.finer();
            let x = self.pos.x * 2;
            let y = self.pos.y * 2;
            let z = self.pos.z * 2;
            let mut result = [
                (
                    GridCell {
                        lod,
                        pos: GridPosition { x, y, z },
                    },
                    Vec::new(),
                ),
                (
                    GridCell {
                        lod,
                        pos: GridPosition { x, y, z: z + 1 },
                    },
                    Vec::new(),
                ),
                (
                    GridCell {
                        lod,
                        pos: GridPosition { x, y: y + 1, z },
                    },
                    Vec::new(),
                ),
                (
                    GridCell {
                        lod,
                        pos: GridPosition {
                            x,
                            y: y + 1,
                            z: z + 1,
                        },
                    },
                    Vec::new(),
                ),
                (
                    GridCell {
                        lod,
                        pos: GridPosition { x: x + 1, y, z },
                    },
                    Vec::new(),
                ),
                (
                    GridCell {
                        lod,
                        pos: GridPosition {
                            x: x + 1,
                            y,
                            z: z + 1,
                        },
                    },
                    Vec::new(),
                ),
                (
                    GridCell {
                        lod,
                        pos: GridPosition {
                            x: x + 1,
                            y: y + 1,
                            z,
                        },
                    },
                    Vec::new(),
                ),
                (
                    GridCell {
                        lod,
                        pos: GridPosition {
                            x: x + 1,
                            y: y + 1,
                            z: z + 1,
                        },
                    },
                    Vec::new(),
                ),
            ];

            let center = self.bounds().center();
            for point in points {
                let mut index = 0;
                let position = point.position;
                if position.x > center.x {
                    index += 4;
                }
                if position.y > center.y {
                    index += 2;
                }
                if position.z > center.z {
                    index += 1;
                }
                result[index].1.push(point);
            }

            result
        }
    }

    impl NodeData {
        pub fn new() -> Self {
            NodeData {
                points: HashMap::new(),
            }
        }

        /// insert points into the node
        /// and return the points, that have been "rejected", that need to go into a finer LOD.
        pub fn insert(&mut self, points: Vec<Point>, cell: &GridCell) -> Vec<Point> {
            let mut next_lod_points = Vec::with_capacity(points.len());
            let fine_cell_size = cell.lod.point_distance();
            for mut point in points {
                let position = point.position;
                let fine_cell = GridPosition {
                    x: (position.x / fine_cell_size) as i32,
                    y: (position.y / fine_cell_size) as i32,
                    z: (position.z / fine_cell_size) as i32,
                };
                match self.points.entry(fine_cell) {
                    Entry::Vacant(v) => {
                        v.insert(point);
                    }
                    Entry::Occupied(mut o) => {
                        let fine_cell_center = Point3::new(
                            fine_cell.x as f64 * fine_cell_size + fine_cell_size * 0.5,
                            fine_cell.y as f64 * fine_cell_size + fine_cell_size * 0.5,
                            fine_cell.z as f64 * fine_cell_size + fine_cell_size * 0.5,
                        );
                        let old_distance_to_center =
                            distance_squared(&fine_cell_center, &o.get().position.into());
                        let new_distance_to_center =
                            distance_squared(&fine_cell_center, &point.position.into());
                        if new_distance_to_center < old_distance_to_center {
                            mem::swap(o.get_mut(), &mut point);
                        }
                        next_lod_points.push(point);
                    }
                };
            }

            next_lod_points
        }

        pub fn points(&self) -> Vec<Point> {
            self.points.values().cloned().collect()
        }
    }

    impl Query {
        pub fn should_load(&self, cell: &GridCell) -> bool {
            // intersection test between view frustum and cell
            let aabb = cell.bounds();
            let mut intersects = true;
            let view_projection_matrix = self.camera.projection_matrix * self.camera.view_matrix;
            let view_projection_matrix_inv =
                self.camera.view_matrix_inv * self.camera.projection_matrix_inv;
            let cube = [
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
                Vector3::new(0.0, 1.0, 0.0),
                Vector3::new(0.0, 1.0, 1.0),
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 1.0),
                Vector3::new(1.0, 1.0, 0.0),
                Vector3::new(1.0, 1.0, 1.0),
            ];
            let cell_on_screen = cube
                .iter()
                .map(|&v| {
                    let factor_min = v;
                    let factor_max = Vector3::new(1.0, 1.0, 1.0) - v;
                    let point = aabb.min().coords.component_mul(&factor_min)
                        + aabb.max().coords.component_mul(&factor_max);
                    let point_hom = Vector4::new(point.x, point.y, point.z, 1.0);
                    let point_screen_hom = view_projection_matrix * point_hom;
                    point_screen_hom.xyz() / point_screen_hom.w
                })
                .collect::<Vec<Vector3<f64>>>();
            let view_frustum_in_world = cube
                .iter()
                .map(|v| {
                    let point = v * 2.0 - Vector3::new(1.0, 1.0, 1.0);
                    let point_hom = Vector4::new(point.x, point.y, point.z, 1.0);
                    let point_world_hom = view_projection_matrix_inv * point_hom;
                    point_world_hom.xyz() / point_world_hom.w
                })
                .collect::<Vec<Vector3<f64>>>();
            if cell_on_screen.iter().all(|p| p.x < -1.0) {
                intersects = false
            }
            if cell_on_screen.iter().all(|p| p.y < -1.0) {
                intersects = false
            }
            if cell_on_screen.iter().all(|p| p.z < -1.0) {
                intersects = false
            }
            if cell_on_screen.iter().all(|p| p.x > 1.0) {
                intersects = false
            }
            if cell_on_screen.iter().all(|p| p.y > 1.0) {
                intersects = false
            }
            if cell_on_screen.iter().all(|p| p.z > 1.0) {
                intersects = false
            }
            if view_frustum_in_world.iter().all(|p| p.x < aabb.min().x) {
                intersects = false
            }
            if view_frustum_in_world.iter().all(|p| p.y < aabb.min().y) {
                intersects = false
            }
            if view_frustum_in_world.iter().all(|p| p.z < aabb.min().z) {
                intersects = false
            }
            if view_frustum_in_world.iter().all(|p| p.x > aabb.max().x) {
                intersects = false
            }
            if view_frustum_in_world.iter().all(|p| p.y > aabb.max().y) {
                intersects = false
            }
            if view_frustum_in_world.iter().all(|p| p.z > aabb.max().z) {
                intersects = false
            }
            if !intersects {
                return false;
            }

            // test the lod level
            if cell.lod == LodLevel::base() {
                return true;
            }
            let center = aabb.center();
            let center_hom = Vector4::new(center.x, center.y, center.z, 1.0);
            let center_screen_hom = view_projection_matrix * center_hom;
            let min_clip_dist = self.min_screen_point_distance / self.camera.window_size.x * 2.0;
            let min_world_point_distance = (view_projection_matrix_inv
                * Vector4::new(min_clip_dist * center_screen_hom.w, 0.0, 0.0, 0.0))
            .xyz()
            .norm();
            cell.lod.point_distance() >= min_world_point_distance
        }
    }

    impl Octree {
        pub fn new() -> Self {
            Octree {
                nodes: HashMap::new(),
                roots: HashSet::new(),
                dirty: HashSet::new(),
            }
        }

        pub fn get(&self, cell: &GridCell) -> Arc<NodeData> {
            match self.nodes.get(cell) {
                None => Arc::new(NodeData::new()),
                Some(n) => Arc::clone(n),
            }
        }

        pub fn set(&mut self, cell: &GridCell, data: Arc<NodeData>) {
            self.dirty.insert(cell.clone());
            self.nodes.insert(cell.clone(), data);
            if cell.lod == LodLevel::base() {
                self.roots.insert(cell.clone());
            }
        }

        pub fn insert(&mut self, points: Vec<Point>) {
            let mut lod0 = HashMap::new();
            for point in points {
                let cell = GridCell::root_at_point(&point);
                lod0.entry(cell).or_insert_with(Vec::new).push(point);
            }
            let mut todo = lod0.into_iter().collect::<Vec<_>>();
            while let Some((cell, cell_points)) = todo.pop() {
                let mut data = self.get(&cell);
                let next_lod_points = Arc::make_mut(&mut data).insert(cell_points, &cell);
                self.set(&cell, data);
                if cell.lod.0 < 10 {
                    for (child_cell, child_points) in cell.split_children(next_lod_points) {
                        if !child_points.is_empty() {
                            todo.push((child_cell, child_points));
                        }
                    }
                }
            }
        }

        pub fn take_dirty(&mut self) -> HashSet<GridCell> {
            mem::take(&mut self.dirty)
        }

        pub fn query(&self, query: Query) -> HashSet<GridCell> {
            let mut result = HashSet::new();

            let mut todo = Vec::new();
            for root in &self.roots {
                todo.push(root.clone());
            }

            while let Some(next_cell) = todo.pop() {
                if self.nodes.contains_key(&next_cell) && query.should_load(&next_cell) {
                    for child in next_cell.children() {
                        todo.push(child);
                    }
                    result.insert(next_cell);
                }
            }

            result
        }
    }
}

fn update_query(
    camera: &Matrices,
    octree: &mut mini_mno::Octree,
    point_clouds: &mut HashMap<GridCell, PointCloudId>,
    window: &Window,
) -> bool {
    // query
    let query = mini_mno::Query {
        camera: camera.clone(),
        min_screen_point_distance: 2.0,
    };
    let query_result = octree.query(query);

    // which nodes to remove, add and reload
    let existing = point_clouds.keys().cloned().collect::<HashSet<_>>();
    let mut nodes_remove = existing
        .difference(&query_result)
        .cloned()
        .collect::<HashSet<_>>();
    let mut nodes_add = query_result
        .difference(&existing)
        .cloned()
        .collect::<HashSet<_>>();
    let mut limited = false; // never load/remove more then 10 nodes at once, so we get a chance to update the query, if the user navigates around
    if nodes_remove.len() > 10 {
        limited = true;
        nodes_remove = nodes_remove.into_iter().take(10).collect();
    }
    if nodes_add.len() > 10 {
        limited = true;
        nodes_add = nodes_add.into_iter().take(10).collect();
    }
    let nodes_reload = existing
        .intersection(&octree.take_dirty())
        .cloned()
        .collect::<HashSet<_>>()
        .difference(&nodes_remove)
        .cloned()
        .collect::<HashSet<_>>();

    // add new nodes
    for cell in nodes_add {
        let node = octree.get(&cell);

        let point_buffer: VectorBuffer = node.points().into_iter().collect();
        let point_cloud_id = window
            .add_point_cloud_with_attributes(&point_buffer, &[&attributes::CLASSIFICATION])
            .unwrap();
        point_clouds.insert(cell, point_cloud_id);
    }

    // refresh existing nodes
    for cell in nodes_reload {
        let node = octree.get(&cell);

        let point_buffer: VectorBuffer = node.points().into_iter().collect();
        let point_cloud_id = *point_clouds.get(&cell).unwrap();
        window
            .update_point_cloud(
                point_cloud_id,
                &point_buffer,
                &[&attributes::CLASSIFICATION],
            )
            .unwrap();
    }

    // remove old nodes
    for cell in nodes_remove.into_iter() {
        let point_cloud_id = point_clouds.remove(&cell).unwrap();
        window.remove_point_cloud(point_cloud_id).unwrap();
    }

    limited
}

fn main() {
    dotenvy::dotenv().ok();
    pretty_env_logger::init();

    let laz_file_path = std::env::var("INPUT_FILE").expect(
        "Please specify the las/laz file to load by setting the INPUT_FILE environment variable.",
    );

    GliumRenderOptions::default().run(move |render_thread| {
        // open viewer window
        let window = render_thread.open_window().unwrap();
        window
            .set_render_settings(BaseRenderSettings {
                grid: Some(Default::default()),
                enable_edl: true,
                ..Default::default()
            })
            .unwrap();
        window
            .set_default_point_cloud_settings(PointCloudRenderSettings {
                point_color: PointColor::CategoricalAttribute(CategoricalAttributeColoring {
                    attribute: attributes::CLASSIFICATION,
                    color_palette: ColorPalette::las_classification_colors(),
                }),
                point_shape: PointShape::Round,
                point_size: PointSize::Fixed(5.0),
            })
            .unwrap();

        // move camera so the point cloud is visible
        let mut reader = Reader::from_path(laz_file_path).unwrap();
        let bounds = reader.header().bounds();
        let aabb = AABB::from_min_max(
            Point3::new(bounds.min.x, bounds.min.y, bounds.min.z),
            Point3::new(bounds.max.x, bounds.max.y, bounds.max.z),
        );
        window
            .camera_movement()
            .focus_on_bounding_box(aabb)
            .execute()
            .unwrap();

        // we will need access to the camera position for the lod
        let camera_receiver = window.subscribe_to_camera().unwrap();
        let mut camera = camera_receiver.recv().unwrap();

        // octree to insert the points in
        let mut octree = mini_mno::Octree::new();
        let mut point_clouds = HashMap::<GridCell, PointCloudId>::new();

        // read file in chunks of 50_000 points
        let mut points = Vec::with_capacity(50_000);
        for point in reader.points() {
            // read point
            let point = point.unwrap();
            points.push(LasPointFormat0 {
                position: Vector3::new(point.x, point.y, point.z),
                classification: point.classification.into(),
                intensity: point.intensity,
                ..Default::default()
            });

            // at the end of each chunk...
            if points.len() >= 50_000 {
                // add chunk of points to octree
                octree.insert(points);
                points = Vec::with_capacity(50_000);

                // get current camera
                while let Ok(m) = camera_receiver.try_recv() {
                    camera = m;
                }

                // refresh point cloud on screen
                update_query(&camera, &mut octree, &mut point_clouds, &window);
            }
        }

        // insert remaining points
        octree.insert(points);
        update_query(&camera, &mut octree, &mut point_clouds, &window);

        // keep refreshing, whenever the camera changes
        loop {
            camera = match camera_receiver.recv() {
                Ok(c) => c,
                Err(_) => return,
            };

            while update_query(&camera, &mut octree, &mut point_clouds, &window) {
                while let Ok(m) = camera_receiver.try_recv() {
                    camera = m;
                }
            }
        }
    })
}
