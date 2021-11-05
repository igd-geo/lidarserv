use crate::geometry::grid::{GridCell, LodLevel};
use crate::geometry::points::PointType;
use crate::geometry::position::Component;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::mem;

pub trait SamplingFactory {
    type Point;
    type Sampling: Sampling<Point = Self::Point>;

    /// Usually the LOD.
    /// Or a LeveledGridCell for some more special applications.
    type Param;

    fn build(&self, param: &Self::Param) -> Self::Sampling;
}

/// Samples incoming points and stores the selected ones.
pub trait Sampling {
    type Point;

    /// Return the number of points, that have been selected by the sampling.
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool;

    /// Samples from the given list of points and stores the selected ones.
    /// The return value contains
    /// all points that got rejected from the sampling,
    /// as well as all preexisting points, that got replaced by a selected point.
    fn insert<F>(&mut self, points: Vec<Self::Point>, patch_rejected: F) -> Vec<Self::Point>
    where
        F: FnMut(&Self::Point, &mut Self::Point);

    /// Returns the list of sampled points.
    fn into_points(self) -> Vec<Self::Point>;

    /// Empties the node and returns the list of sampled points.
    fn drain_points(&mut self) -> Vec<Self::Point>;

    /// Returns a copy of the list of sampled points.
    fn clone_points(&self) -> Vec<Self::Point>
    where
        Self::Point: Clone;
}

#[derive(Clone)]
pub struct GridCenterEntry<Point, Position, Distance> {
    point: Point,
    center: Position,
    center_distance: Distance,
}

#[derive(Clone)]
pub struct GridCenterSampling<Grid, Point, Position, Distance> {
    grid: Grid,
    points: HashMap<GridCell, GridCenterEntry<Point, Position, Distance>>,
}

#[derive(Clone, Debug)]
pub struct GridCenterSamplingFactory<GridHierarchy, Point, Position, Distance> {
    grid_hierarchy: GridHierarchy,

    #[allow(clippy::type_complexity)]
    _phantom: PhantomData<fn() -> (Point, Position, Distance)>,
}

impl<GridHierarchy, Point, Position, Distance>
    GridCenterSamplingFactory<GridHierarchy, Point, Position, Distance>
{
    pub fn new(point_grid_hierarchy: GridHierarchy) -> Self {
        GridCenterSamplingFactory {
            grid_hierarchy: point_grid_hierarchy,
            _phantom: PhantomData,
        }
    }
}

impl<GridHierarchy, Point, Position, Distance> SamplingFactory
    for GridCenterSamplingFactory<GridHierarchy, Point, Position, Distance>
where
    GridHierarchy: super::grid::GridHierarchy,
    Distance: PartialOrd,
    Position: super::position::Position<Distance = Distance>,
    Position::Component: Component,
    Point: PointType<Position = Position>,
    GridHierarchy::Grid: super::grid::Grid<Position = Position, Component = Position::Component>,
{
    type Point = Point;
    type Sampling = GridCenterSampling<GridHierarchy::Grid, Point, Position, Distance>;
    type Param = LodLevel;

    fn build(&self, level: &Self::Param) -> Self::Sampling {
        GridCenterSampling {
            grid: self.grid_hierarchy.level(level).into_grid(),
            points: HashMap::new(),
        }
    }
}

impl<Grid, Point, Position, Distance> Sampling
    for GridCenterSampling<Grid, Point, Position, Distance>
where
    Distance: PartialOrd,
    Position: super::position::Position<Distance = Distance>,
    Position::Component: Component,
    Point: PointType<Position = Position>,
    Grid: super::grid::Grid<Position = Position, Component = Position::Component>,
{
    type Point = Point;

    fn len(&self) -> usize {
        self.points.len()
    }

    fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    fn insert<F>(
        &mut self,
        insert_points: Vec<Self::Point>,
        mut patch_rejected: F,
    ) -> Vec<Self::Point>
    where
        F: FnMut(&Self::Point, &mut Self::Point),
    {
        let GridCenterSampling { grid, points, .. } = self;
        let mut rejected = Vec::new();

        // insert each point
        for mut point in insert_points {
            // cell that the point belongs to
            let cell = grid.cell_at(point.position());
            match points.entry(cell) {
                Entry::Occupied(mut o) => {
                    // there already is a point for this cell.
                    // take whichever point is closer to the center, reject the other one
                    let existing_entry = o.get_mut();
                    let dist = existing_entry.center.distance_to(point.position());
                    if dist < existing_entry.center_distance {
                        patch_rejected(&point, &mut existing_entry.point);
                        std::mem::swap(&mut point, &mut existing_entry.point);
                        existing_entry.center_distance = dist;
                    }
                    rejected.push(point);
                }
                Entry::Vacant(v) => {
                    // this is a new cell.
                    let center: Position = grid.cell_bounds(&cell).center();
                    let center_distance = center.distance_to(point.position());
                    v.insert(GridCenterEntry {
                        point,
                        center,
                        center_distance,
                    });
                }
            }
        }
        rejected
    }

    fn into_points(self) -> Vec<Self::Point> {
        self.points.into_values().map(|entry| entry.point).collect()
    }

    fn drain_points(&mut self) -> Vec<Self::Point> {
        let points = mem::take(&mut self.points);
        points.into_values().map(|c| c.point).collect()
    }

    fn clone_points(&self) -> Vec<Self::Point>
    where
        Self::Point: Clone,
    {
        self.points
            .values()
            .map(|entry| entry.point.clone())
            .collect()
    }
}
