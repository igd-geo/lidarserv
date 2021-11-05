use crate::geometry::bounding_box::{BaseAABB, AABB};
use crate::geometry::position::{F64Position, I32Position, Position};
use crate::nalgebra::Point3;
use nalgebra::Scalar;
use serde::{Deserialize, Serialize};

/// Represents a level of detail.
/// LOD 0 is the "base lod", the coarsest possible level.
/// As the LOD level gets larger, more details are introduced - with every level, the minimum
/// distance between two points is halved.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct LodLevel(u16);

impl LodLevel {
    /// Returns the coarsest possible level (LOD 0).
    #[inline]
    pub fn base() -> Self {
        LodLevel(0)
    }

    /// Returns the level, that is finer by one LOD step. So the minimum distance between points
    /// is halved.
    #[inline]
    pub fn finer(&self) -> Self {
        LodLevel(self.0 + 1)
    }

    /// Returns the level, that is finer by the given number of LOD steps.
    #[inline]
    pub fn finer_by(&self, by: u16) -> Self {
        LodLevel(self.0 + by)
    }

    /// Returns the level, that is coarser by one LOD step, or None if the current level is LOD 0.
    pub fn coarser(&self) -> Option<Self> {
        if self.0 == 0 {
            None
        } else {
            Some(LodLevel(self.0 - 1))
        }
    }

    /// Returns the current LOD level.
    #[inline]
    pub fn level(&self) -> u16 {
        self.0
    }

    /// Constructs an LodLevel from the given level.
    #[inline]
    pub fn from_level(level: u16) -> Self {
        LodLevel(level)
    }
}

/// A collection of grids, that get finer with each LOD.
pub trait GridHierarchy {
    type Component: Scalar;
    type Position: Position<Component = Self::Component>;
    type Grid: Grid<Component = Self::Component, Position = Self::Position>;

    /// Returns the given hierarchy level.
    fn level(&self, lod: &LodLevel) -> LevelGrid<Self::Grid>;

    /// Returns the bounding box of the given cell
    fn get_leveled_cell_bounds(&self, cell: &LeveledGridCell) -> AABB<Self::Component> {
        self.level(&cell.lod).cell_bounds(&cell.pos)
    }
}

/// A partitioning of the space into cubic grid cells.
pub trait Grid {
    type Component: Scalar;
    type Position: Position<Component = Self::Component>;

    /// Calculates the bounds of the cell.
    fn cell_bounds(&self, cell_pos: &GridCell) -> AABB<Self::Component>;

    /// Returns the cell, that contains the given position.
    fn cell_at(&self, position: &Self::Position) -> GridCell;
}

/// A specific level of a [GridHierarchy].
/// Basically a grid, that also knows about its LOD within the GridHierarchy.
pub struct LevelGrid<G> {
    lod: LodLevel,
    grid: G,
}

impl<G> LevelGrid<G>
where
    G: Grid,
{
    /// Construct a new [LevelGrid]
    pub fn new(lod: LodLevel, grid: G) -> Self {
        LevelGrid { lod, grid }
    }

    /// Returns the cell within the [GridHierarchy], that contains the given position.
    pub fn leveled_cell_at(&self, position: &G::Position) -> LeveledGridCell {
        LeveledGridCell {
            lod: self.lod,
            pos: self.grid.cell_at(position),
        }
    }

    /// Returns the underlying grid, essentially getting rid of the additional LOD information
    /// carried by the LevelGrid.
    pub fn into_grid(self) -> G {
        self.grid
    }
}

impl<G: Grid> Grid for LevelGrid<G> {
    type Component = G::Component;
    type Position = G::Position;

    #[inline]
    fn cell_bounds(&self, cell_pos: &GridCell) -> AABB<Self::Component> {
        self.grid.cell_bounds(cell_pos)
    }

    #[inline]
    fn cell_at(&self, position: &Self::Position) -> GridCell {
        self.grid.cell_at(position)
    }
}

/// [GridHierarchy] for [f64] coordinates.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct F64GridHierarchy {
    base_size: f64,
}

/// [Grid] for [f64] coordinates.
#[derive(Debug, Copy, Clone)]
pub struct F64Grid {
    cell_size: f64,
}

impl F64GridHierarchy {
    /// Construct a new grid hierarchy.
    /// Base Size is the size of the grid cells at LOD 0.
    pub fn new(base_size: f64) -> Self {
        F64GridHierarchy { base_size }
    }
}

