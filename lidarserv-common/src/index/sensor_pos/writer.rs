use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
use crate::geometry::grid::{GridHierarchy, LodLevel};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{Component, Position};
use crate::geometry::sampling::{RawSamplingEntry, Sampling, SamplingFactory};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::index::sensor_pos::page_manager::{BinDataPage, PageManager};
use crate::index::sensor_pos::partitioned_node::{
    node_select_child, PartitionedNode, PartitionedPoints,
};
use crate::index::sensor_pos::point::SensorPositionAttribute;
use crate::index::sensor_pos::{Inner, Replacement, Update};
use crate::index::Writer;
use crate::las::{Las, LasReadWrite, ReadLasError, WriteLasError};
use crate::lru_cache::pager::{CacheCleanupError, CacheLoadError};
use crate::nalgebra::Scalar;
use crate::span;
use crate::utils::thread_pool::Threads;
use crossbeam_channel::{Receiver, Sender};
use log::trace;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::array::IntoIter;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::error::Error as StdError;
use std::fmt::Debug;
use std::io::Cursor;
use std::mem;
use std::sync::Arc;
use std::thread::{spawn, JoinHandle};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("Could not read or write page: {source}")]
    PageIo {
        #[from]
        source: CacheLoadError,
    },

    #[error(transparent)]
    ReadLas {
        #[from]
        source: ReadLasError,
    },

    #[error(transparent)]
    WriteLas {
        #[from]
        source: WriteLasError,
    },

    #[error(transparent)]
    Other(Box<dyn StdError + Send + Sync>),
}

impl<K: Debug, V> From<CacheCleanupError<K, V>> for IndexError {
    fn from(e: CacheCleanupError<K, V>) -> Self {
        IndexError::PageIo {
            source: CacheLoadError::IO { source: e.source },
        }
    }
}

trait WithKey<OkValue> {
    fn with_key<K: Debug>(self, key: &K) -> Result<OkValue, IndexError>;
}

impl<OkValue> WithKey<OkValue> for Result<OkValue, CacheLoadError> {
    fn with_key<K: Debug>(self, key: &K) -> Result<OkValue, IndexError> {
        self.map_err(|e| IndexError::PageIo { source: e })
    }
}

pub struct SensorPosWriter<Point, CSys> {
    coordinator_join: Option<JoinHandle<Result<(), IndexError>>>,
    new_points_sender: Option<Sender<Vec<Point>>>,
    coordinate_system: CSys,
}

impl<Point, Pos, Comp, CSys> SensorPosWriter<Point, CSys>
where
    Point: PointType<Position = Pos>
        + WithAttr<SensorPositionAttribute<Pos>>
        + Clone
        + Send
        + Sync
        + 'static,
    Pos: Position<Component = Comp> + Clone + Sync,
    Comp: Component + Send + Sync + Serialize + DeserializeOwned,
    CSys: PartialEq + Send + Sync + 'static + Clone,
{
    pub(super) fn new<GridH, SamplF, LasL, Sampl, Raw>(
        index_inner: Arc<Inner<GridH, SamplF, Comp, LasL, CSys, Point>>,
    ) -> Self
    where
        GridH: GridHierarchy<Position = Pos, Component = Comp> + Clone + Send + Sync + 'static,
        SamplF: SamplingFactory<Point = Point, Param = LodLevel, Sampling = Sampl>
            + Sync
            + Send
            + 'static,
        Sampl: Sampling<Point = Point, Raw = Raw> + Send,
        Raw: RawSamplingEntry<Point = Point> + Send,
        LasL: LasReadWrite<Point, CSys> + Send + Sync + 'static,
    {
        let (new_points_sender, new_points_receiver) = crossbeam_channel::unbounded();
        let coordinator_join = {
            let inner = Arc::clone(&index_inner);
            let meta_tree = inner.shared.read().unwrap().meta_tree.clone();
            spawn(move || coordinator_thread(inner, new_points_receiver, meta_tree))
        };
        SensorPosWriter {
            coordinator_join: Some(coordinator_join),
            new_points_sender: Some(new_points_sender),
            coordinate_system: index_inner.coordinate_system.clone(),
        }
    }

    pub fn coordinate_system(&self) -> &CSys {
        &self.coordinate_system
    }
}

impl<Point, CSys> Writer<Point> for SensorPosWriter<Point, CSys>
where
    Point: PointType,
{
    fn insert(&mut self, points: Vec<Point>) {
        self.new_points_sender
            .as_mut()
            .unwrap()
            .send(points)
            .expect("Indexing thread crashed.");
    }
}

