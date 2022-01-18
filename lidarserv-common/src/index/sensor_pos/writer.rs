use crate::geometry::bounding_box::BaseAABB;
use crate::geometry::grid::LodLevel;
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{I32CoordinateSystem, I32Position};
use crate::geometry::sampling::{RawSamplingEntry, Sampling, SamplingFactory};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::index::sensor_pos::page_manager::SensorPosPage;
use crate::index::sensor_pos::partitioned_node::{PartitionedNode, PartitionedNodeSplitter};
use crate::index::sensor_pos::point::SensorPositionAttribute;
use crate::index::sensor_pos::{Inner, Replacement, Update};
use crate::index::Writer;
use crate::las::{LasReadWrite, ReadLasError, WriteLasError};
use crate::lru_cache::pager::{CacheCleanupError, CacheLoadError};
use crate::span;
use crossbeam_channel::{Receiver, Sender};
use log::{error, trace};
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::error::Error as StdError;
use std::fmt::Debug;
use std::mem;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{atomic, Arc};
use std::thread::{spawn, JoinHandle};
use thiserror::Error;
use tracy_client::{create_plot, Plot};

static QUEUE_LENGTH_PLOT: Plot = create_plot!("Point Queue length");

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

pub struct SensorPosWriter<Point> {
    coordinator_join: Option<JoinHandle<Result<(), IndexError>>>,
    new_points_sender: Option<Sender<Vec<Point>>>,
    coordinate_system: I32CoordinateSystem,
    pending_points: Arc<AtomicUsize>,
}

impl<Point> SensorPosWriter<Point>
where
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + Clone
        + Send
        + Sync
        + 'static,
{
    pub(super) fn new<SamplF, LasL, Sampl>(
        index_inner: Arc<Inner<SamplF, LasL, Point, Sampl>>,
    ) -> Self
    where
        SamplF: SamplingFactory<Point = Point, Sampling = Sampl> + Sync + Send + 'static,
        Sampl: Sampling<Point = Point> + Send + Sync + Clone + 'static,
        LasL: LasReadWrite<Point> + Send + Sync + Clone + 'static,
    {
        // main worker thread
        let (new_points_sender, new_points_receiver) = crossbeam_channel::unbounded();
        let pending_points = Arc::new(AtomicUsize::new(0));
        let coordinator_join = {
            let inner = Arc::clone(&index_inner);
            let meta_tree = inner.shared.read().unwrap().meta_tree.clone();
            let pending_points = Arc::clone(&pending_points);
            spawn(move || coordinator_thread(inner, new_points_receiver, meta_tree, pending_points))
        };
        SensorPosWriter {
            coordinator_join: Some(coordinator_join),
            new_points_sender: Some(new_points_sender),
            coordinate_system: index_inner.coordinate_system.clone(),
            pending_points,
        }
    }

    pub fn coordinate_system(&self) -> &I32CoordinateSystem {
        &self.coordinate_system
    }
}

impl<Point> Writer<Point> for SensorPosWriter<Point>
where
    Point: PointType,
{
    fn backlog_size(&self) -> usize {
        self.pending_points
            .load(std::sync::atomic::Ordering::Acquire)
    }

    fn insert(&mut self, points: Vec<Point>) {
        self.pending_points
            .fetch_add(points.len(), std::sync::atomic::Ordering::Release);
        self.new_points_sender
            .as_mut()
            .unwrap()
            .send(points)
            .expect("Indexing thread crashed.");
    }
}

