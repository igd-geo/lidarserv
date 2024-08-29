use pasture_core::containers::BorrowedBuffer;
use serde::{Deserialize, Serialize};
use std::mem;

use crate::{
    geometry::{
        bounding_box::Aabb,
        coordinate_system::CoordinateSystem,
        grid::{GridHierarchy, LeveledGridCell, LodLevel},
        position::{Component, WithComponentTypeOnce},
    },
    query::empty::EmptyQuery,
};

use super::{ExecutableQuery, NodeQueryResult, Query};

/// Query that matches all points in a certain bounding box.
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct AabbQuery(
    /// The bounding box in global coordinates
    pub Aabb<f64>,
);

/// Query that matches all points in a certain bounding box (local coordinates).
#[derive(Debug, Copy, Clone, PartialEq)]
struct AabbQueryExecutable<C: Component> {
    node_hierarchy: GridHierarchy,
    aabb: Aabb<C>,
}

impl Query for AabbQuery {
    type Executable = Box<dyn ExecutableQuery>;

    fn prepare(self, ctx: &super::QueryContext) -> Box<dyn ExecutableQuery> {
        if self.0.is_empty() {
            return Box::new(EmptyQuery) as Box<dyn ExecutableQuery>;
        }

        struct Wct {
            aabb_global: Aabb<f64>,
            coordinate_system: CoordinateSystem,
            node_hierarchy: GridHierarchy,
        }
        impl WithComponentTypeOnce for Wct {
            type Output = Box<dyn ExecutableQuery>;

            fn run_once<C: Component>(self) -> Self::Output {
                // ensure this aabb is in the bounds of this coordinate system
                // (shrink accordingly)
                let bounds = self.coordinate_system.bounds::<C>();
                let cut_min = self.aabb_global.min.sup(&bounds.min);
                let cut_max = self.aabb_global.max.inf(&bounds.max);

                // convert aabb to local coordinates
                let mut local_min = self
                    .coordinate_system
                    .encode_position::<C>(cut_min)
                    .expect("we made sure, that the coordinates are in the bounds");
                let mut local_max = self
                    .coordinate_system
                    .encode_position::<C>(cut_max)
                    .expect("we made sure, that the coordinates are in the bounds");
                for i in 0..3 {
                    if local_min[i] > local_max[i] {
                        mem::swap(&mut local_min[i], &mut local_max[i])
                    }
                }

                // create query
                let q = AabbQueryExecutable {
                    node_hierarchy: self.node_hierarchy,
                    aabb: Aabb::new(local_min, local_max),
                };
                Box::new(q)
            }
        }

        Wct {
            aabb_global: self.0,
            coordinate_system: ctx.coordinate_system,
            node_hierarchy: ctx.node_hierarchy,
        }
        .for_component_type_once(ctx.component_type)
    }
}

impl<C: Component> ExecutableQuery for AabbQueryExecutable<C> {
    fn matches_node(&self, node: LeveledGridCell) -> super::NodeQueryResult {
        let node_aabb = self.node_hierarchy.get_leveled_cell_bounds::<C>(node);
        if self.aabb.intersects_aabb(node_aabb) {
            if self.aabb.contains_aabb(node_aabb) {
                NodeQueryResult::Positive
            } else {
                NodeQueryResult::Partial
            }
        } else {
            NodeQueryResult::Negative
        }
    }