impl<Point, CSys> Drop for SensorPosWriter<Point, CSys> {
    fn drop(&mut self) {
        // close the channel for new points. That will make the writer threads stop.
        let sender = self.new_points_sender.take().unwrap();
        drop(sender);

        // Wait for the thread to actually stop.
        let join_handle = self.coordinator_join.take().unwrap();
        join_handle
            .join()
            .expect("Indexing thread crashed.")
            .expect("Indexing thread terminated with error");
    }
}

struct Node<Sampl, Comp: Scalar, Point> {
    node_id: MetaTreeNodeId,
    sampling: Sampl,
    aabb: OptionAABB<Comp>,
    bogus_points: Vec<Point>,
}

fn coordinator_thread<GridH, SamplF, Point, Sampl, Pos, Comp, LasL, CSys, Raw>(
    inner: Arc<Inner<GridH, SamplF, Comp, LasL, CSys, Point>>,
    new_points_receiver: Receiver<Vec<Point>>,
    mut meta_tree: MetaTree<GridH, Comp>,
) -> Result<(), IndexError>
where
    Comp: Component + Send + Sync + Serialize + DeserializeOwned,
    SamplF:
        SamplingFactory<Sampling = Sampl, Param = LodLevel, Point = Point> + Send + Sync + 'static,
    Sampl: Sampling<Point = Point, Raw = Raw> + Send,
    Raw: RawSamplingEntry<Point = Point>,
    Sampl::Raw: Send,
    Point: PointType<Position = Pos>
        + WithAttr<SensorPositionAttribute<Pos>>
        + Clone
        + Send
        + Sync
        + 'static,
    Pos: Position<Component = Comp> + Clone + Sync,
    GridH: GridHierarchy<Component = Comp, Position = Pos> + Send + Sync + 'static,
    LasL: LasReadWrite<Point, CSys> + Send + Sync + 'static,
    CSys: Clone + PartialEq + Send + Sync + 'static,
{
    // start thread that publishes changes to reders
    let (changes_sender, changes_receiver) = crossbeam_channel::unbounded();
    let inner_clone = Arc::clone(&inner);
    let notify_thread = spawn(move || notify_readers_thread(changes_receiver, inner_clone));

    let mut new_points = VecDeque::new();
    let mut loaded_nodes = Vec::<PartitionedNode<Sampl, Point, Comp>>::new();
    let nr_threads = inner.nr_threads;
    let mut threads = Threads::new(nr_threads);

    'main: loop {
        // make sure we have points to insert
        let s1 = span!("coordinator_thread: receive points");
        while new_points.is_empty() {
            let received = match new_points_receiver.recv() {
                Ok(p) => p,
                Err(_) => break 'main,
            };
            if !received.is_empty() {
                new_points.push_back(received);
            }
        }

        // empty the rest of the channel, so that we can insert as many points at once as possible.
        for received in new_points_receiver.try_iter() {
            if !received.is_empty() {
                new_points.push_back(received);
            }
        }

        // find the nodes to insert points into based on the current sensor position
        drop(s1);
        let s1 = span!("coordinator_thread: choose nodes");
        let first_point = &new_points[0][0];
        let sensor_pos = first_point
            .attribute::<SensorPositionAttribute<_>>()
            .0
            .clone();
        let nodes = meta_tree.query_sensor_position(&sensor_pos);

        // get the points from the head of new_points,
        // that can be inserted into the same nodes.
        drop(s1);
        let s1 = span!("coordinator_thread: how many points");
        let bounds = nodes.min_bounds();
        let mut nr_points = 0;
        'blocks: for block in new_points.iter() {
            for (point_index, point) in block.iter().enumerate() {
                let sensor_pos = &point.attribute::<SensorPositionAttribute<_>>().0;
                if !bounds.contains(sensor_pos) {
                    nr_points += point_index;
                    break 'blocks;
                }
            }
            nr_points += block.len();
        }
        s1.emit_value(nr_points as u64);

        // transfer points into buffers for individual worker threads
        drop(s1);
        let s1 = span!("coordinator_thread: copy points");
        let mut worker_buffers = Vec::new();
        let points_per_thread = nr_points / nr_threads;
        for thread_id in 0..nr_threads {
            // number of points that will go to this specific thread
            let mut points_left = points_per_thread;
            if thread_id == 0 {
                points_left += nr_points % nr_threads;
            }

            // copy points
            let mut points = Vec::new();
            while points_left > 0 && points_left >= new_points[0].len() {
                let mut first = new_points.pop_front().unwrap(); // unwrap: points_left cannot exceed the number of points in new_points, so if points_left > 0, new_points must be non-empty.
                points_left -= first.len();
                points.append(&mut first);
            }
            if points_left > 0 {
                let mut remaining = new_points[0].split_off(points_left);
                mem::swap(&mut new_points[0], &mut remaining);
                points_left -= remaining.len();
                points.append(&mut remaining);
            }
            debug_assert_eq!(points_left, 0);

            // use for thread `thread_id`
            worker_buffers.push(points);
        }
        let mut worker_buffers = PartitionedPoints::from_partitions(worker_buffers);

        // insert points into each lod, top-to-bottom
        // until no points are left in all worker buffers.
        let mut lod = LodLevel::base();
        drop(s1);
        while !worker_buffers.is_empty() {
            let s1 = span!("coordinator_thread: insert lod");
            s1.emit_value(lod.level() as u64);

            // get sampling for this node
            let node_id = nodes.node_for_lod(&lod);
            let lod_level = lod.level() as usize;
            let mut change = Update {
                node: node_id.clone(),
                replaced_by: vec![],
            };

            // load from disk, if needed
            let s2 = span!("coordinator_thread:: load to/from disk");
            if lod_level >= loaded_nodes.len() || *loaded_nodes[lod_level].node_id() != node_id {
                let s3 = span!("coordinator_thread::: parallel_load");
                let node = PartitionedNode::parallel_load(
                    nr_threads,
                    node_id.clone(),
                    &inner.sampling_factory,
                    &inner.page_manager,
                    &inner.las_loader,
                    &inner.coordinate_system,
                    &mut threads,
                )?;
                drop(s3);

                // keep around in loaded_nodes, so we can re-use it in the following iterations.
                match lod_level.cmp(&loaded_nodes.len()) {
                    Ordering::Less => {
                        // if the newly loaded node replaces a previous one, we  also need to
                        // write that to disk.
                        let s3 = span!("coordinator_thread::: save old node");
                        let mut old_node = mem::replace(&mut loaded_nodes[lod_level], node);
                        if let Some(aabb) = old_node.bounds().clone().into_aabb() {
                            meta_tree.set_node_aabb(old_node.node_id(), &aabb)
                        }
                        old_node.parallel_store(
                            &inner.page_manager,
                            &inner.las_loader,
                            &inner.coordinate_system,
                            &mut threads,
                        )?;
                        drop(s3);
                    }
                    Ordering::Equal => {
                        loaded_nodes.push(node);
                    }
                    Ordering::Greater => {
                        unreachable!()
                    }
                };

                // make sure we do not overfill the cache while loading nodes
                let s3 = span!("coordinator_thread::: cache cleanup");
                threads
                    .execute(|_| inner.page_manager.cleanup())
                    .join()
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()?;
                drop(s3);
            }
            let node = &mut loaded_nodes[lod_level];

            // add new points
            drop(s2);
            let s2 = span!("coordinator_thread:: add new points");
            if lod == inner.max_lod {
                // At the max lod level, we keep all points.
                // So no sampling needs to be performed.
                // We will use the bogus points as a simple "flat points buffer"
                node.parallel_insert_bogus(worker_buffers, &mut threads);
                worker_buffers = PartitionedPoints::new(nr_threads);
            } else {
                node.parallel_insert(
                    worker_buffers,
                    &inner.sampling_factory,
                    |p, q| {
                        *q.attribute_mut::<SensorPositionAttribute<Pos>>() =
                            p.attribute::<SensorPositionAttribute<Pos>>().clone()
                    },
                    &mut threads,
                );
                worker_buffers = node.drain_bogus_points();
            }

            // check, if we need to split the node
            drop(s2);
            if node.nr_points() > inner.max_node_size {
                let s2 = span!("coordinator_thread:: split");

                // queue of nodes, that still need to be split
                let mut queue = Vec::new();

                // keep refining node at current sensor position
                // until small enough
                let s3 = span!("coordinator_thread::: Phase 1");
                while node.nr_points() > inner.max_node_size {
                    // split node

                    let s4 = span!("coordinator_thread:::: Parallel split");
                    let mut children = Vec::from(node.parallel_split(
                        &meta_tree,
                        &inner.sampling_factory,
                        &mut threads,
                    ));
                    drop(s4);

                    // replace with newly created child node at current sensor position
                    let node_center = meta_tree.node_center(node.node_id());
                    let replace_with = node_select_child(&node_center, &sensor_pos);
                    let mut replacement_node = children.swap_remove(replace_with);
                    let old_node = mem::replace(node, replacement_node);

                    // node is now empty. save.
                    let s4 = span!("coordinator_thread:::: Parallel store");
                    meta_tree.set_node_aabb(
                        old_node.node_id(),
                        &old_node.bounds().clone().into_aabb().unwrap(),
                    ); // unwrap: the node cannot be empty, because it exceeded the max node size.
                    old_node.parallel_store(
                        &inner.page_manager,
                        &inner.las_loader,
                        &inner.coordinate_system,
                        &mut threads,
                    )?;
                    drop(s4);

                    // add remaining children to queue - we will deal with them in the next step
                    queue.append(&mut children);
                }
                drop(s3);

                // process remaining child nodes
                let s3 = span!("Phase 2");
                while !queue.is_empty() {
                    // get node from queue to process
                    // unwrap: queue is not empty due to loop condition
                    let mut node = queue.pop().unwrap();

                    // if the node is still to large we need to split it further.
                    if node.nr_points() > inner.max_node_size {
                        let mut children =
                            node.parallel_split(&meta_tree, &inner.sampling_factory, &mut threads);
                        queue.extend(children);
                    } else {
                        // When no further splitting needs to be performed for the node,
                        // we can add it to the changes that will be sent to the connected readers.
                        if let Some(bounds) = node.bounds().clone().into_aabb() {
                            change.replaced_by.push(Replacement {
                                replace_with: node.node_id().clone(),
                                bounds,
                                points: Arc::new(node.clone_points()),
                            });
                        }
                    }

                    // save node
                    if !node.bounds().is_empty() {
                        meta_tree.set_node_aabb(
                            node.node_id(),
                            &node.bounds().clone().into_aabb().unwrap(), // unwrap: we just checked, that the aabb is not empty
                        );
                        node.parallel_store(
                            &inner.page_manager,
                            &inner.las_loader,
                            &inner.coordinate_system,
                            &mut threads,
                        )?;
                    }
                }
                drop(s3);

                drop(s2);
            }

            // send changes to connected readers
            if let Some(bounds) = node.bounds().clone().into_aabb() {
                change.replaced_by.push(Replacement {
                    replace_with: node.node_id().clone(),
                    bounds,
                    points: Arc::new(node.clone_points()),
                });
            }
            trace!("{:#?}", &change);
            changes_sender.send(change).unwrap(); // unwrap: notify sender thread will only stop once the changes_sender is dropped.

            // next lod level in next loop iteration
            lod = lod.finer();
            drop(s1);
        }
    }

    // write remaining nodes to disk
    let s1 = span!("coordinator_thread: unload loaded nodes");
    for mut node in loaded_nodes {
        if let Some(aabb) = node.bounds().clone().into_aabb() {
            meta_tree.set_node_aabb(node.node_id(), &aabb)
        }
        node.parallel_store(
            &inner.page_manager,
            &inner.las_loader,
            &inner.coordinate_system,
            &mut threads,
        )?
    }

    // dump metatree to disk
    drop(s1);
    let s1 = span!("coordinator_thread: dump meta tree");
    meta_tree
        .write_to_file(&inner.meta_tree_file)
        .map_err(|e| IndexError::Other(Box::new(e)))?;
    drop(s1);

    // stop notify thread
    drop(changes_sender);
    notify_thread.join().unwrap();

    Ok(())
}

fn notify_readers_thread<GridH, SamplF, Comp, LasL, CSys, Point>(
    changes_receiver: crossbeam_channel::Receiver<Update<Comp, Point>>,
    inner: Arc<Inner<GridH, SamplF, Comp, LasL, CSys, Point>>,
) where
    Comp: Component,
    GridH: GridHierarchy<Component = Comp>,
    Point: Clone,
{
    for change in changes_receiver {
        let mut shared = inner.shared.write().unwrap();

        // update tree
        for Replacement {
            replace_with,
            bounds,
            ..
        } in &change.replaced_by
        {
            shared.meta_tree.set_node_aabb(replace_with, bounds);
        }

        // forward to all readers
        let mut pos = 0;
        while pos < shared.readers.len() {
            match shared.readers[pos].send(change.clone()) {
                Ok(_) => {
                    pos += 1;
                }
                Err(_) => {
                    shared.readers.swap_remove(pos);
                }
            }
        }
    }
}