impl<Point> Drop for SensorPosWriter<Point> {
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

fn coordinator_thread<SamplF, Point, Sampl, LasL>(
    inner: Arc<Inner<SamplF, LasL, Point, Sampl>>,
    new_points_receiver: Receiver<Vec<Point>>,
    mut meta_tree: MetaTree,
    pending_points: Arc<AtomicUsize>,
) -> Result<(), IndexError>
where
    SamplF: SamplingFactory<Sampling = Sampl, Point = Point> + Send + Sync + 'static,
    Sampl: Sampling<Point = Point> + Send + Sync + Clone + 'static,
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + Clone
        + Send
        + Sync
        + 'static,
    LasL: LasReadWrite<Point> + Send + Sync + Clone + 'static,
{
    tracy_client::set_thread_name("Coordinator thread");

    // start thread that publishes changes to reders
    let (changes_sender, changes_receiver) = crossbeam_channel::unbounded();
    let inner_clone = Arc::clone(&inner);
    let notify_thread = spawn(move || notify_readers_thread(changes_receiver, inner_clone));

    // start the threads that writes nodes to disk
    assert!(inner.nr_threads > 0);
    let nr_cache_cleanup_threads = inner.nr_threads - 1;
    let stop_cache_cleanup_threads = Arc::new(AtomicBool::new(false));
    let mut cache_cleanup_threads = Vec::new();
    for thread_id in 0..nr_cache_cleanup_threads {
        let inner = Arc::clone(&inner);
        let stop_cache_cleanup_threads = Arc::clone(&stop_cache_cleanup_threads);
        let handle = spawn(move || {
            tracy_client::set_thread_name(&format!("Cache cleanup #{}", thread_id));
            cache_cleanup_thread(inner, stop_cache_cleanup_threads);
        });
        cache_cleanup_threads.push(handle);
    }

    // init
    let mut new_points = VecDeque::new();
    let mut loaded_nodes = Vec::<PartitionedNode<Sampl, Point>>::new();
    let mut previous_split_levels = Vec::new();

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
        drop(s1);

        // find the nodes to insert points into based on the current sensor position
        let s1 = span!("coordinator_thread: choose nodes");
        let first_point = &new_points[0][0];
        let sensor_pos = first_point.attribute::<SensorPositionAttribute>().0.clone();
        let nodes = meta_tree.query_sensor_position(&sensor_pos, &previous_split_levels);
        previous_split_levels = nodes.split_levels();
        drop(s1);

        // get the points from the head of new_points,
        // that can be inserted into the same nodes.
        let s1 = span!("coordinator_thread: how many points");
        let bounds = nodes.min_bounds();
        let mut nr_points = 0;
        'blocks: for block in new_points.iter() {
            for (point_index, point) in block.iter().enumerate() {
                let sensor_pos = &point.attribute::<SensorPositionAttribute>().0;
                if !bounds.contains(sensor_pos) {
                    nr_points += point_index;
                    break 'blocks;
                }
            }
            nr_points += block.len();
            if nr_points > inner.max_node_size * 2 {
                // not too many points at once, to avoid excessive node splits.
                nr_points = inner.max_node_size * 2;
                break 'blocks;
            }
        }

        s1.emit_value(nr_points as u64);
        pending_points.fetch_sub(nr_points, atomic::Ordering::Release);
        drop(s1);

        // transfer points into buffers for individual worker threads
        let s1 = span!("coordinator_thread: copy points");
        let mut worker_buffer = Vec::new();
        let mut points_left = nr_points;
        while points_left > 0 && points_left >= new_points[0].len() {
            let mut first = new_points.pop_front().unwrap(); // unwrap: points_left cannot exceed the number of points in new_points, so if points_left > 0, new_points must be non-empty.
            points_left -= first.len();
            worker_buffer.append(&mut first);
        }
        if points_left > 0 {
            let mut remaining = new_points[0].split_off(points_left);
            mem::swap(&mut new_points[0], &mut remaining);
            points_left -= remaining.len();
            worker_buffer.append(&mut remaining);
        }
        debug_assert_eq!(points_left, 0);
        drop(s1);

        // load nodes
        let mut lod = LodLevel::base();
        while lod <= inner.max_lod {
            // load from disk, if needed
            let s1 = span!("coordinator_thread: load lod");
            s1.emit_value(lod.level() as u64);
            let node_id = nodes.node_for_lod(&lod);
            let lod_level = lod.level() as usize;
            if lod_level >= loaded_nodes.len() || *loaded_nodes[lod_level].node_id() != node_id {
                let s3 = span!("coordinator_thread::: parallel_load");
                let node = inner
                    .page_manager
                    .load_or_default(&node_id)?
                    .get_node(node_id, &inner.sampling_factory, &inner.las_loader)?
                    .as_ref()
                    .clone();
                drop(s3);

                // keep around in loaded_nodes, so we can re-use it in the following iterations.
                match lod_level.cmp(&loaded_nodes.len()) {
                    Ordering::Less => {
                        // if the newly loaded node replaces a previous one, we  also need to
                        // write that to disk.
                        let s3 = span!("coordinator_thread::: save old node");
                        let mut old_node = mem::replace(&mut loaded_nodes[lod_level], node);
                        if old_node.is_dirty() {
                            apply_updates(
                                old_node.node_id().clone(),
                                old_node,
                                vec![],
                                inner.as_ref(),
                                &mut meta_tree,
                                &changes_sender,
                            )?;
                        }
                        drop(s3);
                    }
                    Ordering::Equal => {
                        loaded_nodes.push(node);
                    }
                    Ordering::Greater => {
                        unreachable!()
                    }
                };
            }

            // next lod
            lod = lod.finer();
            drop(s1);
        }

