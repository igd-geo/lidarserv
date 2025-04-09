use nalgebra::point;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};
use std::mem;
use std::ops::RangeInclusive;

use crate::f64_utils::f64_next_down;

use super::bounding_box::Aabb;
use super::position::{Component, Position};

/// Represents a level of detail.
/// LOD 0 is the "base lod", the coarsest possible level.
/// As the LOD level gets larger, more details are introduced - with every level, the minimum
/// distance between two points is halved.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct LodLevel(u8);

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
    pub fn finer_by(&self, by: u8) -> Self {
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
    pub fn level(&self) -> u8 {
        self.0
    }

    /// Constructs an LodLevel from the given level.
    #[inline]
    pub fn from_level(level: u8) -> Self {
        LodLevel(level)
    }
}

impl Display for LodLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LOD{}", self.0)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Grid<C: Component>(C::Grid);

impl<C: Component> Grid<C> {
    /// Calculates the bounds of the cell.
    pub fn cell_bounds(&self, cell_pos: GridCell) -> Aabb<C> {
        let (xmin, xmax) = C::grid_get_cell_range(self.0, cell_pos.x);
        let (ymin, ymax) = C::grid_get_cell_range(self.0, cell_pos.y);
        let (zmin, zmax) = C::grid_get_cell_range(self.0, cell_pos.z);
        Aabb::new(point![xmin, ymin, zmin], point![xmax, ymax, zmax])
    }

    /// Returns the side length of the cells in this grid
    pub fn cell_size(&self) -> C {
        C::grid_get_cell_size(self.0)
    }

    /// Returns the cell, that contains the given position.
    pub fn cell_at(&self, position: Position<C>) -> GridCell {
        GridCell {
            x: position.x.grid_get_cell(self.0),
            y: position.y.grid_get_cell(self.0),
            z: position.z.grid_get_cell(self.0),
        }
    }
}

/// A LOD Hierarchy of grids: The coarsest grid is at LOD 0 and with each LOD, the grid gets finer.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridHierarchy {
    shift: i16,
}

impl GridHierarchy {
    pub fn new(shift: i16) -> Self {
        GridHierarchy { shift }
    }

    /// Returns the shift value of this hierarchy
    pub fn shift(&self) -> i16 {
        self.shift
    }

    /// Returns the given hierarchy level.
    pub fn level<C: Component>(&self, lod: LodLevel) -> Grid<C> {
        let level = self.shift - lod.level() as i16;
        assert!(C::grid_get_level_minmax().contains(&level));
        Grid(C::grid_get_level(level))
    }

    /// Returns the maximum LOD level that is allowed.
    /// Or None, if the grid hierarchy is really incompatible
    /// with the component type C.
    /// (
    /// Can e.g. happen, if already LOD 0 is finer than the resolution of C would allow. E.g. a grid hierarchy
    /// where lod 0 has a node size of 0.01 - this would be fine for floats, but makes no sense for integers.
    /// )
    pub fn max_lod<C: Component>(&self) -> Option<LodLevel> {
        let (level_min, level_max) = C::grid_get_level_minmax().into_inner();
        let lod_min = self.shift - level_max;
        let lod_max = self.shift - level_min;
        if lod_min > 0 {
            return None;
        }
        if lod_max < 0 {
            return None;
        }
        if lod_max >= u8::MAX as i16 {
            return Some(LodLevel::from_level(u8::MAX));
        }
        Some(LodLevel::from_level(lod_max as u8))
    }

    /// Returns the bounding box of the given cell
    pub fn get_leveled_cell_bounds<C: Component>(&self, cell: LeveledGridCell) -> Aabb<C> {
        self.level(cell.lod).cell_bounds(cell.pos)
    }

    /// Returns the cell that contains the given position at the requested lod
    pub fn get_leveled_cell_at<C: Component>(
        &self,
        lod: LodLevel,
        position: Position<C>,
    ) -> LeveledGridCell {
        LeveledGridCell {
            lod,
            pos: self.level(lod).cell_at(position),
        }
    }
}

impl Default for GridHierarchy {
    fn default() -> Self {
        GridHierarchy::new(0)
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
            if n < 0 { (n - 1) / 2 } else { n / 2 }
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

    pub fn overlaps_with(&self, other: &LeveledGridCell) -> bool {
        let mut cell_1 = self;
        let mut cell_2 = other;
        if cell_1.lod > cell_2.lod {
            mem::swap(&mut cell_1, &mut cell_2);
        }
        let lod_difference = cell_2.lod.level() - cell_1.lod.level();
        let multiplier = 1 << (lod_difference);
        let min = GridCell {
            x: cell_1.pos.x * multiplier,
            y: cell_1.pos.y * multiplier,
            z: cell_1.pos.z * multiplier,
        };
        let max = GridCell {
            x: (cell_1.pos.x + 1) * multiplier,
            y: (cell_1.pos.y + 1) * multiplier,
            z: (cell_1.pos.z + 1) * multiplier,
        };
        min.x <= cell_2.pos.x
            && cell_2.pos.x < max.x
            && min.y <= cell_2.pos.y
            && cell_2.pos.y < max.y
            && min.z <= cell_2.pos.z
            && cell_2.pos.z < max.z
    }
}

pub trait GridComponent: Sized {
    type Grid: Debug + Copy + Clone + Send + Sync;