    fn matches_points(
        &self,
        _lod: LodLevel,
        points: &pasture_core::containers::VectorBuffer,
    ) -> Vec<bool> {
        points
            .view_attribute::<C::PasturePrimitive>(&C::position_attribute())
            .into_iter()
            .map(|p| C::pasture_to_position(p))
            .map(|pos| self.aabb.contains(pos))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use nalgebra::{point, vector};
    use pasture_core::containers::VectorBuffer;

    use crate::{
        geometry::{
            bounding_box::Aabb,
            coordinate_system::CoordinateSystem,
            grid::{GridCell, GridHierarchy, LeveledGridCell, LodLevel},
            position::PositionComponentType,
            test::{F64Point, I32Point},
        },
        query::{ExecutableQuery, NodeQueryResult, Query, QueryContext},
    };

    use super::AabbQuery;

    #[test]
    fn test_filter_nodes_f64() {
        let query = AabbQuery(Aabb::new(
            point![50.0, 50.0, 50.0],
            point![75.0, 75.0, 75.0],
        ));

        let ctx = QueryContext {
            node_hierarchy: GridHierarchy::new(10),
            point_hierarchy: GridHierarchy::new(5),
            coordinate_system: CoordinateSystem::from_las_transform(
                vector![100.0 / 1024.0, 100.0 / 1024.0, 100.0 / 1024.0],
                vector![0.0, 0.0, 0.0],
            ),
            component_type: PositionComponentType::F64,
        };
        assert_eq!(
            // This assert is not really part of the test.
            // If this assert fails, then someone changed the
            // implementation of the grid hierarchy. In this case,
            // we will also need to adapt this test.
            ctx.node_hierarchy
                .get_leveled_cell_bounds::<f64>(LeveledGridCell {
                    lod: LodLevel::from_level(0),
                    pos: GridCell { x: 0, y: 0, z: 0 }
                }),
            Aabb::new(
                point![0.0, 0.0, 0.0],
                point![1023.9999999999999, 1023.9999999999999, 1023.9999999999999]
            )
        );

        let q = query.prepare(&ctx);
        assert_eq!(
            q.matches_node(LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 0, y: 0, z: 0 }
            }),
            NodeQueryResult::Partial
        );
        assert_eq!(
            q.matches_node(LeveledGridCell {
                lod: LodLevel::from_level(1),
                pos: GridCell { x: 0, y: 0, z: 0 }
            }),
            NodeQueryResult::Negative
        );
        assert_eq!(
            q.matches_node(LeveledGridCell {
                lod: LodLevel::from_level(2),
                pos: GridCell { x: 2, y: 2, z: 2 }
            }),
            NodeQueryResult::Positive
        );
        assert_eq!(
            q.matches_node(LeveledGridCell {
                lod: LodLevel::from_level(4),
                pos: GridCell { x: 9, y: 10, z: 9 }
            }),
            NodeQueryResult::Positive
        );
    }

    #[test]
    fn test_filter_points_f64() {
        let query = AabbQuery(Aabb::new(
            point![50.0, 50.0, 50.0],
            point![75.0, 75.0, 75.0],
        ));

        let ctx = QueryContext {
            node_hierarchy: GridHierarchy::new(10),
            point_hierarchy: GridHierarchy::new(5),
            coordinate_system: CoordinateSystem::from_las_transform(
                vector![100.0 / 1024.0, 100.0 / 1024.0, 100.0 / 1024.0],
                vector![0.0, 0.0, 0.0],
            ),
            component_type: PositionComponentType::F64,
        };

        let q = query.prepare(&ctx);
        let points: VectorBuffer = [
            F64Point {
                position: vector![500.0, 500.0, 500.0],
            },
            F64Point {
                position: vector![512.0, 512.0, 512.0],
            },
            F64Point {
                position: vector![600.0, 600.0, 600.0],
            },
            F64Point {
                position: vector![1024.0, 1024.0, 1024.0],
            },
        ]
        .into_iter()
        .collect();
        assert_eq!(
            q.matches_points(LodLevel::base(), &points),
            vec![false, true, true, false]
        )
    }

    #[test]
    fn test_filter_nodes_i32() {
        let query = AabbQuery(Aabb::new(
            point![2.56, 2.56, 2.56],
            point![5.11, 5.11, 5.11],
        ));

        // lod 0:                      0
        // lod 0:                   0.0-10.23
        //                  /----------+----------\
        // lod 1:          0                       1
        // lod 1:       0.0-5.11               5.12-10.23
        //            /----+----\             /----+----\
        // lod 2:    0           1           2           3
        // lod 2: 0.0-2.55   2.56-5.11   5.12-7.67   7.68-10.23

        let ctx = QueryContext {
            node_hierarchy: GridHierarchy::new(10),
            point_hierarchy: GridHierarchy::new(5),
            coordinate_system: CoordinateSystem::from_las_transform(
                vector![0.01, 0.01, 0.01],
                vector![0.0, 0.0, 0.0],
            ),
            component_type: PositionComponentType::I32,
        };
        assert_eq!(
            // This assert is not really part of the test.
            // If this assert fails, then someone changed the
            // implementation of the grid hierarchy. In this case,
            // we will also need to adapt this test.
            ctx.node_hierarchy
                .get_leveled_cell_bounds::<i32>(LeveledGridCell {
                    lod: LodLevel::from_level(0),
                    pos: GridCell { x: 0, y: 0, z: 0 }
                }),
            Aabb::new(point![0, 0, 0], point![1023, 1023, 1023])
        );

        let q = query.prepare(&ctx);
        assert_eq!(
            q.matches_node(LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 0, y: 0, z: 0 }
            }),
            NodeQueryResult::Partial
        );
        assert_eq!(
            q.matches_node(LeveledGridCell {
                lod: LodLevel::from_level(2),
                pos: GridCell { x: 1, y: 1, z: 1 }
            }),
            NodeQueryResult::Positive
        );
        assert_eq!(
            q.matches_node(LeveledGridCell {
                lod: LodLevel::from_level(2),
                pos: GridCell { x: 3, y: 1, z: 1 }
            }),
            NodeQueryResult::Negative
        );
        assert_eq!(
            q.matches_node(LeveledGridCell {
                lod: LodLevel::from_level(1),
                pos: GridCell { x: 1, y: 1, z: 1 }
            }),
            NodeQueryResult::Negative
        );
    }

    #[test]
    fn test_filter_points_i32() {
        let query = AabbQuery(Aabb::new(
            point![2.56, 2.56, 2.56],
            point![5.11, 5.11, 5.11],
        ));

        let ctx = QueryContext {
            node_hierarchy: GridHierarchy::new(21),
            point_hierarchy: GridHierarchy::new(31),
            coordinate_system: CoordinateSystem::from_las_transform(
                vector![0.01, 0.01, 0.01],
                vector![0.0, 0.0, 0.0],
            ),
            component_type: PositionComponentType::I32,
        };

        let q = query.prepare(&ctx);

        let points: VectorBuffer = [
            I32Point {
                position: vector![255, 255, 255],
            },
            I32Point {
                position: vector![256, 256, 256],
            },
            I32Point {
                position: vector![300, 300, 300],
            },
            I32Point {
                position: vector![511, 511, 511],
            },
            I32Point {
                position: vector![512, 512, 512],
            },
        ]
        .into_iter()
        .collect();
        assert_eq!(
            q.matches_points(LodLevel::base(), &points),
            vec![false, true, true, true, false]
        )
    }
}