        // insert points into each lod, top-to-bottom
        // until no points are left in all worker buffers.
        let s1 = span!("coordinator_thread: insert points");
        assert!(loaded_nodes.len() > 0);
        let last_lod_node = loaded_nodes.len() - 1;
        for node in &mut loaded_nodes[..last_lod_node] {
            worker_buffer = node.insert_points(worker_buffer, |p, q| {
                q.set_attribute(p.attribute::<SensorPositionAttribute>().clone());
            });
        }
        loaded_nodes[last_lod_node].insert_bogus_points(worker_buffer);
        drop(s1);

        // split nodes, that got too big
        let mut lod = LodLevel::base();
        while lod <= inner.max_lod {
            let lod_level = lod.level() as usize;
            let node = &mut loaded_nodes[lod_level];
            if node.nr_points() > inner.max_node_size
                && node.node_id().tree_node().lod < inner.max_node_split_level
            {
                let s2 = span!("coordinator_thread:: split");

                // queue of nodes, that still need to be split
                let mut queue = vec![node.drain_into_splitter(sensor_pos.clone())];

                // nodes that are fully split (nr of points is below the max_node_size)
                let mut fully_split = Vec::new();

                // keep processing nodes that are queued for splitting, until queue is empty
                while let Some(split_node) = queue.pop() {
                    // split
                    let children = split_node.split(&meta_tree);

                    // the child nodes, that are small enough are put into `fully_split`, the other
                    // ones are re-queued.
                    for child in children {
                        if child.nr_points() > inner.max_node_size
                            && child.node_id().tree_node().lod < inner.max_node_split_level
                        {
                            queue.push(child)
                        } else {
                            fully_split.push(child)
                        }
                    }
                }

                // find the node that replaces the old one
                // unwrap: node splitting is implemented, such that replaces_base_node() returns true for exactly one node.
                let (replacement_index, _) = fully_split
                    .iter()
                    .enumerate()
                    .find(|(_, node)| node.replaces_base_node())
                    .unwrap();
                let replacement_node = fully_split
                    .swap_remove(replacement_index)
                    .into_node(&inner.sampling_factory);
                let old_node = mem::replace(node, replacement_node);

                // save
                apply_updates(
                    old_node.node_id().clone(),
                    node.clone(),
                    fully_split,
                    inner.as_ref(),
                    &mut meta_tree,
                    &changes_sender,
                )?;

                // save the old node (it is now empty)
                inner
                    .page_manager
                    .store(old_node.node_id(), SensorPosPage::new_from_binary(vec![]));
                drop(s2);
            }

            // next lod
            lod = lod.finer()
        }

        // clear cache
        let min_cleanup_tasks = nr_cache_cleanup_threads * 25; // chosen pretty arbitrarily, but large enough so that we do not have to do anything after a normal node split. This part should only have to do work, if the cleanup threads can't keep up.
        let (max_size, mut current_size) = inner.page_manager.size();
        while current_size > max_size + min_cleanup_tasks {
            let s1 = span!("coordinator_thread: cache cleanup");
            match inner.page_manager.cleanup_one() {
                Ok(_) => (),
                Err(e) => {
                    error!("Could not flush page to disk: {:?}", e);
                }
            }

            let (_, new_size) = inner.page_manager.size();
            current_size = new_size;
            drop(s1);
        }

        // Publish the changes to the connected viewers.
        // (For the indexing itself, this part is irrelevant: A node gets saved when it is "unloaded".)
        let s1 = span!("coordinator_thread: publish changes");
        let mut dirty_nodes: Vec<_> = loaded_nodes
            .iter_mut()
            .flat_map(|node| node.dirty_since().map(|dirty_since| (dirty_since, node)))
            .collect();
        dirty_nodes.sort_by_key(|(dirty_since, _)| *dirty_since);
        for (dirty_since, node) in dirty_nodes {
            // if there are more points to insert,
            // we abort early, so that only the nodes are published, that we really have to.
            // (Those, that have been dirty for longer than inner.max_delay)
            if !new_points.is_empty() || !new_points_receiver.is_empty() {
                let dirty_time = dirty_since.elapsed();
                if dirty_time <= inner.max_delay {
                    break;
                }
            }

            // save node
            apply_updates(
                node.node_id().clone(),
                node.clone(),
                Vec::new(),
                inner.as_ref(),
                &mut meta_tree,
                &changes_sender,
            )?;
            node.mark_clean();
        }
        drop(s1);

