use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
use crate::geometry::grid::GridHierarchy;
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{Component, Position};
use crate::geometry::sampling::{RawSamplingEntry, Sampling, SamplingFactory};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::index::sensor_pos::point::SensorPositionAttribute;
use nalgebra::Scalar;
use std::mem;
use std::time::Instant;

#[derive(Clone)]
pub struct PartitionedNode<Sampl, Point, Comp: Scalar> {
    sampling: Sampl,
    bogus: Vec<Point>,
    bounds: OptionAABB<Comp>,
    node_id: MetaTreeNodeId,
    dirty_since: Option<Instant>,
}

pub struct PartitionedNodeSplitter<Point, Pos, Raw> {
    sampled: Vec<Raw>,
    bogus: Vec<Point>,
    node_id: MetaTreeNodeId,
    replaces_base_node_at: Option<Pos>,
}

impl<Sampl, Point, Comp> PartitionedNode<Sampl, Point, Comp>
where
    Sampl: Sampling,
    Comp: Component,
{
    pub fn new<SamplF>(node_id: MetaTreeNodeId, sampling_factory: &SamplF, dirty: bool) -> Self
    where
        SamplF: SamplingFactory<Sampling = Sampl>,
    {
        PartitionedNode {
            sampling: sampling_factory.build(node_id.lod()),
            bogus: Vec::new(),
            bounds: OptionAABB::empty(),
            dirty_since: if dirty { Some(Instant::now()) } else { None },
            node_id,
        }
    }

    pub fn node_id(&self) -> &MetaTreeNodeId {
        &self.node_id
    }

    pub fn bounds(&self) -> &OptionAABB<Comp> {
        &self.bounds
    }

    pub fn nr_bogus_points(&self) -> usize {
        self.bogus.len()
    }

    pub fn nr_sampled_points(&self) -> usize {
        self.sampling.len()
    }

    pub fn nr_points(&self) -> usize {
        self.nr_sampled_points() + self.nr_bogus_points()
    }

    pub fn mark_dirty(&mut self) {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now())
        }
    }

    pub fn mark_clean(&mut self) {
        self.dirty_since = None;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_since.is_some()
    }

    pub fn dirty_since(&self) -> &Option<Instant> {
        &self.dirty_since
    }
}

impl<Sampl, Point, Comp> PartitionedNode<Sampl, Point, Comp>
where
    Comp: Component,
    Sampl: Sampling<Point = Point>,
    Point: std::clone::Clone + PointType,
    Point::Position: Position<Component = Comp>,
{
    pub fn get_las_points(&self) -> (Vec<Point>, OptionAABB<Comp>, u32) {
        let mut points = self.sampling.clone_points();
        let non_bogus_points = points.len() as u32;
        points.append(&mut self.bogus.clone());
        (points, self.bounds.clone(), non_bogus_points)
    }

    pub fn from_las_points<SamplF: SamplingFactory<Sampling = Sampl>>(
        node_id: MetaTreeNodeId,
        sampling_factory: &SamplF,
        mut points: Vec<Point>,
        nr_non_bogus_points: usize,
    ) -> Self {
        let mut this = Self::new(node_id.clone(), sampling_factory, false);
        if points.is_empty() {
            return this;
        }

        // split points into sampled and bogus points
        let bogus_points = points.split_off(nr_non_bogus_points);
        let sampled_points = points;

        // insert points
        let rejected = this.sampling.insert(sampled_points, |_, _| ());
        assert!(rejected.is_empty());
        this.bogus = bogus_points;
        this
    }

    pub fn insert_points<Patch>(
        &mut self,
        mut points_to_insert: Vec<Point>,
        patch_rejected: Patch,
    ) -> Vec<Point>
    where
        Patch: Fn(&Point, &mut Point) + Sync,
    {
        // mark dirty
        self.mark_dirty();

        // calculate aabb
        for point in &points_to_insert {
            self.bounds.extend(point.position());
        }

        // insert!
        self.sampling.insert(points_to_insert, patch_rejected)
    }

    pub fn insert_bogus_points(&mut self, mut points_to_insert: Vec<Point>) {
        // mark dirty
        self.mark_dirty();

        // calculate aabb
        for point in &points_to_insert {
            self.bounds.extend(point.position());
        }

        // insert!
        self.bogus.append(&mut points_to_insert);
    }

    pub fn drain_into_splitter(
        &mut self,
        sensor_position: Point::Position,
    ) -> PartitionedNodeSplitter<Point, Point::Position, Sampl::Raw> {
        self.mark_dirty();
        PartitionedNodeSplitter {
            sampled: self.sampling.drain_raw(),
            bogus: mem::replace(&mut self.bogus, Vec::new()),
            node_id: self.node_id.clone(),
            replaces_base_node_at: Some(sensor_position),
        }
    }
}

