//! rhymes with "mate tea"

use crate::geometry::bounding_box::AABB;
use crate::geometry::grid::{GridCell, GridHierarchy, LeveledGridCell, LodLevel};
use crate::geometry::position::{Component, Position};
use crate::index::sensor_pos::page_manager::FileId;
use crate::nalgebra::Scalar;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fs::File;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct MetaTree<GridH, Comp: Scalar> {
    sensor_grid_hierarchy: GridH,
    lods: Vec<MetaTreeLod<Comp>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetaTreeLod<Comp: Scalar> {
    depth: Vec<HashMap<GridCell, Node<Comp>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node<Comp: Scalar> {
    is_leaf: bool,
    bounds: AABB<Comp>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetaTreeNodeId {
    lod: LodLevel,
    node: LeveledGridCell,
}

#[derive(Debug, Clone)]
pub struct SensorPositionQueryResult<Comp: Scalar> {
    min_bounds: AABB<Comp>,
    fallback_node: LeveledGridCell,
    node_ids: Vec<LeveledGridCell>,
}

#[derive(Debug, Error)]
pub enum MetaTreeIoError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Invalid file format")]
    SerDe(#[source] Box<dyn StdError + Send + Sync>),
}

impl<GridH, Comp: Scalar> MetaTree<GridH, Comp> {
    pub fn new(sensor_grid_hierarchy: GridH) -> Self {
        MetaTree {
            sensor_grid_hierarchy,
            lods: vec![],
        }
    }

    pub fn nodes(&self) -> impl Iterator<Item = MetaTreeNodeId> + '_ {
        self.lods
            .iter()
            .enumerate()
            .flat_map(|(lod_level, subtree)| {
                let lod = LodLevel::from_level(lod_level as u16);
                subtree
                    .depth
                    .iter()
                    .enumerate()
                    .flat_map(move |(depth_level, subsubtree)| {
                        let depth = LodLevel::from_level(depth_level as u16);
                        subsubtree.iter().filter(|(_, node)| node.is_leaf).map(
                            move |(grid_cell, _)| MetaTreeNodeId {
                                lod,
                                node: LeveledGridCell {
                                    lod: depth,
                                    pos: *grid_cell,
                                },
                            },
                        )
                    })
            })
    }
}

impl<GridH, Comp> MetaTree<GridH, Comp>
where
    Comp: Scalar + Serialize + DeserializeOwned,
{
    pub fn write_to_file(&self, file_name: &Path) -> Result<(), MetaTreeIoError> {
        let file = File::create(file_name)?;

        ciborium::ser::into_writer(&self.lods, file)
            .map_err(|e| MetaTreeIoError::SerDe(Box::new(e)))?;
        Ok(())
    }

    pub fn load_from_file(
        file_name: &Path,
        sensor_grid_hierarchy: GridH,
    ) -> Result<Self, MetaTreeIoError> {
        if !file_name.exists() {
            return Ok(Self::new(sensor_grid_hierarchy));
        }

        let file = File::open(file_name)?;
        let lods =
            ciborium::de::from_reader(file).map_err(|e| MetaTreeIoError::SerDe(Box::new(e)))?;

        Ok(MetaTree {
            sensor_grid_hierarchy,
            lods,
        })
    }
}

impl<GridH, Comp, Pos> MetaTree<GridH, Comp>
where
    Comp: Component,
    GridH: GridHierarchy<Component = Comp, Position = Pos>,
    Pos: Position<Component = Comp>,
{
    pub fn node_center(&self, node: &MetaTreeNodeId) -> Pos where {
        let aabb = self
            .sensor_grid_hierarchy
            .get_leveled_cell_bounds(&node.node);
        aabb.center()
    }

    /// Queries, which nodes should be loaded for a given sensor position.
    pub fn query_sensor_position(&self, sensor_pos: &Pos) -> SensorPositionQueryResult<Comp> {
        // Node to start inserting points into, if a lod is completely empty.
        let lod0 = LodLevel::base();

        // Make a sensor position query for each existing LOD
        let node_ids: Vec<LeveledGridCell> = self
            .lods
            .iter()
            .map(|meta_tree_lod| {
                meta_tree_lod.query_sensor_position(sensor_pos, &self.sensor_grid_hierarchy)
            })
            .collect();

        // Find the node where we went deepest into the tree.
        // This will be the one with the smallest bounding box, that limits the bounds of the query
        // result the most.
        let fallback_node = self
            .sensor_grid_hierarchy
            .level(&lod0)
            .leveled_cell_at(sensor_pos);
        let mut min_bounds_node = fallback_node;
        for node in &node_ids {
            if node.lod > min_bounds_node.lod {
                min_bounds_node = *node
            }
        }

        // bounds of this node = min bounds of query
        let min_bounds = self
            .sensor_grid_hierarchy
            .get_leveled_cell_bounds(&min_bounds_node);

        SensorPositionQueryResult {
            fallback_node,
            node_ids,
            min_bounds,
        }
    }

    pub fn split_node(&mut self, node: &MetaTreeNodeId) {
        let lod_level = node.lod.level() as usize;
        let depth = node.node.lod.level() as usize;
        self.lods[lod_level].depth[depth]
            .get_mut(&node.node.pos)
            .unwrap()
            .is_leaf = false;
    }

    pub fn set_node_aabb(&mut self, node: &MetaTreeNodeId, aabb: &AABB<Comp>) {
        let lod_level = node.lod.level() as usize;
        let depth_level = node.node.lod.level() as usize;
        while lod_level >= self.lods.len() {
            self.lods.push(MetaTreeLod { depth: vec![] })
        }
        let lod = &mut self.lods[lod_level];
        while depth_level >= lod.depth.len() {
            lod.depth.push(HashMap::new());
        }
        let depth = &mut lod.depth[depth_level];
        match depth.entry(node.node.pos) {
            Entry::Occupied(mut o) => o.get_mut().bounds.extend_union(aabb),
            Entry::Vacant(v) => {
                v.insert(Node {
                    is_leaf: true,
                    bounds: aabb.clone(),
                });
            }
        }
        let mut lod_node = node.node;
        while let Some(parent) = lod_node.parent() {
            lod_node = parent;
            let depth = &mut lod.depth[lod_node.lod.level() as usize];
            match depth.entry(lod_node.pos) {
                Entry::Occupied(mut o) => {
                    o.get_mut().bounds.extend_union(aabb);
                    o.get_mut().is_leaf = false;
                }
                Entry::Vacant(v) => {
                    v.insert(Node {
                        is_leaf: false,
                        bounds: aabb.clone(),
                    });
                }
            }
        }
    }
}

impl<Comp> MetaTreeLod<Comp>
where
    Comp: Component,
{
    fn query_sensor_position<Pos, GridH>(
        &self,
        sensor_pos: &Pos,
        sensor_grid_hierarchy: &GridH,
    ) -> LeveledGridCell
    where
        Pos: Position<Component = Comp>,
        GridH: GridHierarchy<Component = Comp, Position = Pos>,
    {
        for (depth, nodes) in self.depth.iter().enumerate() {
            let cell = sensor_grid_hierarchy
                .level(&LodLevel::from_level(depth as u16))
                .leveled_cell_at(sensor_pos);
            match nodes.get(&cell.pos) {
                None => return cell,
                Some(Node { is_leaf: true, .. }) => return cell,
                Some(Node { is_leaf: false, .. }) => (),
            }
        }

        sensor_grid_hierarchy
            .level(&LodLevel::from_level(self.depth.len() as u16))
            .leveled_cell_at(sensor_pos)
    }
}

impl<Comp> SensorPositionQueryResult<Comp>
where
    Comp: Component,
{
    /// Returns the bounds for the sensor position that this query result is valid for.
    /// This query result can be used, as long as the sensor stays within this bounding box.
    #[inline]
    pub fn min_bounds(&self) -> &AABB<Comp> {
        &self.min_bounds
    }

    pub fn node_for_lod(&self, lod: &LodLevel) -> MetaTreeNodeId {
        let level = lod.level() as usize;
        let node = if level < self.node_ids.len() {
            self.node_ids[level]
        } else {
            self.fallback_node
        };
        MetaTreeNodeId { lod: *lod, node }
    }
}

impl MetaTreeNodeId {
    pub fn file(&self, thread_id: usize) -> FileId {
        FileId {
            lod: self.lod,
            tree_depth: self.node.lod,
            grid_cell: self.node.pos,
            thread_index: thread_id,
        }
    }

    pub fn children(&self) -> [MetaTreeNodeId; 8] {
        self.node.children().map(|node| MetaTreeNodeId {
            lod: self.lod,
            node,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::bounding_box::{BaseAABB, OptionAABB, AABB};
    use crate::geometry::grid::{
        F64GridHierarchy, GridCell, GridHierarchy, LeveledGridCell, LodLevel,
    };
    use crate::geometry::position::F64Position;
    use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeLod, MetaTreeNodeId, Node};
    use nalgebra::Point3;
    use std::collections::HashMap;
    use std::iter::FromIterator;

    #[test]
    fn query() {
        let node = Node {
            is_leaf: false,
            bounds: OptionAABB::new(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 1.0))
                .into_aabb()
                .unwrap(),
        };

        let t = MetaTreeLod {
            depth: vec![
                HashMap::from_iter([
                    (
                        GridCell { x: 0, y: 0, z: 0 },
                        Node {
                            is_leaf: false,
                            ..node.clone()
                        },
                    ),
                    (
                        GridCell { x: 1, y: 0, z: 0 },
                        Node {
                            is_leaf: true,
                            ..node.clone()
                        },
                    ),
                ]),
                HashMap::from_iter([
                    (
                        GridCell { x: 0, y: 0, z: 0 },
                        Node {
                            is_leaf: false,
                            ..node.clone()
                        },
                    ),
                    (
                        GridCell { x: 1, y: 0, z: 0 },
                        Node {
                            is_leaf: true,
                            ..node.clone()
                        },
                    ),
                ]),
            ],
        };

        let grid_hierarchy = F64GridHierarchy::new(1.0);
        let p1: F64Position = grid_hierarchy
            .get_leveled_cell_bounds(&LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 2, y: 0, z: 0 },
            })
            .center();
        let p2: F64Position = grid_hierarchy
            .get_leveled_cell_bounds(&LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 1, y: 0, z: 0 },
            })
            .center();
        let p3: F64Position = grid_hierarchy
            .get_leveled_cell_bounds(&LeveledGridCell {
                lod: LodLevel::from_level(1),
                pos: GridCell { x: 0, y: 1, z: 0 },
            })
            .center();
        let p4: F64Position = grid_hierarchy
            .get_leveled_cell_bounds(&LeveledGridCell {
                lod: LodLevel::from_level(1),
                pos: GridCell { x: 1, y: 0, z: 0 },
            })
            .center();
        let p5: F64Position = grid_hierarchy
            .get_leveled_cell_bounds(&LeveledGridCell {
                lod: LodLevel::from_level(2),
                pos: GridCell { x: 0, y: 0, z: 0 },
            })
            .center();

        assert_eq!(
            t.query_sensor_position(&p1, &grid_hierarchy),
            LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 2, y: 0, z: 0 },
            }
        );
        assert_eq!(
            t.query_sensor_position(&p2, &grid_hierarchy),
            LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 1, y: 0, z: 0 },
            }
        );
        assert_eq!(
            t.query_sensor_position(&p3, &grid_hierarchy),
            LeveledGridCell {
                lod: LodLevel::from_level(1),
                pos: GridCell { x: 0, y: 1, z: 0 },
            }
        );
        assert_eq!(
            t.query_sensor_position(&p4, &grid_hierarchy),
            LeveledGridCell {
                lod: LodLevel::from_level(1),
                pos: GridCell { x: 1, y: 0, z: 0 },
            }
        );
        assert_eq!(
            t.query_sensor_position(&p5, &grid_hierarchy),
            LeveledGridCell {
                lod: LodLevel::from_level(2),
                pos: GridCell { x: 0, y: 0, z: 0 },
            }
        );
    }

    #[test]
    fn set_node_aabb() {
        let mut t = MetaTree {
            sensor_grid_hierarchy: F64GridHierarchy::new(1.0),
            lods: vec![],
        };
        t.set_node_aabb(
            &MetaTreeNodeId {
                lod: LodLevel::base(),
                node: LeveledGridCell {
                    lod: LodLevel::from_level(3),
                    pos: GridCell { x: 5, y: 6, z: 9 },
                },
            },
            &OptionAABB::new(Point3::new(1.0, 1.0, 1.0), Point3::new(4.0, 4.0, 4.0))
                .into_aabb()
                .unwrap(),
        );
        assert_eq!(
            t.lods,
            vec![MetaTreeLod {
                depth: vec![
                    HashMap::from_iter([(
                        GridCell { x: 0, y: 0, z: 1 },
                        Node {
                            is_leaf: false,
                            bounds: OptionAABB::new(
                                Point3::new(1.0, 1.0, 1.0),
                                Point3::new(4.0, 4.0, 4.0)
                            )
                            .into_aabb()
                            .unwrap()
                        }
                    )]),
                    HashMap::from_iter([(
                        GridCell { x: 1, y: 1, z: 2 },
                        Node {
                            is_leaf: false,
                            bounds: OptionAABB::new(
                                Point3::new(1.0, 1.0, 1.0),
                                Point3::new(4.0, 4.0, 4.0)
                            )
                            .into_aabb()
                            .unwrap()
                        }
                    )]),
                    HashMap::from_iter([(
                        GridCell { x: 2, y: 3, z: 4 },
                        Node {
                            is_leaf: false,
                            bounds: OptionAABB::new(
                                Point3::new(1.0, 1.0, 1.0),
                                Point3::new(4.0, 4.0, 4.0)
                            )
                            .into_aabb()
                            .unwrap()
                        }
                    )]),
                    HashMap::from_iter([(
                        GridCell { x: 5, y: 6, z: 9 },
                        Node {
                            is_leaf: true,
                            bounds: OptionAABB::new(
                                Point3::new(1.0, 1.0, 1.0),
                                Point3::new(4.0, 4.0, 4.0)
                            )
                            .into_aabb()
                            .unwrap()
                        }
                    )]),
                ]
            }]
        );

        t.set_node_aabb(
            &MetaTreeNodeId {
                lod: LodLevel::base(),
                node: LeveledGridCell {
                    lod: LodLevel::from_level(3),
                    pos: GridCell { x: 7, y: 4, z: 9 },
                },
            },
            &OptionAABB::new(Point3::new(2.0, 2.0, 2.0), Point3::new(5.0, 5.0, 5.0))
                .into_aabb()
                .unwrap(),
        );

        assert_eq!(
            t.lods,
            vec![MetaTreeLod {
                depth: vec![
                    HashMap::from_iter([(
                        GridCell { x: 0, y: 0, z: 1 },
                        Node {
                            is_leaf: false,
                            bounds: OptionAABB::new(
                                Point3::new(1.0, 1.0, 1.0),
                                Point3::new(5.0, 5.0, 5.0)
                            )
                            .into_aabb()
                            .unwrap()
                        }
                    )]),
                    HashMap::from_iter([(
                        GridCell { x: 1, y: 1, z: 2 },
                        Node {
                            is_leaf: false,
                            bounds: OptionAABB::new(
                                Point3::new(1.0, 1.0, 1.0),
                                Point3::new(5.0, 5.0, 5.0)
                            )
                            .into_aabb()
                            .unwrap()
                        }
                    ),]),
                    HashMap::from_iter([
                        (
                            GridCell { x: 2, y: 3, z: 4 },
                            Node {
                                is_leaf: false,
                                bounds: OptionAABB::new(
                                    Point3::new(1.0, 1.0, 1.0),
                                    Point3::new(4.0, 4.0, 4.0)
                                )
                                .into_aabb()
                                .unwrap()
                            }
                        ),
                        (
                            GridCell { x: 3, y: 2, z: 4 },
                            Node {
                                is_leaf: false,
                                bounds: OptionAABB::new(
                                    Point3::new(2.0, 2.0, 2.0),
                                    Point3::new(5.0, 5.0, 5.0)
                                )
                                .into_aabb()
                                .unwrap()
                            }
                        )
                    ]),
                    HashMap::from_iter([
                        (
                            GridCell { x: 5, y: 6, z: 9 },
                            Node {
                                is_leaf: true,
                                bounds: OptionAABB::new(
                                    Point3::new(1.0, 1.0, 1.0),
                                    Point3::new(4.0, 4.0, 4.0)
                                )
                                .into_aabb()
                                .unwrap()
                            }
                        ),
                        (
                            GridCell { x: 7, y: 4, z: 9 },
                            Node {
                                is_leaf: true,
                                bounds: OptionAABB::new(
                                    Point3::new(2.0, 2.0, 2.0),
                                    Point3::new(5.0, 5.0, 5.0)
                                )
                                .into_aabb()
                                .unwrap()
                            }
                        )
                    ]),
                ]
            }]
        );
    }
}