    /// Returns a grid for the given level of dissection.
    /// Level 0 is always the level with grid size 1.
    /// Each larger level doubles the grid size.
    /// Each smaller (negative) level halves the grid size.
    /// So, the grid size is always 2^level.
    /// The level has to be in the range returned by [GridComponent::grid_get_level_minmax] (inclusive).
    /// Otherwise, this function may panick.
    fn grid_get_level(level: i16) -> Self::Grid;

    fn grid_get_level_minmax() -> RangeInclusive<i16>;

    /// The grid cell along this component
    fn grid_get_cell(self, grid: Self::Grid) -> i32;

    /// The cell size in this grid.
    fn grid_get_cell_size(grid: Self::Grid) -> Self;

    /// The range of values in the given cell
    /// returns a tuple (min, max) (both inclusive).
    fn grid_get_cell_range(grid: Self::Grid, cell: i32) -> (Self, Self);
}

impl GridComponent for i32 {
    type Grid = u8;

    fn grid_get_level(level: i16) -> Self::Grid {
        assert!(level >= 0);
        assert!(level <= 31);
        level as u8
    }

    fn grid_get_level_minmax() -> RangeInclusive<i16> {
        0..=31
    }

    fn grid_get_cell(self, grid: Self::Grid) -> i32 {
        self >> grid
    }

    fn grid_get_cell_size(grid: Self::Grid) -> Self {
        1 << grid
    }

    fn grid_get_cell_range(grid: Self::Grid, cell: i32) -> (Self, Self) {
        let min = cell << grid;
        let max = min + (1 << grid) - 1;
        (min, max)
    }
}

impl GridComponent for f64 {
    type Grid = f64;

    fn grid_get_level(level: i16) -> Self::Grid {
        let sign = 0_u64;
        let exponent = (1023_i16 + level) as u64;
        let fraction = 0_u64;
        let number = (sign << 63) | (exponent << 52) | fraction;
        f64::from_bits(number)
    }

    fn grid_get_level_minmax() -> RangeInclusive<i16> {
        -1022..=1023
    }

    fn grid_get_cell(self, grid: Self::Grid) -> i32 {
        (self / grid).floor() as i32
    }

    fn grid_get_cell_size(grid: Self::Grid) -> Self {
        grid
    }

    fn grid_get_cell_range(grid: Self::Grid, cell: i32) -> (Self, Self) {
        let min = (cell as f64) * grid;
        let max = f64_next_down(min + grid);
        (min, max)
    }
}

#[cfg(test)]
mod tests {
    use nalgebra::point;

    use crate::geometry::bounding_box::Aabb;
    use crate::geometry::grid::{GridCell, LeveledGridCell, LodLevel};

    use super::Grid;

    #[test]
    fn int_grid_cell() {
        let grid = Grid::<i32>(2);
        let test_cases = [
            (
                point![-5, -4, -3],
                GridCell {
                    x: -2,
                    y: -1,
                    z: -1,
                },
            ),
            (point![-2, -1, 0], GridCell { x: -1, y: -1, z: 0 }),
            (point![1, 2, 3], GridCell { x: 0, y: 0, z: 0 }),
            (point![4, 5, 6], GridCell { x: 1, y: 1, z: 1 }),
            (point![7, 8, 9], GridCell { x: 1, y: 2, z: 2 }),
        ];
        for (position, cell) in test_cases {
            assert_eq!(
                grid.cell_at(position),
                cell,
                "Test case for position {position}"
            )
        }
    }

    #[test]
    fn int_grid_cell_bounds() {
        let grid = Grid::<i32>(2);

        let test_cases = [
            (
                GridCell {
                    x: -4,
                    y: -3,
                    z: -2,
                },
                Aabb::new(point![-16, -12, -8], point![-13, -9, -5]),
            ),
            (
                GridCell { x: -1, y: 0, z: 1 },
                Aabb::new(point![-4, 0, 4], point![-1, 3, 7]),
            ),
            (
                GridCell { x: 2, y: 3, z: 3 },
                Aabb::new(point![8, 12, 12], point![11, 15, 15]),
            ),
        ];

        for (cell, correct_bounds) in test_cases {
            let bounds = grid.cell_bounds(cell);
            assert_eq!(bounds, correct_bounds, "failed at cell {:?}", cell);
        }
    }

    #[test]
    fn f64_grid_cell() {
        let grid = Grid::<f64>(4.0);

        let test_cases = [
            (
                point![-4.1, -4.0, -3.9],
                GridCell {
                    x: -2,
                    y: -1,
                    z: -1,
                },
            ),
            (point![-0.0, 0.0, 2.0], GridCell { x: 0, y: 0, z: 0 }),
            (point![3.9, 4.0, 4.1], GridCell { x: 0, y: 1, z: 1 }),
            (point![7.9, 8.0, 8.1], GridCell { x: 1, y: 2, z: 2 }),
        ];
        for (position, cell) in test_cases {
            assert_eq!(
                grid.cell_at(position),
                cell,
                "Test case for position {position}"
            )
        }
    }

