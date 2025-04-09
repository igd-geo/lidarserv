use std::{
    collections::{HashMap, hash_map::Entry},
    slice,
};

use log::warn;
use nalgebra::{Point3, Vector3};
use pasture_core::{
    containers::{
        BorrowedBuffer, InterleavedBuffer, InterleavedBufferMut, MakeBufferFromLayout,
        OwningBuffer, VectorBuffer,
    },
    layout::PointLayout,
};

use crate::geometry::position::WithComponentTypeOnce;

use super::{
    grid::{Grid, GridCell, GridHierarchy, LodLevel},
    position::Component,
};

/// Allows to sample a point cloud.
/// E.g. perform grid-center-sampling.
///
///
pub trait Sampling {
    /// Return the number of points, that have been selected by the sampling.
    fn len(&self) -> usize;

    /// Returns true, if the sampling is empty
    fn is_empty(&self) -> bool;

    /// Samples from the given list of points and stores the selected ones.
    /// All points that got rejected from the sampling,
    /// as well as all preexisting points that got replaced by a selected point,
    /// will become bogus points. They can be retrieved using [Self::take_bogus_points].
    /// Sets the dirty bit.
    fn insert(&mut self, points: &VectorBuffer);

    fn insert_multi(&mut self, points: &[VectorBuffer]);

    /// Returns a copy of the points in this node.
    /// (All points - both accepted and bogus!)
    fn clone_points(&self) -> VectorBuffer;

    /// Returns a reference to the points in this node.
    /// (All points - both accepted and bogus!)
    fn points(&self) -> &VectorBuffer;

    /// Reset the dirty bit.
    fn reset_dirty(&mut self);

    /// Return the status of the dirty bit.
    fn is_dirty(&self) -> bool;

    fn nr_bogus_points(&self) -> usize;

    fn take_bogus_points(&mut self) -> VectorBuffer;

    fn dyn_clone(&self) -> Box<dyn Sampling + Send + Sync>;
}

#[derive(Debug, Clone)]
pub struct GridCenterSampling<C: Component> {
    grid: Grid<C>,
    occupation: HashMap<GridCell, PointEntry<C>>,
    points: VectorBuffer,
    dirty: bool,
}

#[derive(Debug, Clone)]
struct PointEntry<C: Component> {
    index: usize,
    distance_to_center: C,
}

impl<C: Component> GridCenterSampling<C> {
    pub fn new(grid: Grid<C>, layout: PointLayout) -> Self {
        assert!(
            layout.has_attribute(&C::position_attribute()),
            "Incompatible point layout."
        );
        let points = VectorBuffer::new_from_layout(layout);
        let occupation = HashMap::new();
        GridCenterSampling {
            grid,
            occupation,
            points,
            dirty: false,
        }
    }
}

pub fn create_sampling(
    hierarchy: GridHierarchy,
    lod: LodLevel,
    point_layout: &PointLayout,
) -> Box<dyn Sampling + Send + Sync> {
    struct Wct {
        hierarchy: GridHierarchy,
        lod: LodLevel,
        point_layout: PointLayout,
    }

    impl WithComponentTypeOnce for Wct {
        type Output = Box<dyn Sampling + Send + Sync>;

        fn run_once<C: Component>(self) -> Self::Output {
            let Self {
                hierarchy,
                lod,
                point_layout,
            } = self;
            let grid = hierarchy.level::<C>(lod);
            Box::new(GridCenterSampling::<C>::new(grid, point_layout))
        }
    }

    Wct {
        hierarchy,
        lod,
        point_layout: point_layout.clone(),
    }
    .for_layout_once(point_layout)
}

pub fn create_sampling_from_points(
    hierarchy: GridHierarchy,
    lod: LodLevel,
    points: VectorBuffer,
    nr_bogus_points: usize,
) -> Box<dyn Sampling + Send + Sync> {
    struct Wct {
        hierarchy: GridHierarchy,
        lod: LodLevel,
        points: VectorBuffer,
        nr_bogus_points: usize,
    }

    impl WithComponentTypeOnce for Wct {
        type Output = Box<dyn Sampling + Send + Sync>;

        fn run_once<C: Component>(self) -> Self::Output {
            let Self {
                hierarchy,
                lod,
                points,
                nr_bogus_points,
            } = self;
            let grid = hierarchy.level::<C>(lod);
            Box::new(GridCenterSampling::<C>::new_from_points(
                grid,
                points,
                nr_bogus_points,
            ))
        }
    }

    let layout = points.point_layout().clone();
    Wct {
        hierarchy,
        lod,
        points,
        nr_bogus_points,
    }
    .for_layout_once(&layout)
}