impl<Point, Pos, Raw, Comp> PartitionedNodeSplitter<Point, Pos, Raw>
where
    Raw: RawSamplingEntry<Point = Point>,
    Point: PointType<Position = Pos>,
    Pos: Position<Component = Comp>,
    Comp: Component,
{
    pub fn node_id(&self) -> &MetaTreeNodeId {
        &self.node_id
    }

    pub fn nr_points(&self) -> usize {
        self.bogus.len() + self.sampled.len()
    }

    pub fn replaces_base_node(&self) -> bool {
        self.replaces_base_node_at.is_some()
    }

    pub fn split<GridH>(mut self, meta_tree: &MetaTree<GridH, Comp>) -> [Self; 8]
    where
        GridH: GridHierarchy<Position = Pos, Component = Comp>,
        Point: WithAttr<SensorPositionAttribute<Pos>>,
    {
        // center of the node is where to split
        let node_center = meta_tree.node_center(&self.node_id);

        // prepare children to insert points into
        let mut children = self
            .node_id
            .children()
            .map(|child| PartitionedNodeSplitter {
                sampled: vec![],
                bogus: vec![],
                node_id: child,
                replaces_base_node_at: None,
            });

        // pass down the sensor position
        if let Some(sensor_pos) = self.replaces_base_node_at {
            let replace_child_id = node_select_child(&node_center, &sensor_pos);
            children[replace_child_id].replaces_base_node_at = Some(sensor_pos);
        }

        // split sampled points
        for point in self.sampled {
            let sensor_pos = point.point().attribute::<SensorPositionAttribute<Pos>>();
            let child_index = node_select_child(&node_center, &sensor_pos.0);
            children[child_index].sampled.push(point);
        }

        // split bogus points
        for point in self.bogus {
            let sensor_pos = point.attribute::<SensorPositionAttribute<Pos>>();
            let child_index = node_select_child(&node_center, &sensor_pos.0);
            children[child_index].bogus.push(point);
        }

        children
    }

    pub fn into_node<SamplF, Sampl>(
        self,
        sampling_factory: &SamplF,
    ) -> PartitionedNode<Sampl, Point, Comp>
    where
        SamplF: SamplingFactory<Sampling = Sampl>,
        Sampl: Sampling<Point = Point, Raw = Raw>,
    {
        // new empty node
        let mut node = PartitionedNode::new(self.node_id.clone(), sampling_factory, true);

        // calculate aabb
        for point in &self.sampled {
            node.bounds.extend(point.point().position());
        }
        for point in &self.bogus {
            node.bounds.extend(point.position());
        }

        // insert sampled points
        let rejected = node.sampling.insert_raw(self.sampled, |_, _| ());
        assert!(rejected.is_empty());

        // insert bogus points
        node.bogus = self.bogus;

        node
    }
}

fn node_select_child<Pos>(node_center: &Pos, sensor_pos: &Pos) -> usize
where
    Pos: Position,
{
    let mut child_num = 0;
    if sensor_pos.x() >= node_center.x() {
        child_num += 1;
    }
    if sensor_pos.y() >= node_center.y() {
        child_num += 2;
    }
    if sensor_pos.z() >= node_center.z() {
        child_num += 4;
    }
    child_num
}