impl GridHierarchy for F64GridHierarchy {
    type Component = f64;
    type Position = F64Position;
    type Grid = F64Grid;

    fn level(&self, lod: &LodLevel) -> LevelGrid<Self::Grid> {
        let cell_size = self.base_size * 0.5_f64.powi(lod.level() as i32);
        LevelGrid::new(*lod, F64Grid { cell_size })
    }
}

impl Grid for F64Grid {
    type Component = f64;
    type Position = F64Position;

    fn cell_bounds(&self, cell_pos: &GridCell) -> AABB<<Self::Position as Position>::Component> {
        let min = Point3::new(
            cell_pos.x as f64 * self.cell_size,
            cell_pos.y as f64 * self.cell_size,
            cell_pos.z as f64 * self.cell_size,
        );
        let max = Point3::new(
            min.x + self.cell_size,
            min.y + self.cell_size,
            min.z + self.cell_size,
        );
        AABB::new(min, max)
    }

    fn cell_at(&self, position: &Self::Position) -> GridCell {
        let x = (position.x() / self.cell_size).floor() as i32;
        let y = (position.y() / self.cell_size).floor() as i32;
        let z = (position.z() / self.cell_size).floor() as i32;
        GridCell { x, y, z }
    }
}

#[derive(Clone, Debug)]
pub struct I32GridHierarchy {
    shift: u16,
}

#[derive(Clone, Debug)]
pub struct I32Grid {
    shift: u8,
}

impl I32GridHierarchy {
    pub fn new(shift: u16) -> Self {
        I32GridHierarchy { shift }
    }
}

impl Default for I32GridHierarchy {
    fn default() -> Self {
        I32GridHierarchy::new(0)
    }
}

impl GridHierarchy for I32GridHierarchy {
    type Component = i32;
    type Position = I32Position;
    type Grid = I32Grid;

    fn level(&self, lod: &LodLevel) -> LevelGrid<Self::Grid> {
        let grid = I32Grid::new(&lod.finer_by(self.shift));
        LevelGrid::new(*lod, grid)
    }
}

impl I32Grid {
    pub fn new(lod: &LodLevel) -> Self {
        assert!(lod.level() < 32, "Grid level 32 or higher not supported.");
        I32Grid {
            shift: 31 - lod.level() as u8,
        }
    }
}

impl Grid for I32Grid {
    type Component = i32;
    type Position = I32Position;

    fn cell_bounds(&self, cell_pos: &GridCell) -> AABB<Self::Component> {
        AABB::new(
            Point3::new(
                cell_pos.x << self.shift,
                cell_pos.y << self.shift,
                cell_pos.z << self.shift,
            ),
            Point3::new(
                (cell_pos.x.wrapping_add(1) << self.shift).wrapping_sub(1),
                (cell_pos.y.wrapping_add(1) << self.shift).wrapping_sub(1),
                (cell_pos.z.wrapping_add(1) << self.shift).wrapping_sub(1),
            ),
        )
    }

    fn cell_at(&self, position: &Self::Position) -> GridCell {
        GridCell {
            x: position.x() >> self.shift,
            y: position.y() >> self.shift,
            z: position.z() >> self.shift,
        }
    }
}

/// Selects a cell within a grid
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
pub struct GridCell {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Selects a cell within a grid hierarchy
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
pub struct LeveledGridCell {
    pub lod: LodLevel,
    pub pos: GridCell,
}

impl LeveledGridCell {
    /// Returns the 8 cells in the next LOD, that this cell contains.
    pub fn children(&self) -> [LeveledGridCell; 8] {
        let lod = self.lod.finer();
        let GridCell { x, y, z } = self.pos;
        [
            LeveledGridCell {
                lod,
                pos: GridCell {
                    x: x * 2,
                    y: y * 2,
                    z: z * 2,
                },
            },
            LeveledGridCell {
                lod,
                pos: GridCell {
                    x: x * 2 + 1,
                    y: y * 2,
                    z: z * 2,
                },
            },
            LeveledGridCell {
                lod,
                pos: GridCell {
                    x: x * 2,
                    y: y * 2 + 1,
                    z: z * 2,
                },
            },
            LeveledGridCell {
                lod,
                pos: GridCell {
                    x: x * 2 + 1,
                    y: y * 2 + 1,
                    z: z * 2,
                },
            },
            LeveledGridCell {
                lod,
                pos: GridCell {
                    x: x * 2,
                    y: y * 2,
                    z: z * 2 + 1,
                },
            },
            LeveledGridCell {
                lod,
                pos: GridCell {
                    x: x * 2 + 1,
                    y: y * 2,
                    z: z * 2 + 1,
                },
            },
            LeveledGridCell {
                lod,
                pos: GridCell {
                    x: x * 2,
                    y: y * 2 + 1,
                    z: z * 2 + 1,
                },
            },
            LeveledGridCell {
                lod,
                pos: GridCell {
                    x: x * 2 + 1,
                    y: y * 2 + 1,
                    z: z * 2 + 1,
                },
            },
        ]
    }

