use crate::geometry::bounding_box::{BaseAABB, AABB};
use crate::geometry::position::{I32Position, Position};
use crate::nalgebra::Point3;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::mem;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I32GridHierarchy {
    shift: u16,
}

/// A partitioning of space into cubic grid cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I32Grid {
    shift: u8,
}

/// A specific level of a [GridHierarchy].
/// Basically a grid, that also knows about its LOD within the GridHierarchy.
pub struct LevelGrid {
    lod: LodLevel,
    grid: I32Grid,
}

impl LevelGrid {
    /// Construct a new [LevelGrid]
    pub fn new(lod: LodLevel, grid: I32Grid) -> Self {
        LevelGrid { lod, grid }
    }

    /// Returns the cell within the [GridHierarchy], that contains the given position.
    pub fn leveled_cell_at(&self, position: &I32Position) -> LeveledGridCell {
        LeveledGridCell {
            lod: self.lod,
            pos: self.grid.cell_at(position),
        }
    }

    /// Returns the underlying grid, essentially getting rid of the additional LOD information
    /// carried by the LevelGrid.
    pub fn into_grid(self) -> I32Grid {
        self.grid
    }

    /// Calculates the bounds of the cell.
    #[inline]
    pub fn cell_bounds(&self, cell_pos: &GridCell) -> AABB<i32> {
        self.grid.cell_bounds(cell_pos)
    }

    /// Returns the cell, that contains the given position.
    #[inline]
    pub fn cell_at(&self, position: &I32Position) -> GridCell {
        self.grid.cell_at(position)
    }

    /// Returns the side length of the cells in this grid
    #[inline]
    pub fn cell_size(&self) -> i32 {
        self.grid.cell_size()
    }
}

impl I32GridHierarchy {
    pub fn new(shift: u16) -> Self {
        I32GridHierarchy { shift }
    }

    /// Returns the given hierarchy level.
    pub fn level(&self, lod: &LodLevel) -> LevelGrid {
        let grid = I32Grid::new(&lod.finer_by(self.shift));
        LevelGrid::new(*lod, grid)
    }

    pub fn max_level(&self) -> LodLevel {
        LodLevel::from_level(31 - self.shift)
    }

    /// Returns the bounding box of the given cell
    pub fn get_leveled_cell_bounds(&self, cell: &LeveledGridCell) -> AABB<i32> {
        self.level(&cell.lod).cell_bounds(&cell.pos)
    }
}

impl Default for I32GridHierarchy {
    fn default() -> Self {
        I32GridHierarchy::new(0)
    }
}

impl I32Grid {
    pub fn new(lod: &LodLevel) -> Self {
        assert!(lod.level() < 32, "Grid level 32 or higher not supported.");
        I32Grid {
            shift: 31 - lod.level() as u8,
        }
    }

    /// Calculates the bounds of the cell.
    pub fn cell_bounds(&self, cell_pos: &GridCell) -> AABB<i32> {
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

    /// Returns the side length of the cells in this grid
    pub fn cell_size(&self) -> i32 {
        1_i32 << self.shift
    }

    /// Returns the cell, that contains the given position.
    pub fn cell_at(&self, position: &I32Position) -> GridCell {
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

#[cfg(test)]
mod tests {
    use crate::geometry::bounding_box::{BaseAABB, AABB};
    use crate::geometry::grid::{GridCell, I32Grid, LeveledGridCell, LodLevel};
    use crate::geometry::position::{CoordinateSystem, I32CoordinateSystem, Position};
    use crate::nalgebra::Point3;

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
    #[allow(clippy::unusual_byte_groupings)] // bits are intentionally grouped like this
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
        assert!(base_cell.overlaps_with(
            &base_cell
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
        ));

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
        assert!(base_cell.overlaps_with(
            &base_cell
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
        ));

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