/// Returns mutable references to two points in a pasture buffer.
///
/// # Safety
///
/// i1 and i2 must not be the same.
#[inline]
unsafe fn pasture_two_references<'a>(
    points: &mut VectorBuffer,
    i1: usize,
    i2: usize,
) -> (&'a mut [u8], &'a mut [u8]) {
    unsafe {
        debug_assert_ne!(i1, i2);
        let ptr1 = points.get_point_mut(i1) as *mut [u8];
        let ptr2 = points.get_point_mut(i2) as *mut [u8];
        (&mut *ptr1, &mut *ptr2)
    }
}

impl<C: Component> Sampling for GridCenterSampling<C> {
    fn len(&self) -> usize {
        self.occupation.len()
    }

    fn is_empty(&self) -> bool {
        self.occupation.is_empty()
    }

    fn insert(&mut self, points: &VectorBuffer) {
        self.insert_multi(slice::from_ref(points))
    }

    fn insert_multi(&mut self, multi_points: &[VectorBuffer]) {
        if multi_points.is_empty() {
            return;
        }
        let mut total_nr_points = 0;
        for points in multi_points {
            total_nr_points += points.len();
            assert_eq!(
                points.point_layout(),
                self.points.point_layout(),
                "Incompatible point layout"
            );
        }
        let position_attr = multi_points
            .first()
            .expect("just tested for emptines")
            .point_layout()
            .get_attribute(&C::position_attribute())
            .expect("Missing position attribute")
            .clone();

        // resize point buffer
        let mut wr_reject = self.points.len();
        let mut wr_accept = self.occupation.len();
        let new_size = self.points.len() + total_nr_points;
        self.points.resize(new_size);

        // insert points
        for points in multi_points {
            for rd in 0..points.len() {
                let bytes_point = points.get_point_ref(rd);

                // get position
                let position = {
                    let bytes_position = &bytes_point[position_attr.byte_range_within_point()];
                    let mut coord = Vector3::zeros();
                    bytemuck::cast_slice_mut::<Vector3<C>, u8>(slice::from_mut(&mut coord))
                        .copy_from_slice(bytes_position);
                    Point3::from(coord)
                };

                // get grid cell
                let cell = self.grid.cell_at(position);
                let cell_aabb = self.grid.cell_bounds(cell);
                let cell_centre = cell_aabb.center().expect("Grid cells can't be empty");
                let dist_to_center = (position - cell_centre).map(|c| c * c).sum();

                // accept or reject
                match self.occupation.entry(cell) {
                    Entry::Occupied(mut e) => {
                        let cur_dist_to_center = e.get().distance_to_center;
                        if dist_to_center < cur_dist_to_center {
                            // CASE 1: new point is closer to center.
                            //  - accept new point
                            //  - reject old point
                            e.get_mut().distance_to_center = dist_to_center;
                            let index = e.get().index;
                            let (bytes_rejected, bytes_accepted) = unsafe {
                                // safety:
                                // We have index < wr_reject,
                                // because index refers to an already initialized point
                                // in the buffer and wr_reject points to the first point
                                // in the buffer that is still uninitialized.
                                // Therefore, both refer to different memory regions.
                                pasture_two_references(&mut self.points, wr_reject, index)
                            };
                            bytes_rejected.copy_from_slice(bytes_accepted);
                            bytes_accepted.copy_from_slice(bytes_point);
                            wr_reject += 1;
                            self.dirty = true;
                        } else {
                            // CASE 2: current point is closer to center.
                            //  - reject new point
                            let bytes_reject = self.points.get_point_mut(wr_reject);
                            bytes_reject.copy_from_slice(bytes_point);
                            wr_reject += 1;
                        }
                    }
                    Entry::Vacant(e) => {
                        // CASE 3: first point in a new cell
                        //  - accept point into that cell
                        e.insert(PointEntry {
                            index: wr_accept,
                            distance_to_center: dist_to_center,
                        });
                        if wr_accept != wr_reject {
                            // This is safe, because since `wr_accept != wr_reject`, they refer to different
                            // memory regions.
                            let (bytes_accept, bytes_reject) = unsafe {
                                pasture_two_references(&mut self.points, wr_accept, wr_reject)
                            };
                            bytes_reject.copy_from_slice(bytes_accept);
                            bytes_accept.copy_from_slice(bytes_point);
                        } else {
                            let bytes_accept = self.points.get_point_mut(wr_accept);
                            bytes_accept.copy_from_slice(bytes_point);
                        }
                        wr_accept += 1;
                        wr_reject += 1;
                        self.dirty = true;
                    }
                }
            }
        }
    }

