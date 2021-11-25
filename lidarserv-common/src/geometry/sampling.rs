use crate::geometry::grid::{GridCell, LodLevel};
use crate::geometry::points::PointType;
use crate::geometry::position::{Component, Position};
use std::collections::hash_map::{Entry, Values};
use std::collections::HashMap;
use std::hash::Hash;

use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
use nalgebra::Point;
use std::marker::PhantomData;
use std::mem;

pub trait SamplingFactory {
    type Point;
    type Sampling: Sampling<Point = Self::Point>;

    fn build(&self, param: &LodLevel) -> Self::Sampling;
}

/// Samples incoming points and stores the selected ones.
pub trait Sampling {
    type Point: PointType;
    type Raw: RawSamplingEntry;

    /// Return the number of points, that have been selected by the sampling.
    fn len(&self) -> usize;

    /// The minimum distance between two sampled points
    fn point_distance(&self) -> <<Self::Point as PointType>::Position as Position>::Component;

    /// Returns true, if the sampling is empty
    fn is_empty(&self) -> bool;

    /// calculate the bounding box of the contained points.
    fn bounding_box(
        &self,
    ) -> OptionAABB<<<Self::Point as PointType>::Position as Position>::Component>;

    /// Samples from the given list of points and stores the selected ones.
    /// The return value contains
    /// all points that got rejected from the sampling,
    /// as well as all preexisting points, that got replaced by a selected point.
    fn insert<F>(&mut self, points: Vec<Self::Point>, patch_rejected: F) -> Vec<Self::Point>
    where
        F: FnMut(&Self::Point, &mut Self::Point);

    /// Deletes all points in the sampling.
    fn clear(&mut self);

    /// Returns the list of sampled points.
    fn into_points(self) -> Vec<Self::Point>;

    /// Empties the node and returns the list of sampled points.
    fn drain_points(&mut self) -> Vec<Self::Point>;

    /// Returns a copy of the list of sampled points.
    fn clone_points(&self) -> Vec<Self::Point>
    where
        Self::Point: Clone;

    /// Returns the list of entries in this node.
    fn into_raw(self) -> Vec<Self::Raw>;

    /// Returns the list of entries in this node, leaving the node empty.
    fn drain_raw(&mut self) -> Vec<Self::Raw>;

    ///
    fn points_into_raw(&self, points: Vec<Self::Point>) -> Vec<Self::Raw>;

    /// Inserts raw entries into the node, that have been obtained from [Self::into_raw] on a
    /// different node of the same LOD.
    /// When points are already inserted in a sampling, but have to be re-inserted into a different
    /// sampling, then using [Self::into_raw] and [Self::insert_raw] can be more efficient than
    /// [Self::into_points] and [Self::insert], because it can carry over some internal meta-data,
    /// that does not need to be re-calculated.
    fn insert_raw<F>(&mut self, entries: Vec<Self::Raw>, patch_rejected: F) -> Vec<Self::Point>
    where
        F: FnMut(&Self::Point, &mut Self::Point);

    fn iter<'a>(&'a self) -> <&Self as IntoExactSizeIterator>::IntoIter
    where
        &'a Self: IntoExactSizeIterator<Item = &'a Self::Point>,
    {
        self.into_iter()
    }
}

pub trait RawSamplingEntry {
    type Cell: Hash;
    type Point;

    fn cell(&self) -> &Self::Cell;
    fn point(&self) -> &Self::Point;
}

#[derive(Clone)]
pub struct GridCenterEntry<Point, Position, Distance> {
    point: Point,
    center: Position,
    center_distance: Distance,
}

#[derive(Clone)]
pub struct GridCenterRawEntry<Point, Position, Distance> {
    cell: GridCell,
    entry: GridCenterEntry<Point, Position, Distance>,
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

impl<Point, Position, Distance> RawSamplingEntry for GridCenterRawEntry<Point, Position, Distance> {
    type Cell = GridCell;
    type Point = Point;

    fn cell(&self) -> &Self::Cell {
        &self.cell
    }