    /// Returns the cell in the next coarser LOD level, that contains this cell.
    pub fn parent(&self) -> Option<LeveledGridCell> {
        fn div2(n: i32) -> i32 {
            if n < 0 {
                (n - 1) / 2
            } else {
                n / 2
            }
        }
        self.lod.coarser().map(|lod| LeveledGridCell {
            lod,
            pos: GridCell {
                x: div2(self.pos.x),
                y: div2(self.pos.y),
                z: div2(self.pos.z),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::bounding_box::{BaseAABB, AABB};
    use crate::geometry::grid::{
        F64GridHierarchy, Grid, GridCell, GridHierarchy, I32Grid, LeveledGridCell, LodLevel,
    };
    use crate::geometry::position::{CoordinateSystem, F64Position, I32CoordinateSystem, Position};
    use crate::nalgebra::Point3;

    #[test]
    fn float_grid_cell_bounds() {
        let hierarchy = F64GridHierarchy::new(2.5);

        let level_0 = hierarchy.level(&LodLevel::base());

        assert_eq!(
            level_0.cell_bounds(&GridCell {
                x: -3,
                y: -2,
                z: -1
            }),
            AABB::new(Point3::new(-7.5, -5.0, -2.5), Point3::new(-5.0, -2.5, 0.0))
        );
        assert_eq!(
            level_0.cell_bounds(&GridCell { x: 0, y: 1, z: 2 }),
            AABB::new(Point3::new(0.0, 2.5, 5.0), Point3::new(2.5, 5.0, 7.5))
        );

        let level_1 = hierarchy.level(&LodLevel::base().finer());
        assert_eq!(
            level_1.cell_bounds(&GridCell { x: 0, y: 1, z: 2 }),
            AABB::new(Point3::new(0.0, 1.25, 2.5), Point3::new(1.25, 2.5, 3.75))
        );
    }

    #[test]
    fn float_grid_cell() {
        let hierarchy = F64GridHierarchy::new(2.0);

        let level_0 = hierarchy.level(&LodLevel::base());

        assert_eq!(
            level_0.leveled_cell_at(&F64Position::from_components(-6.0, -5.0, -4.0)),
            LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell {
                    x: -3,
                    y: -3,
                    z: -2
                }
            }
        );
        assert_eq!(
            level_0.leveled_cell_at(&F64Position::from_components(-3.0, -2.0, -1.0)),
            LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell {
                    x: -2,
                    y: -1,
                    z: -1
                }
            }
        );
        assert_eq!(
            level_0.leveled_cell_at(&F64Position::from_components(-0.0, 0.0, 1.0)),
            LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 0, y: 0, z: 0 }
            }
        );
        assert_eq!(
            level_0.leveled_cell_at(&F64Position::from_components(2.0, 3.0, 4.0)),
            LeveledGridCell {
                lod: LodLevel::from_level(0),
                pos: GridCell { x: 1, y: 1, z: 2 }
            }
        );

        let level_1 = hierarchy.level(&LodLevel::base().finer());
        assert_eq!(
            level_1.leveled_cell_at(&F64Position::from_components(0.0, 1.0, 2.0)),
            LeveledGridCell {
                lod: LodLevel::from_level(1),
                pos: GridCell { x: 0, y: 1, z: 2 }
            }
        );
    }