    fn clone_points(&self) -> VectorBuffer {
        self.points.clone()
    }

    fn points(&self) -> &VectorBuffer {
        &self.points
    }

    fn reset_dirty(&mut self) {
        self.dirty = false;
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn nr_bogus_points(&self) -> usize {
        self.points.len() - self.occupation.len()
    }

    fn take_bogus_points(&mut self) -> VectorBuffer {
        let mut result =
            VectorBuffer::with_capacity(self.nr_bogus_points(), self.points.point_layout().clone());
        let bogus_start = self.occupation.len();
        let bogus_end = self.points.len();
        let bogus = self.points.get_point_range_ref(bogus_start..bogus_end);
        unsafe {
            // unsafe: safe, because point layouts are identical.
            result.push_points(bogus)
        };
        self.points.resize(bogus_start);
        result
    }

    fn dyn_clone(&self) -> Box<dyn Sampling + Send + Sync> {
        Box::new(self.clone())
    }
}

impl<C: Component> GridCenterSampling<C> {
    pub fn new_from_points(grid: Grid<C>, points: VectorBuffer, nr_bogus_points: usize) -> Self {
        let position_attr = points
            .point_layout()
            .get_attribute(&C::position_attribute())
            .expect("Missing position attribute")
            .clone();

        let mut nr_bogus_points = nr_bogus_points;
        if nr_bogus_points > points.len() {
            warn!("Too many bogus points in node.");
            nr_bogus_points = points.len();
        }
        let mut nr_accepted = points.len() - nr_bogus_points;

        let mut result = GridCenterSampling {
            grid,
            occupation: HashMap::with_capacity(nr_accepted),
            points,
            dirty: false,
        };

        let mut index = 0;
        while index < nr_accepted {
            let bytes_point = result.points.get_point_ref(index);

            // get position
            let position = {
                let bytes_position = &bytes_point[position_attr.byte_range_within_point()];
                let mut coord = Vector3::zeros();
                bytemuck::cast_slice_mut::<Vector3<C>, u8>(slice::from_mut(&mut coord))
                    .copy_from_slice(bytes_position);
                Point3::from(coord)
            };

            // get grid cell
            let cell = result.grid.cell_at(position);
            let cell_aabb = result.grid.cell_bounds(cell);
            let cell_centre = cell_aabb.center().expect("Grid cells can't be empty");
            let distance_to_center = (position - cell_centre).map(|c| c * c).sum();

            // insert
            match result.occupation.entry(cell) {
                // this should be the normal case for previously saved nodes
                Entry::Vacant(v) => {
                    v.insert(PointEntry {
                        index,
                        distance_to_center,
                    });
                    index += 1;
                }

                // this is basically error handling
                Entry::Occupied(o) => {
                    let o = o.into_mut();

                    // swap the one that is closer to the center to the
                    // accepted position
                    if o.distance_to_center > distance_to_center {
                        o.distance_to_center = distance_to_center;
                        {
                            let index_accepted = o.index;
                            let (bytes_swap_1, bytes_swap_2) = unsafe {
                                // safety:
                                // we have `index_accepted < index`
                                pasture_two_references(&mut result.points, index_accepted, index)
                            };
                            bytes_swap_1.swap_with_slice(bytes_swap_2);
                        }
                    }

                    // swap the other one to a rejected position
                    {
                        let index_rejected = nr_accepted - 1;
                        if index != index_rejected {
                            let (bytes_swap_1, bytes_swap_2) = unsafe {
                                // safety: Since we have `index != swap_index`, they refer to different points.
                                pasture_two_references(&mut result.points, index_rejected, index)
                            };
                            bytes_swap_1.swap_with_slice(bytes_swap_2);
                        }
                    }

                    // this turened a 'normal' point into a bogus point.
                    nr_accepted -= 1;
                    nr_bogus_points += 1;

                    // mark dirty
                    result.dirty = true;
                }
            }
        }

        if result.dirty {
            warn!("Multiple points per cell encountered in node.")
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::{
        grid::{GridHierarchy, LodLevel},
        sampling::{GridCenterSampling, Sampling},
        test::{F64Point, I32Point},
    };
    use nalgebra::vector;
    use pasture_core::{containers::VectorBuffer, layout::PointType};

    #[test]
    fn test_sampling_f64() {
        // create sampling
        let grid = GridHierarchy::new(0).level::<f64>(LodLevel::base());
        assert_eq!(grid.cell_size(), 1.0);
        let layout = F64Point::layout();
        let mut sampling = GridCenterSampling::new(grid, layout);
        assert!(!sampling.is_dirty());

        // sample!
        let points: VectorBuffer = [
            F64Point {
                position: vector![1.4, 2.0, 2.0],
            },
            F64Point {
                position: vector![2.1, 2.0, 2.0],
            },
            F64Point {
                position: vector![3.3, 2.0, 2.0],
            },
            F64Point {
                position: vector![2.5, 2.0, 2.0],
            },
            F64Point {
                position: vector![1.1, 2.0, 2.0],
            },
        ]
        .into_iter()
        .collect();
        sampling.insert(&points);
        let rejected = sampling.take_bogus_points();
        assert!(sampling.is_dirty());

        // check result
        let expected: VectorBuffer = [
            F64Point {
                position: vector![1.4, 2.0, 2.0],
            },
            F64Point {
                position: vector![2.5, 2.0, 2.0],
            },
            F64Point {
                position: vector![3.3, 2.0, 2.0],
            },
        ]
        .into_iter()
        .collect();
        let expected_rejected: VectorBuffer = [
            F64Point {
                position: vector![2.1, 2.0, 2.0],
            },
            F64Point {
                position: vector![1.1, 2.0, 2.0],
            },
        ]
        .into_iter()
        .collect();
        assert_eq!(sampling.clone_points(), expected);
        assert_eq!(rejected, expected_rejected);
    }

    #[test]
    fn test_sampling_i32() {
        // create sampling
        let grid = GridHierarchy::new(3).level::<i32>(LodLevel::base());
        assert_eq!(grid.cell_size(), 8);
        let layout = I32Point::layout();
        let mut sampling = GridCenterSampling::new(grid, layout);
        assert!(!sampling.is_dirty());

        // sample!
        let points: VectorBuffer = [
            I32Point {
                position: vector![1, 1, 1],
            },
            I32Point {
                position: vector![12, 1, 1],
            },
            I32Point {
                position: vector![16, 1, 1],
            },
            I32Point {
                position: vector![10, 1, 1],
            },
            I32Point {
                position: vector![4, 1, 1],
            },
        ]
        .into_iter()
        .collect();
        sampling.insert(&points);
        let rejected = sampling.take_bogus_points();
        assert!(sampling.is_dirty());

        // check result
        let expected: VectorBuffer = [
            I32Point {
                position: vector![4, 1, 1],
            },
            I32Point {
                position: vector![12, 1, 1],
            },
            I32Point {
                position: vector![16, 1, 1],
            },
        ]
        .into_iter()
        .collect();
        let expected_rejected: VectorBuffer = [
            I32Point {
                position: vector![10, 1, 1],
            },
            I32Point {
                position: vector![1, 1, 1],
            },
        ]
        .into_iter()
        .collect();
        assert_eq!(sampling.clone_points(), expected);
        assert_eq!(rejected, expected_rejected);
    }
}