    fn point(&self) -> &Self::Point {
        &self.entry.point
    }
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
    Position: super::position::Position<Component = Distance>,
    Position::Component: Component,
    Point: PointType<Position = Position>,
    GridHierarchy::Grid: super::grid::Grid<Position = Position, Component = Position::Component>,
{
    type Point = Point;
    type Sampling = GridCenterSampling<GridHierarchy::Grid, Point, Position, Distance>;

    fn build(&self, level: &LodLevel) -> Self::Sampling {
        GridCenterSampling {
            grid: self.grid_hierarchy.level(level).into_grid(),
            points: HashMap::new(),
        }
    }
}

impl<Grid, Point, Position, Distance> Sampling
    for GridCenterSampling<Grid, Point, Position, Distance>
where
    Distance: PartialOrd + Component,
    Position: super::position::Position<Component = Distance>,
    Position::Component: Component,
    Point: PointType<Position = Position>,
    Grid: super::grid::Grid<Position = Position, Component = Position::Component>,
{
    type Point = Point;
    type Raw = GridCenterRawEntry<Point, Position, Distance>;

    fn len(&self) -> usize {
        self.points.len()
    }

    fn point_distance(
        &self,
    ) -> <<Self::Point as PointType>::Position as crate::geometry::position::Position>::Component
    {
        let example_cell = self.grid.cell_bounds(&GridCell { x: 0, y: 0, z: 0 });
        let min = example_cell.min::<Position>();
        let max = example_cell.max::<Position>();
        let p1 = Position::from_components(min.x(), min.y(), min.z());
        let p2 = Position::from_components(max.x(), min.y(), min.z());
        p1.distance_to(&p2)
    }

    fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    fn bounding_box(
        &self,
    ) -> OptionAABB<
        <<Self::Point as PointType>::Position as crate::geometry::position::Position>::Component,
    > {
        let mut bounds = OptionAABB::empty();
        for p in self.points.values() {
            bounds.extend(p.point.position());
        }
        bounds
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

    fn clear(&mut self) {
        self.points.clear();
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

    fn into_raw(self) -> Vec<Self::Raw> {
        self.points
            .into_iter()
            .map(|(k, v)| GridCenterRawEntry { cell: k, entry: v })
            .collect()
    }

    fn drain_raw(&mut self) -> Vec<Self::Raw> {
        let points = mem::take(&mut self.points);
        points
            .into_iter()
            .map(|(k, v)| GridCenterRawEntry { cell: k, entry: v })
            .collect()
    }

    fn points_into_raw(&self, points: Vec<Self::Point>) -> Vec<Self::Raw> {
        points
            .into_iter()
            .map(|point| {
                let cell = self.grid.cell_at(point.position());
                let center: Position = self.grid.cell_bounds(&cell).center();
                let center_distance = center.distance_to(point.position());
                GridCenterRawEntry {
                    cell,
                    entry: GridCenterEntry {
                        point,
                        center,
                        center_distance,
                    },
                }
            })
            .collect()
    }

    fn insert_raw<F>(&mut self, entries: Vec<Self::Raw>, mut patch_rejected: F) -> Vec<Self::Point>
    where
        F: FnMut(&Self::Point, &mut Self::Point),
    {
        let mut rejected = Vec::new();

        // insert each point
        for GridCenterRawEntry { cell, mut entry } in entries {
            // cell that the point belongs to
            match self.points.entry(cell) {
                Entry::Occupied(mut o) => {
                    let existing_entry = o.get_mut();
                    if entry.center_distance < existing_entry.center_distance {
                        patch_rejected(&entry.point, &mut existing_entry.point);
                        std::mem::swap(&mut entry, existing_entry);
                    }
                    rejected.push(entry.point);
                }
                Entry::Vacant(v) => {
                    v.insert(entry);
                }
            }
        }
        rejected
    }
}

pub trait IntoExactSizeIterator {
    type Item;
    type IntoIter: Iterator<Item = Self::Item> + ExactSizeIterator;
    fn into_iter(self) -> Self::IntoIter;
}

impl<'a, Grid, Point, Position, Distance> IntoExactSizeIterator
    for &'a GridCenterSampling<Grid, Point, Position, Distance>
{
    type Item = &'a Point;
    type IntoIter = GridCenterSamplingIter<'a, Point, Position, Distance>;

    fn into_iter(self) -> Self::IntoIter {
        let values = self.points.values();
        GridCenterSamplingIter { inner: values }
    }
}

pub struct GridCenterSamplingIter<'a, Point, Position, Distance> {
    inner: Values<'a, GridCell, GridCenterEntry<Point, Position, Distance>>,
}

impl<'a, Point, Position, Distance> Iterator
    for GridCenterSamplingIter<'a, Point, Position, Distance>
{
    type Item = &'a Point;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|e| &e.point)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a, Point, Position, Distance> ExactSizeIterator
    for GridCenterSamplingIter<'a, Point, Position, Distance>
{
}