    #[test]
    fn int_grid_cell() {
        let grid = I32Grid::new(&LodLevel::from_level(2));
        let coordinates =
            I32CoordinateSystem::new(Point3::new(1.0, 1.0, 1.0), Point3::new(16.0, 16.0, 16.0));
        let test_cases = [
            (
                coordinates
                    .encode_position(&Point3::new(1.0, 2.0, 3.0))
                    .unwrap(),
                GridCell {
                    x: -4,
                    y: -4,
                    z: -3,
                },
            ),
            (
                coordinates
                    .encode_position(&Point3::new(4.0, 5.0, 6.0))
                    .unwrap(),
                GridCell {
                    x: -3,
                    y: -2,
                    z: -2,
                },
            ),
            (
                coordinates
                    .encode_position(&Point3::new(7.0, 8.0, 9.0))
                    .unwrap(),
                GridCell { x: -1, y: -1, z: 0 },
            ),
            (
                coordinates
                    .encode_position(&Point3::new(10.0, 11.0, 12.0))
                    .unwrap(),
                GridCell { x: 0, y: 1, z: 1 },
            ),
            (
                coordinates
                    .encode_position(&Point3::new(13.0, 14.0, 15.0))
                    .unwrap(),
                GridCell { x: 2, y: 2, z: 3 },
            ),
            (
                coordinates
                    .encode_position(&Point3::new(16.0, 16.0, 16.0))
                    .unwrap(),
                GridCell { x: 3, y: 3, z: 3 },
            ),
        ];
        for (position, cell) in test_cases {
            assert_eq!(
                grid.cell_at(&position),
                cell,
                "Test case for position {:?}",
                position.decode(&coordinates)
            )
        }
    }

    #[test]
    fn int_grid_cell_bounds() {
        let grid = I32Grid::new(&LodLevel::from_level(2));

        let test_cases = [
            (
                GridCell {
                    x: -4,
                    y: -3,
                    z: -2,
                },
                AABB::new(
                    Point3::new(
                        0b100_00000000000000000000000000000_u32 as i32,
                        0b101_00000000000000000000000000000_u32 as i32,
                        0b110_00000000000000000000000000000_u32 as i32,
                    ),
                    Point3::new(
                        0b100_11111111111111111111111111111_u32 as i32,
                        0b101_11111111111111111111111111111_u32 as i32,
                        0b110_11111111111111111111111111111_u32 as i32,
                    ),
                ),
            ),
            (
                GridCell { x: -1, y: 0, z: 1 },
                AABB::new(
                    Point3::new(
                        0b111_00000000000000000000000000000_u32 as i32,
                        0b000_00000000000000000000000000000,
                        0b001_00000000000000000000000000000,
                    ),
                    Point3::new(
                        0b111_11111111111111111111111111111_u32 as i32,
                        0b000_11111111111111111111111111111,
                        0b001_11111111111111111111111111111,
                    ),
                ),
            ),
            (
                GridCell { x: 2, y: 3, z: 3 },
                AABB::new(
                    Point3::new(
                        0b010_00000000000000000000000000000,
                        0b011_00000000000000000000000000000,
                        0b011_00000000000000000000000000000,
                    ),
                    Point3::new(
                        0b010_11111111111111111111111111111,
                        0b011_11111111111111111111111111111,
                        0b011_11111111111111111111111111111,
                    ),
                ),
            ),
        ];

        for (cell, correct_bounds) in test_cases {
            let bounds = grid.cell_bounds(&cell);
            assert_eq!(bounds, correct_bounds, "failed at cell {:?}", cell);
        }
    }

    #[test]
    fn leveled_grid_cell_parent() {
        assert_eq!(
            LeveledGridCell {
                lod: LodLevel::from_level(4),
                pos: GridCell { x: 4, y: 3, z: 2 }
            }
            .parent()
            .unwrap(),
            LeveledGridCell {
                lod: LodLevel::from_level(3),
                pos: GridCell { x: 2, y: 1, z: 1 }
            }
        );
        assert_eq!(
            LeveledGridCell {
                lod: LodLevel::from_level(4),
                pos: GridCell { x: 1, y: 0, z: -1 }
            }
            .parent()
            .unwrap(),
            LeveledGridCell {
                lod: LodLevel::from_level(3),
                pos: GridCell { x: 0, y: 0, z: -1 }
            }
        );
        assert_eq!(
            LeveledGridCell {
                lod: LodLevel::from_level(4),
                pos: GridCell {
                    x: -2,
                    y: -3,
                    z: -4
                }
            }
            .parent()
            .unwrap(),
            LeveledGridCell {
                lod: LodLevel::from_level(3),
                pos: GridCell {
                    x: -1,
                    y: -2,
                    z: -2
                }
            }
        );
    }
}
