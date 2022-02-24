//! rhymes with "mate tea"

use crate::geometry::bounding_box::AABB;
use crate::geometry::grid::{GridCell, I32GridHierarchy, LeveledGridCell, LodLevel};
use crate::geometry::position::I32Position;
use crate::index::sensor_pos::{Replacement, Update};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::{Debug, Formatter};
use std::fs::File;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct MetaTree {
    sensor_grid_hierarchy: I32GridHierarchy,
    lods: Vec<MetaTreeLevel>,
}

pub struct MetaTreePart {
    lod: LodLevel,
    sensor_grid_hierarchy: I32GridHierarchy,
    tree: MetaTreeLevel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MetaTreeLevel {
    depth: Vec<HashMap<GridCell, Node>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub is_leaf: bool,
    pub bounds: AABB<i32>,
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct MetaTreeNodeId {
    lod: LodLevel,
    node: LeveledGridCell,
}

#[derive(Debug, Clone)]
pub struct SensorPositionQueryResult {
    min_bounds: AABB<i32>,
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

impl MetaTree {
    pub fn new(sensor_grid_hierarchy: I32GridHierarchy) -> Self {
        MetaTree {
            sensor_grid_hierarchy,
            lods: vec![],
        }
    }

    pub fn split_into_parts(self, min_nr_parts: usize) -> Vec<MetaTreePart> {
        // turn existing LODs into parts
        let mut parts = self
            .lods
            .into_iter()
            .enumerate()
            .map(|(lod_level, lod_tree)| MetaTreePart {
                lod: LodLevel::from_level(lod_level as u16),
                sensor_grid_hierarchy: self.sensor_grid_hierarchy.clone(),
                tree: lod_tree,
            })
            .collect::<Vec<_>>();

        // fill up with empty parts until min_nr_parts is reached
        while parts.len() < min_nr_parts {
            parts.push(MetaTreePart {
                lod: LodLevel::from_level(parts.len() as u16),
                sensor_grid_hierarchy: self.sensor_grid_hierarchy.clone(),
                tree: MetaTreeLevel { depth: vec![] },
            })
        }
        parts
    }

    pub fn recombine_parts(parts: Vec<MetaTreePart>) -> Self {
        assert!(!parts.is_empty());
        let sensor_grid_hierarchy = parts[0].sensor_grid_hierarchy.clone();

        MetaTree {
            lods: parts
                .into_iter()
                .enumerate()
                .map(|(idx, p)| {
                    assert_eq!(p.lod.level() as usize, idx);
                    assert_eq!(p.sensor_grid_hierarchy, sensor_grid_hierarchy);
                    p.tree
                })
                .collect(),
            sensor_grid_hierarchy,
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

    pub fn root_nodes_for_lod(&self, lod: &LodLevel) -> impl Iterator<Item = MetaTreeNodeId> + '_ {
        let lod = *lod;
        self.lods
            .get(lod.level() as usize)
            .into_iter()
            .map(move |lod_nodes| {
                if lod_nodes.depth.is_empty() {
                    None
                } else {
                    Some(
                        lod_nodes.depth[0]
                            .iter()
                            .map(move |(pos, _)| MetaTreeNodeId {
                                lod,
                                node: LeveledGridCell {
                                    lod: LodLevel::base(),
                                    pos: *pos,
                                },
                            }),
                    )
                }
            })
            .flatten()
            .flatten()
    }

    pub fn get(&self, node_id: &MetaTreeNodeId) -> Option<&Node> {
        let lod = match self.lods.get(node_id.lod.level() as usize) {
            None => return None,
            Some(v) => v,
        };
        let depth = match lod.depth.get(node_id.node.lod.level() as usize) {
            None => return None,
            Some(v) => v,
        };
        depth.get(&node_id.node.pos)
    }

    pub fn sensor_grid_hierarchy(&self) -> &I32GridHierarchy {
        &self.sensor_grid_hierarchy
    }

    pub fn write_to_file(&self, file_name: &Path) -> Result<(), MetaTreeIoError> {
        let file = File::create(file_name)?;

        ciborium::ser::into_writer(&self.lods, file)
            .map_err(|e| MetaTreeIoError::SerDe(Box::new(e)))?;
        Ok(())
    }

    pub fn load_from_file(
        file_name: &Path,
        sensor_grid_hierarchy: I32GridHierarchy,
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

    pub fn node_center(&self, node: &MetaTreeNodeId) -> I32Position where {
        let aabb = self
            .sensor_grid_hierarchy
            .get_leveled_cell_bounds(&node.node);
        aabb.center()
    }

    /// Queries, which nodes should be loaded for a given sensor position.
    pub fn query_sensor_position(
        &self,
        sensor_pos: &I32Position,
        previous_split_levels: &[LodLevel],
    ) -> SensorPositionQueryResult {
        // Node to start inserting points into, if a lod is completely empty.
        let lod0 = LodLevel::base();

        // Make a sensor position query for each existing LOD
        let node_ids: Vec<LeveledGridCell> = self
            .lods
            .iter()
            .enumerate()
            .map(|(lod_index, meta_tree_lod)| {
                meta_tree_lod.query_sensor_position(
                    sensor_pos,
                    &self.sensor_grid_hierarchy,
                    &previous_split_levels
                        .get(lod_index)
                        .cloned()
                        .unwrap_or_else(LodLevel::base)
                        .coarser()
                        .unwrap_or_else(LodLevel::base),
                )
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

    pub fn set_node_aabb(&mut self, node: &MetaTreeNodeId, aabb: &AABB<i32>) {
        let lod_level = node.lod.level() as usize;
        while lod_level >= self.lods.len() {
            self.lods.push(MetaTreeLevel { depth: vec![] })
        }
        let lod = &mut self.lods[lod_level];
        lod.set_node_aabb(&node.node, aabb);
    }

    pub(super) fn apply_update(&mut self, update: &Update) {
        for Replacement {
            replace_with,
            bounds,
            ..
        } in &update.replaced_by
        {
            self.set_node_aabb(replace_with, bounds);
        }
    }
}

impl MetaTreeLevel {
    fn query_sensor_position(
        &self,
        sensor_pos: &I32Position,
        sensor_grid_hierarchy: &I32GridHierarchy,
        min_level: &LodLevel,
    ) -> LeveledGridCell {
        for (depth, nodes) in self.depth.iter().enumerate() {
            let cell = sensor_grid_hierarchy
                .level(&LodLevel::from_level(depth as u16))
                .leveled_cell_at(sensor_pos);
            match nodes.get(&cell.pos) {
                None => {
                    return if cell.lod >= *min_level {
                        cell
                    } else {
                        sensor_grid_hierarchy
                            .level(min_level)
                            .leveled_cell_at(sensor_pos)
                    }
                }
                Some(Node { is_leaf: true, .. }) => return cell,
                Some(Node { is_leaf: false, .. }) => (),
            }
        }

        sensor_grid_hierarchy
            .level(&LodLevel::from_level(self.depth.len() as u16))
            .leveled_cell_at(sensor_pos)
    }

    pub fn set_node_aabb(&mut self, node: &LeveledGridCell, aabb: &AABB<i32>) {
        // set/extend aabbb of node itself
        let depth_level = node.lod.level() as usize;
        while depth_level >= self.depth.len() {
            self.depth.push(HashMap::new());
        }
        let depth = &mut self.depth[depth_level];
        match depth.entry(node.pos) {
            Entry::Occupied(mut o) => o.get_mut().bounds.extend_union(aabb),
            Entry::Vacant(v) => {
                v.insert(Node {
                    is_leaf: true,
                    bounds: aabb.clone(),
                });
            }
        }

        // set/extend aabb of parents
        let mut node = *node;
        while let Some(parent) = node.parent() {
            node = parent;
            let depth = &mut self.depth[node.lod.level() as usize];
            match depth.entry(node.pos) {
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

impl MetaTreePart {
    pub fn query_sensor_position(
        &self,
        sensor_pos: &I32Position,
        previous_node: Option<&MetaTreeNodeId>,
    ) -> MetaTreeNodeId {
        let node = self.tree.query_sensor_position(
            sensor_pos,
            &self.sensor_grid_hierarchy,
            &previous_node
                .map(|n| n.node.lod.coarser())
                .flatten()
                .unwrap_or_else(LodLevel::base),
        );

        MetaTreeNodeId {
            lod: self.lod,
            node,
        }
    }

    pub fn set_node_aabb(&mut self, node: &MetaTreeNodeId, aabb: &AABB<i32>) {
        assert_eq!(node.lod, self.lod);
        self.tree.set_node_aabb(&node.node, aabb);
    }

    pub fn sensor_pos_hierarchy(&self) -> &I32GridHierarchy {
        &self.sensor_grid_hierarchy
    }
}

impl SensorPositionQueryResult {
    /// Returns the bounds for the sensor position that this query result is valid for.
    /// This query result can be used, as long as the sensor stays within this bounding box.
    #[inline]
    pub fn min_bounds(&self) -> &AABB<i32> {
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

    pub fn split_levels(&self) -> Vec<LodLevel> {
        self.node_ids.iter().map(|node_id| node_id.lod).collect()
    }
}

impl MetaTreeNodeId {
    pub fn children(&self) -> [MetaTreeNodeId; 8] {
        self.node.children().map(|node| MetaTreeNodeId {
            lod: self.lod,
            node,
        })
    }

    pub fn parent(&self) -> Option<MetaTreeNodeId> {
        self.node.parent().map(|node| MetaTreeNodeId {
            lod: self.lod,
            node,
        })
    }

    pub fn lod(&self) -> &LodLevel {
        &self.lod
    }

    pub fn tree_depth(&self) -> &LodLevel {
        &self.tree_node().lod
    }

    pub fn grid_cell(&self) -> &GridCell {
        &self.tree_node().pos
    }

    pub fn tree_node(&self) -> &LeveledGridCell {
        &self.node
    }

    pub fn with_lod(self, lod: LodLevel) -> Self {
        MetaTreeNodeId {
            lod,
            node: self.node,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
    use crate::geometry::grid::{GridCell, I32GridHierarchy, LeveledGridCell, LodLevel};
    use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeLevel, MetaTreeNodeId, Node};
    use nalgebra::Point3;
    use std::collections::HashMap;
    use std::iter::FromIterator;

    #[test]
    fn query() {
        let node = Node {
            is_leaf: false,
            bounds: OptionAABB::new(Point3::new(0, 0, 0), Point3::new(1, 1, 1))
                .into_aabb()
                .unwrap(),
        };

        let t = MetaTreeLevel {
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

        let grid_hierarchy = I32GridHierarchy::new(0);
        let p1 = grid_hierarchy
            .get_leveled_cell_bounds(&LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 2, y: 0, z: 0 },
            })
            .center();
        let p2 = grid_hierarchy
            .get_leveled_cell_bounds(&LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 1, y: 0, z: 0 },
            })
            .center();
        let p3 = grid_hierarchy
            .get_leveled_cell_bounds(&LeveledGridCell {
                lod: LodLevel::from_level(1),
                pos: GridCell { x: 0, y: 1, z: 0 },
            })
            .center();
        let p4 = grid_hierarchy
            .get_leveled_cell_bounds(&LeveledGridCell {
                lod: LodLevel::from_level(1),
                pos: GridCell { x: 1, y: 0, z: 0 },
            })
            .center();
        let p5 = grid_hierarchy
            .get_leveled_cell_bounds(&LeveledGridCell {
                lod: LodLevel::from_level(2),
                pos: GridCell { x: 0, y: 0, z: 0 },
            })
            .center();

        assert_eq!(
            t.query_sensor_position(&p1, &grid_hierarchy, &LodLevel::base()),
            LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 2, y: 0, z: 0 },
            }
        );
        assert_eq!(
            t.query_sensor_position(&p2, &grid_hierarchy, &LodLevel::base()),
            LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 1, y: 0, z: 0 },
            }
        );
        assert_eq!(
            t.query_sensor_position(&p3, &grid_hierarchy, &LodLevel::base()),
            LeveledGridCell {
                lod: LodLevel::from_level(1),
                pos: GridCell { x: 0, y: 1, z: 0 },
            }
        );
        assert_eq!(
            t.query_sensor_position(&p4, &grid_hierarchy, &LodLevel::base()),
            LeveledGridCell {
                lod: LodLevel::from_level(1),
                pos: GridCell { x: 1, y: 0, z: 0 },
            }
        );
        assert_eq!(
            t.query_sensor_position(&p5, &grid_hierarchy, &LodLevel::base()),
            LeveledGridCell {
                lod: LodLevel::from_level(2),
                pos: GridCell { x: 0, y: 0, z: 0 },
            }
        );
    }

    #[test]
    fn set_node_aabb() {
        let mut t = MetaTree {
            sensor_grid_hierarchy: I32GridHierarchy::new(0),
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
            &OptionAABB::new(Point3::new(1, 1, 1), Point3::new(4, 4, 4))
                .into_aabb()
                .unwrap(),
        );
        assert_eq!(
            t.lods,
            vec![MetaTreeLevel {
                depth: vec![
                    HashMap::from_iter([(
                        GridCell { x: 0, y: 0, z: 1 },
                        Node {
                            is_leaf: false,
                            bounds: OptionAABB::new(Point3::new(1, 1, 1), Point3::new(4, 4, 4))
                                .into_aabb()
                                .unwrap()
                        }
                    )]),
                    HashMap::from_iter([(
                        GridCell { x: 1, y: 1, z: 2 },
                        Node {
                            is_leaf: false,
                            bounds: OptionAABB::new(Point3::new(1, 1, 1), Point3::new(4, 4, 4))
                                .into_aabb()
                                .unwrap()
                        }
                    )]),
                    HashMap::from_iter([(
                        GridCell { x: 2, y: 3, z: 4 },
                        Node {
                            is_leaf: false,
                            bounds: OptionAABB::new(Point3::new(1, 1, 1), Point3::new(4, 4, 4))
                                .into_aabb()
                                .unwrap()
                        }
                    )]),
                    HashMap::from_iter([(
                        GridCell { x: 5, y: 6, z: 9 },
                        Node {
                            is_leaf: true,
                            bounds: OptionAABB::new(Point3::new(1, 1, 1), Point3::new(4, 4, 4))
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
            &OptionAABB::new(Point3::new(2, 2, 2), Point3::new(5, 5, 5))
                .into_aabb()
                .unwrap(),
        );

        assert_eq!(
            t.lods,
            vec![MetaTreeLevel {
                depth: vec![
                    HashMap::from_iter([(
                        GridCell { x: 0, y: 0, z: 1 },
                        Node {
                            is_leaf: false,
                            bounds: OptionAABB::new(Point3::new(1, 1, 1), Point3::new(5, 5, 5))
                                .into_aabb()
                                .unwrap()
                        }
                    )]),
                    HashMap::from_iter([(
                        GridCell { x: 1, y: 1, z: 2 },
                        Node {
                            is_leaf: false,
                            bounds: OptionAABB::new(Point3::new(1, 1, 1), Point3::new(5, 5, 5))
                                .into_aabb()
                                .unwrap()
                        }
                    ),]),
                    HashMap::from_iter([
                        (
                            GridCell { x: 2, y: 3, z: 4 },
                            Node {
                                is_leaf: false,
                                bounds: OptionAABB::new(Point3::new(1, 1, 1), Point3::new(4, 4, 4))
                                    .into_aabb()
                                    .unwrap()
                            }
                        ),
                        (
                            GridCell { x: 3, y: 2, z: 4 },
                            Node {
                                is_leaf: false,
                                bounds: OptionAABB::new(Point3::new(2, 2, 2), Point3::new(5, 5, 5))
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
                                bounds: OptionAABB::new(Point3::new(1, 1, 1), Point3::new(4, 4, 4))
                                    .into_aabb()
                                    .unwrap()
                            }
                        ),
                        (
                            GridCell { x: 7, y: 4, z: 9 },
                            Node {
                                is_leaf: true,
                                bounds: OptionAABB::new(Point3::new(2, 2, 2), Point3::new(5, 5, 5))
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

impl Debug for MetaTreeNodeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MetaTreeNodeId \"{}__{}__{}-{}-{}\"",
            self.lod.level(),
            self.node.lod.level(),
            self.node.pos.x,
            self.node.pos.y,
            self.node.pos.z,
        )
    }
}