        // plot queue length
        QUEUE_LENGTH_PLOT.point(pending_points.load(std::sync::atomic::Ordering::Acquire) as f64);
    }

    // write remaining nodes to disk
    let s1 = span!("coordinator_thread: unload loaded nodes");
    for node in loaded_nodes {
        apply_updates(
            node.node_id().clone(),
            node,
            Vec::new(),
            inner.as_ref(),
            &mut meta_tree,
            &changes_sender,
        )?;
    }
    drop(s1);

    // stop cleanup threads
    let s1 = span!("coordinator_thread: terminate cache cleanup threads");
    stop_cache_cleanup_threads.store(true, atomic::Ordering::Release);
    inner.page_manager.block_on_cache_size_external_wakeup();
    for join_handle in cache_cleanup_threads {
        join_handle.join().unwrap();
    }
    drop(s1);

    // dump metatree to disk
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

fn apply_updates<Point, Sampl, SamplF, LasL, Raw>(
    base_node: MetaTreeNodeId,
    replace_with: PartitionedNode<Sampl, Point>,
    replace_with_split: Vec<PartitionedNodeSplitter<Point, Raw>>,
    inner: &Inner<SamplF, LasL, Point, Sampl>,
    meta_tree: &mut MetaTree,
    notify_sender: &crossbeam_channel::Sender<Update>,
) -> Result<(), IndexError>
where
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
    Sampl: Sampling<Point = Point, Raw = Raw> + Send + Clone,
    Point: PointType<Position = I32Position> + Send + Clone,
    Raw: RawSamplingEntry<Point = Point>,
    LasL: LasReadWrite<Point> + Sync + Clone,
{
    let mut aabbs = vec![(
        replace_with.node_id().clone(),
        replace_with.bounds().clone(),
    )];

    // store node contents
    inner.page_manager.store(
        &replace_with.node_id().clone(),
        SensorPosPage::new_from_node(replace_with),
    );

    // store split node contents
    for node in replace_with_split {
        let node_id = node.node_id().clone();
        let node = node.into_node(&inner.sampling_factory);
        let aabb = node.bounds().clone();
        inner
            .page_manager
            .store(&node_id, SensorPosPage::new_from_node(node));
        aabbs.push((node_id, aabb))
    }

    // Publish changes to viewers + global meta tree
    let update = Update {
        node: base_node,
        replaced_by: aabbs
            .iter()
            .flat_map(|(node_id, bounds)| {
                bounds.clone().into_aabb().map(|aabb| Replacement {
                    replace_with: node_id.clone(),
                    bounds: aabb,
                })
            })
            .collect(),
    };
    trace!("{:#?}", &update);
    // unwrap: notify_readers_thread will only terminate, once the sender is dropped.
    notify_sender.send(update).unwrap();

    // update local meta tree
    for (node_id, aabb) in aabbs {
        if let Some(aabb) = aabb.into_aabb() {
            meta_tree.set_node_aabb(&node_id, &aabb);
        }
    }

    Ok(())
}

fn notify_readers_thread<SamplF, LasL, Point, Sampl>(
    changes_receiver: crossbeam_channel::Receiver<Update>,
    inner: Arc<Inner<SamplF, LasL, Point, Sampl>>,
) {
    for change in changes_receiver {
        let mut shared = inner.shared.write().unwrap();

        // update tree
        shared.meta_tree.apply_update(&change);

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

fn cache_cleanup_thread<SamplF, LasL, Point, Sampl>(
    inner: Arc<Inner<SamplF, LasL, Point, Sampl>>,
    shutdown: Arc<AtomicBool>,
) where
    LasL: LasReadWrite<Point> + Clone,
    Point: PointType<Position = I32Position> + Clone,
    Sampl: Sampling<Point = Point>,
{
    loop {
        inner
            .page_manager
            .block_on_cache_size(|cur_size, max_size| {
                cur_size > max_size || shutdown.load(atomic::Ordering::Acquire)
            });
        match inner.page_manager.cleanup() {
            Ok(()) => (),
            Err(e) => {
                error!("Could not flush page to disk. You just lost data: {:?}", e);
            }
        }
        if shutdown.load(atomic::Ordering::Acquire) {
            return;
        }
    }
}