    #[test]
    fn f64_grid_cell_bounds() {
        let grid = Grid::<f64>(4.0);
        let test_cases = [
            (
                GridCell {
                    x: -4,
                    y: -3,
                    z: -2,
                },
                Aabb::new(point![-16.0, -12.0, -8.0], point![-12.0, -8.0, -4.0]),
            ),
            (
                GridCell { x: -1, y: 0, z: 1 },
                Aabb::new(point![-4.0, 0.0, 4.0], point![0.0, 4.0, 8.0]),
            ),
            (
                GridCell { x: 2, y: 3, z: 3 },
                Aabb::new(point![8.0, 12.0, 12.0], point![12.0, 16.0, 16.0]),
            ),
        ];

        for (cell, correct_bounds) in test_cases {
            let bounds = grid.cell_bounds(cell);
            assert_eq!(
                bounds.min, correct_bounds.min,
                "test case for cell {cell:?}\n    expected bounds: {correct_bounds:?}\n    actual bounds: {bounds:?}",
            );
            assert!(
                bounds.max < correct_bounds.max,
                "test case for cell {cell:?}\n    expected bounds: {correct_bounds:?}\n    actual bounds: {bounds:?}",
            );
            assert!(
                bounds.max.map(|c| c + 0.00001) > correct_bounds.max,
                "test case for cell {cell:?}\n    expected bounds: {correct_bounds:?}\n    actual bounds: {bounds:?}",
            );
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

    #[test]
    fn leveled_grid_cell_overlap() {
        let base_cell = LeveledGridCell {
            lod: LodLevel::from_level(3),
            pos: GridCell { x: 0, y: 0, z: 0 },
        };

        // overlaps with self
        assert!(base_cell.overlaps_with(&base_cell));

        // not with neighbor
        assert!(!base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(3),
            pos: GridCell { x: 1, y: 0, z: 0 }
        }));

        // overlaps with parent(s)
        assert!(base_cell.overlaps_with(&base_cell.parent().unwrap()));
        assert!(base_cell.overlaps_with(&base_cell.parent().unwrap().parent().unwrap()));
        assert!(
            base_cell.overlaps_with(
                &base_cell
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
            )
        );

        // overlaps with children
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell { x: 0, y: 0, z: 0 }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell { x: 0, y: 0, z: 1 }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell { x: 0, y: 1, z: 0 }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell { x: 0, y: 1, z: 1 }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell { x: 1, y: 0, z: 0 }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell { x: 1, y: 0, z: 1 }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell { x: 1, y: 1, z: 0 }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell { x: 1, y: 1, z: 1 }
        }));

        // not with non-children
        assert!(!base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell { x: 1, y: 2, z: 0 }
        }));

        // overlaps with recursive children
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(5),
            pos: GridCell { x: 0, y: 1, z: 2 }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(5),
            pos: GridCell { x: 3, y: 2, z: 1 }
        }));

        // not a recursive child any more
        assert!(!base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(5),
            pos: GridCell { x: 4, y: 0, z: 3 }
        }));
    }

    #[test]
    fn leveled_grid_cell_overlap_neg() {
        let base_cell = LeveledGridCell {
            lod: LodLevel::from_level(3),
            pos: GridCell {
                x: -3,
                y: -6,
                z: -9,
            },
        };

        // overlaps with self
        assert!(base_cell.overlaps_with(&base_cell));

        // not with neighbor
        assert!(!base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(3),
            pos: GridCell {
                x: -2,
                y: -6,
                z: -9
            }
        }));

        // overlaps with parent(s)
        assert!(base_cell.overlaps_with(&base_cell.parent().unwrap()));
        assert!(base_cell.overlaps_with(&base_cell.parent().unwrap().parent().unwrap()));
        assert!(
            base_cell.overlaps_with(
                &base_cell
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
            )
        );

        // overlaps with children
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell {
                x: -6,
                y: -12,
                z: -18
            }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell {
                x: -6,
                y: -12,
                z: -17
            }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell {
                x: -6,
                y: -11,
                z: -18
            }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell {
                x: -6,
                y: -11,
                z: -17
            }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell {
                x: -5,
                y: -12,
                z: -18
            }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell {
                x: -5,
                y: -12,
                z: -17
            }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell {
                x: -5,
                y: -11,
                z: -18
            }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell {
                x: -5,
                y: -11,
                z: -17
            }
        }));

        // not with non-children
        assert!(!base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(4),
            pos: GridCell {
                x: -4,
                y: -11,
                z: -18
            }
        }));

        // overlaps with recursive children
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(5),
            pos: GridCell {
                x: -12,
                y: -24,
                z: -36
            }
        }));
        assert!(base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(5),
            pos: GridCell {
                x: -9,
                y: -21,
                z: -33
            }
        }));

        // not a recursive child any more
        assert!(!base_cell.overlaps_with(&LeveledGridCell {
            lod: LodLevel::from_level(5),
            pos: GridCell {
                x: -8,
                y: -21,
                z: -33
            }
        }));
    }
}
