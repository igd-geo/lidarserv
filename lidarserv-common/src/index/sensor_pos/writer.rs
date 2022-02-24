use crate::geometry::bounding_box::BaseAABB;
use crate::geometry::grid::{I32GridHierarchy, LodLevel};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{I32CoordinateSystem, I32Position};
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId, MetaTreePart};
use crate::index::sensor_pos::page_manager::SensorPosPage;
use crate::index::sensor_pos::partitioned_node::PartitionedNode;
use crate::index::sensor_pos::point::SensorPositionAttribute;
use crate::index::sensor_pos::{Inner, Replacement, Update};
use crate::index::Writer;
use crate::las::{LasExtraBytes, LasPointAttributes, ReadLasError};
use crate::lru_cache::pager::{CacheCleanupError, CacheLoadError};
use crate::span;
use crossbeam_utils::Backoff;
use log::error;
use std::error::Error as StdError;
use std::fmt::Debug;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, TryLockError};
use std::thread::JoinHandle;
use std::time::Instant;
use std::{mem, thread};
use thiserror::Error;
use tracy_client::{create_plot, Plot};

pub(super) static QUEUE_LENGTH_PLOT: Plot = create_plot!("Point Queue length");

pub struct SensorPosWriter<Sampl, Point> {
    worker_threads: Vec<JoinHandle<()>>,
    update_relay_thread: Option<JoinHandle<()>>,
    update_thread: Option<JoinHandle<()>>,
    inboxes: Arc<Inboxes<Sampl, Point>>,
    coordinate_system: I32CoordinateSystem,
    pending_points: Arc<AtomicUsize>,
}

pub struct Inboxes<Sampl, Point> {
    lods: Vec<LodInbox<Sampl, Point>>,
    wakeup_state: Mutex<WakeupState>,
    wakeup: Condvar,
}

struct WakeupState {
    nr_waiting_tasks: i32,
    nr_processing_tasks: i32,
    should_exit: bool,
}

pub struct LodInbox<Sampl, Point> {
    priority: AtomicUsize,
    waiting_points: Mutex<WaitingPoints<Point>>,
    index_state: Mutex<IndexState<Sampl, Point>>,
}

pub struct WaitingPoints<Point> {
    points: Vec<Vec<Point>>,
}

pub struct IndexState<Sampl, Point> {
    meta_tree_part: MetaTreePart,
    node: PartitionedNode<Sampl, Point>,
}

pub enum Task<'a, Sampl, Point> {
    Exit,
    InsertPoints {
        points: Vec<Vec<Point>>,
        state: MutexGuard<'a, IndexState<Sampl, Point>>,
    },
}

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
    Other(Box<dyn StdError + Send + Sync>),
}

impl<K: Debug, V> From<CacheCleanupError<K, V>> for IndexError {
    fn from(e: CacheCleanupError<K, V>) -> Self {
        IndexError::PageIo {
            source: CacheLoadError::IO { source: e.source },
        }
    }
}

impl<Sampl, Point> SensorPosWriter<Sampl, Point>
where
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + WithAttr<LasPointAttributes>
        + LasExtraBytes
        + Clone
        + Send
        + Sync
        + 'static,
    Sampl: Sampling<Point = Point> + Send + Sync + Clone + 'static,
{
    pub(super) fn new<SamplF>(index_inner: Arc<Inner<SamplF, Point, Sampl>>) -> Self
    where
        SamplF: SamplingFactory<Point = Point, Sampling = Sampl> + Sync + Send + 'static,
    {
        // load initial nodes
        let meta_tree = { index_inner.shared.read().unwrap().meta_tree.clone() };
        let node_ids = meta_tree.query_sensor_position(&I32Position::default(), &[]);
        let nodes = (0..index_inner.max_lod.level() + 1)
            .map(|lod_level| {
                let node_id = node_ids.node_for_lod(&LodLevel::from_level(lod_level));
                let node = index_inner
                    .page_manager
                    .load_or_default(&node_id)
                    .unwrap()
                    .get_node(
                        node_id,
                        &index_inner.sampling_factory,
                        &index_inner.las_loader,
                    )
                    .unwrap();
                node.as_ref().clone()
            })
            .collect::<Vec<_>>();
        let inboxes = Inboxes::new(nodes, meta_tree);
        let inboxes = Arc::new(inboxes);

        // update relay thread
        let (changes_sender, changes_receiver) = crossbeam_channel::unbounded();
        let update_relay_thread = {
            let inner = Arc::clone(&index_inner);
            std::thread::spawn(move || {
                tracy_client::set_thread_name("Updates relay thread");
                notify_readers_thread(changes_receiver, inner);
            })
        };

        // update thread
        let update_thread = {
            let inboxes = Arc::clone(&inboxes);
            let inner = Arc::clone(&index_inner);
            let changes_sender = changes_sender.clone();
            std::thread::spawn(move || {
                tracy_client::set_thread_name("Update thread");
                update_thread(inboxes, inner, changes_sender);
            })
        };

        // start worker threads
        let pending_points = Arc::new(AtomicUsize::new(0));
        let worker_threads = (0..index_inner.nr_threads)
            .map(|tid| {
                let inboxes = Arc::clone(&inboxes);
                let inner = Arc::clone(&index_inner);
                let changes_sender = changes_sender.clone();
                let is_master = tid == 0;
                let pending_points = Arc::clone(&pending_points);
                std::thread::spawn(move || {
                    tracy_client::set_thread_name(&format!("Worker thread #{}", tid));
                    index_thread(inboxes, inner, changes_sender, is_master, pending_points)
                })
            })
            .collect();

        SensorPosWriter {
            worker_threads,
            update_relay_thread: Some(update_relay_thread),
            update_thread: Some(update_thread),
            inboxes,
            coordinate_system: index_inner.coordinate_system.clone(),
            pending_points,
        }
    }

    pub fn coordinate_system(&self) -> &I32CoordinateSystem {
        &self.coordinate_system
    }
}

impl<Sampl, Point> Writer<Point> for SensorPosWriter<Sampl, Point>
where
    Point: PointType,
{
    fn backlog_size(&self) -> usize {
        self.pending_points
            .load(std::sync::atomic::Ordering::Acquire)
    }

    fn insert(&mut self, points: Vec<Point>) {
        self.pending_points
            .fetch_add(points.len(), Ordering::Release);
        QUEUE_LENGTH_PLOT.point(self.pending_points.load(Ordering::Acquire) as f64);
        self.inboxes.add_points(points);
    }
}

impl<Sampl, Point> Drop for SensorPosWriter<Sampl, Point> {
    fn drop(&mut self) {
        let s = span!("drop writer");

        // stop the worker threads
        self.inboxes.start_shutdown();
        for worker_thread in mem::take(&mut self.worker_threads) {
            worker_thread.join().unwrap();
        }

        // wait for update thread to finish
        self.update_thread.take().unwrap().join().unwrap();
        self.update_relay_thread.take().unwrap().join().unwrap();
        drop(s);
    }
}

impl<Sampl, Point> Inboxes<Sampl, Point> {
    pub fn new(lod_nodes: Vec<PartitionedNode<Sampl, Point>>, meta_tree: MetaTree) -> Self {
        let meta_tree_parts = meta_tree.split_into_parts(lod_nodes.len());
        assert_eq!(
            meta_tree_parts.len(),
            lod_nodes.len(),
            "There are more LODs in the meta tree than lod_nodes given."
        );

        Inboxes {
            lods: lod_nodes
                .into_iter()
                .zip(meta_tree_parts)
                .map(|(node, meta_tree_part)| LodInbox {
                    priority: AtomicUsize::new(0),
                    waiting_points: Mutex::new(WaitingPoints { points: vec![] }),
                    index_state: Mutex::new(IndexState {
                        meta_tree_part,
                        node,
                    }),
                })
                .collect(),
            wakeup_state: Mutex::new(WakeupState {
                nr_waiting_tasks: 0,
                nr_processing_tasks: 0,
                should_exit: false,
            }),
            wakeup: Condvar::new(),
        }
    }

    pub fn add_points(&self, points: Vec<Point>) {
        self.add_points_to_lod(&LodLevel::base(), points)
    }

    pub fn add_points_to_lod(&self, lod: &LodLevel, points: Vec<Point>) {
        // ignore if there are no points to add
        if points.is_empty() {
            return;
        }

        // add points, update priority
        let was_first = {
            let inbox = &self.lods[lod.level() as usize];
            let mut w = inbox.waiting_points.lock().unwrap();
            let nr_points = points.len();
            inbox.priority.fetch_add(nr_points, Ordering::AcqRel);
            w.points.push(points);
            w.points.len() == 1
        };

        // ensure a thread is awake for processing the added points
        if was_first {
            self.wakeup_state.lock().unwrap().nr_waiting_tasks += 1;
            self.wakeup.notify_one();
        }
    }

    pub fn start_shutdown(&self) {
        self.wakeup_state.lock().unwrap().should_exit = true;
        self.wakeup.notify_all();
    }

    pub fn wait_for_next_task(&self) -> Task<Sampl, Point> {
        let backoff = Backoff::new();
        loop {
            // get priority for each lod
            let mut priorities = self
                .lods
                .iter()
                .map(|l| l.priority.load(Acquire))
                .enumerate()
                .filter(|(_, prio)| *prio > 0)
                .collect::<Vec<_>>();

            // try locking and creating tasks for inboxes in the order of their priority
            priorities.sort_by_key(|(_index, prio)| usize::MAX - prio);
            for (index, _) in priorities {
                let lod = &self.lods[index];
                match lod.index_state.try_lock() {
                    Ok(state) => match self.make_task(lod, state) {
                        None => continue,
                        Some(task) => return task,
                    },
                    Err(TryLockError::WouldBlock) => continue,
                    Err(TryLockError::Poisoned(_)) => panic!("Poisoned Mutex"),
                }
            }

            // prevent uncontrolled spinning if all waiting tasks are already blocked by a thread
            backoff.snooze();

            // wait until points are available somewhere
            {
                let mut l = self.wakeup_state.lock().unwrap();
                loop {
                    if l.nr_waiting_tasks > 0 {
                        break;
                    }
                    if l.should_exit && l.nr_waiting_tasks == 0 && l.nr_processing_tasks == 0 {
                        return Task::Exit;
                    }
                    backoff.reset();
                    l = self.wakeup.wait(l).unwrap();
                }
            }
        }
    }

    fn make_task<'a>(
        &self,
        inbox: &LodInbox<Sampl, Point>,
        state: MutexGuard<'a, IndexState<Sampl, Point>>,
    ) -> Option<Task<'a, Sampl, Point>> {
        // take points and reset priority
        let points = {
            let mut l = inbox.waiting_points.lock().unwrap();
            inbox.priority.store(0, Release);
            mem::take(&mut l.points)
        };

        // reject empty tasks
        if points.is_empty() {
            return None;
        }

        // update wakeup condition
        // (no need to wakeup any threads)
        {
            let mut l = self.wakeup_state.lock().unwrap();
            l.nr_waiting_tasks -= 1;
            l.nr_processing_tasks += 1;
        }

        // make task
        Some(Task::InsertPoints { points, state })
    }

    pub fn task_done(&self) {
        let mut l = self.wakeup_state.lock().unwrap();
        l.nr_processing_tasks -= 1;
        if l.should_exit {
            self.wakeup.notify_all()
        } else {
            self.wakeup.notify_one()
        }
    }

    pub(super) fn finalize<SamplF>(
        &self,
        inner: &Inner<SamplF, Point, Sampl>,
        updates_sender: crossbeam_channel::Sender<Update>,
    ) where
        Sampl: Sampling<Point = Point> + Clone,
        SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
        Point: PointType<Position = I32Position>
            + WithAttr<SensorPositionAttribute>
            + WithAttr<LasPointAttributes>
            + LasExtraBytes
            + Clone,
    {
        let s = span!("finalize");
        for inbox in &self.lods {
            let mut l = inbox.index_state.lock().unwrap();

            // store node
            l.node.mark_clean();
            let node_id = l.node.node_id().clone();
            let page = SensorPosPage::new_from_node(l.node.clone());
            inner.page_manager.store(&node_id, page);

            // update readers
            if let Some(aabb) = l.node.bounds().clone().into_aabb() {
                let update = Update {
                    node: node_id.clone(),
                    replaced_by: vec![Replacement {
                        replace_with: node_id,
                        bounds: aabb,
                    }],
                };
                updates_sender.send(update).ok();
            }
        }
        drop(s);
    }
}

pub(super) fn index_thread<SamplF, Sampl, Point>(
    inboxes: Arc<Inboxes<Sampl, Point>>,
    inner: Arc<Inner<SamplF, Point, Sampl>>,
    updates_sender: crossbeam_channel::Sender<Update>,
    is_master: bool,
    pending_points: Arc<AtomicUsize>,
) where
    Sampl: Sampling<Point = Point> + Clone,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + WithAttr<LasPointAttributes>
        + LasExtraBytes
        + Clone,
{
    let s = span!("index thread");
    loop {
        let task = inboxes.wait_for_next_task();
        match task {
            Task::Exit => break,
            Task::InsertPoints { points, mut state } => {
                let IndexState {
                    node,
                    meta_tree_part,
                } = &mut *state;
                index_task(
                    inboxes.as_ref(),
                    node,
                    meta_tree_part,
                    points,
                    inner.as_ref(),
                    &updates_sender,
                    pending_points.as_ref(),
                );
                inboxes.task_done();
                inner.page_manager.cleanup().unwrap();
            }
        }
    }
    if is_master {
        inboxes.finalize(inner.as_ref(), updates_sender);
    }
    drop(s);
}

fn index_task<SamplF, Sampl, Point>(
    inboxes: &Inboxes<Sampl, Point>,
    node: &mut PartitionedNode<Sampl, Point>,
    meta: &mut MetaTreePart,
    mut points: Vec<Vec<Point>>,
    inner: &Inner<SamplF, Point, Sampl>,
    updates_sender: &crossbeam_channel::Sender<Update>,
    pending_points: &AtomicUsize,
) where
    Sampl: Sampling<Point = Point> + Clone,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + WithAttr<LasPointAttributes>
        + LasExtraBytes
        + Clone,
{
    let s = span!("index task");
    while points.iter().any(|it| !it.is_empty()) {
        // select node to insert points into
        let node_id = choose_node(&points, meta, node.node_id());

        // select range of points, that still fits into this node
        let points_to_insert = choose_points(&mut points, &node_id, meta.sensor_pos_hierarchy());
        let last_sensor_position = points_to_insert
            .last()
            .unwrap()
            .attribute::<SensorPositionAttribute>()
            .0
            .clone();

        // load node if necessary
        if node_id != *node.node_id() {
            load_and_store_node(inner, &node_id, node, updates_sender);
        }

        // insert points
        let nr_points_to_insert = points_to_insert.len();
        let rejected = insert_points(inner, node, points_to_insert, meta);
        let actual_points_inserted = nr_points_to_insert - rejected.len();

        // Update pending points
        pending_points.fetch_sub(actual_points_inserted, Ordering::Release);
        QUEUE_LENGTH_PLOT.point(pending_points.load(Ordering::Acquire) as f64);

        // rejected points are to be inserted into the next lod
        if !rejected.is_empty() {
            let next_lod = node.node_id().lod().finer();
            inboxes.add_points_to_lod(&next_lod, rejected);
        }

        // split node if necessary
        if need_node_split(inner, node) {
            split_node(inner, node, last_sensor_position, meta, updates_sender);
        }
    }
    drop(s);
}

fn choose_node<Point>(
    points: &[Vec<Point>],
    meta: &MetaTreePart,
    previous_node: &MetaTreeNodeId,
) -> MetaTreeNodeId
where
    Point: PointType + WithAttr<SensorPositionAttribute>,
{
    let s = span!("choose node");
    let first_point = &points.iter().find(|it| !it.is_empty()).unwrap()[0];
    let sensor_pos = first_point.attribute::<SensorPositionAttribute>().0.clone();
    let node_id = meta.query_sensor_position(&sensor_pos, Some(previous_node));
    drop(s);
    node_id
}

fn choose_points<Point>(
    points: &mut Vec<Vec<Point>>,
    node_id: &MetaTreeNodeId,
    sensor_pos_hierarchy: &I32GridHierarchy,
) -> Vec<Point>
where
    Point: PointType + WithAttr<SensorPositionAttribute>,
{
    let s = span!("choose points");

    // bounds that the sensor position has to be in
    let bounds = sensor_pos_hierarchy.get_leveled_cell_bounds(node_id.tree_node());

    // select points from the beginning of points that are within bounds
    let mut selected = Vec::new();
    while !points.is_empty() {
        // check how many points from this batch we can take
        let batch = &mut points[0];
        let mut nr_points = batch.len();
        for (i, point) in batch.iter().enumerate() {
            let sensor_pos = &point.attribute::<SensorPositionAttribute>().0;
            if !bounds.contains(sensor_pos) {
                nr_points = i;
                break;
            }
        }

        // take points out of `points`
        let full_batch = nr_points == batch.len();
        let mut selected_from_this_batch = if full_batch {
            let this_batch = mem::take(batch);
            let remaining_batches = points.split_off(1);
            *points = remaining_batches;
            this_batch
        } else {
            let rest = batch.split_off(nr_points);
            mem::replace(batch, rest)
        };

        // put into selected
        selected.append(&mut selected_from_this_batch);

        // if we encountered a point. that was outside the bounds, we are done
        if !full_batch {
            break;
        }
    }

    drop(s);
    selected
}

fn load_and_store_node<SamplF, Sampl, Point>(
    inner: &Inner<SamplF, Point, Sampl>,
    node_id: &MetaTreeNodeId,
    node: &mut PartitionedNode<Sampl, Point>,
    update_sender: &crossbeam_channel::Sender<Update>,
) where
    Sampl: Sampling<Point = Point> + Clone,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + WithAttr<LasPointAttributes>
        + LasExtraBytes
        + Clone,
{
    let s = span!("load and store");

    // load new node
    let loaded = inner
        .page_manager
        .load_or_default(node_id)
        .unwrap()
        .get_node(node_id.clone(), &inner.sampling_factory, &inner.las_loader)
        .unwrap()
        .as_ref()
        .clone();

    // store old node
    let old_node = mem::replace(node, loaded);
    let old_node_id = old_node.node_id().clone();
    let old_node_bounds = old_node.bounds().clone();
    inner
        .page_manager
        .store(&old_node_id, SensorPosPage::new_from_node(old_node));

    // notify readers
    if let Some(aabb) = old_node_bounds.into_aabb() {
        let update = Update {
            node: old_node_id.clone(),
            replaced_by: vec![Replacement {
                replace_with: old_node_id,
                bounds: aabb,
            }],
        };
        update_sender.send(update).ok();
    }
    drop(s);
}

fn insert_points<SamplF, Point, Sampl>(
    inner: &Inner<SamplF, Point, Sampl>,
    node: &mut PartitionedNode<Sampl, Point>,
    points: Vec<Point>,
    meta: &mut MetaTreePart,
) -> Vec<Point>
where
    Sampl: Sampling<Point = Point>,
    Point: PointType<Position = I32Position> + WithAttr<SensorPositionAttribute>,
{
    let s = span!("insert points");

    // insert
    let rejected = if *node.node_id().lod() == inner.max_lod {
        node.insert_bogus_points(points);
        Vec::new()
    } else {
        node.insert_points(points, |acc, rej| {
            rej.set_attribute::<SensorPositionAttribute>(
                acc.attribute::<SensorPositionAttribute>().clone(),
            )
        })
    };

    // get new bounds
    let node_id = node.node_id().clone();
    let bounds = node.bounds().clone();
    if let Some(aabb) = bounds.into_aabb() {
        // update meta tree
        meta.set_node_aabb(&node_id, &aabb);
    }

    drop(s);
    rejected
}

fn need_node_split<SamplF, Sampl, Point>(
    inner: &Inner<SamplF, Point, Sampl>,
    node: &PartitionedNode<Sampl, Point>,
) -> bool
where
    Sampl: Sampling<Point = Point>,
    Point: PointType<Position = I32Position>,
{
    node.nr_points() > inner.max_node_size
        && *node.node_id().tree_depth() < inner.max_node_split_level
}

fn split_node<SamplF, Sampl, Point>(
    inner: &Inner<SamplF, Point, Sampl>,
    node: &mut PartitionedNode<Sampl, Point>,
    last_sensor_position: I32Position,
    meta: &mut MetaTreePart,
    update_sender: &crossbeam_channel::Sender<Update>,
) where
    Sampl: Sampling<Point = Point> + Clone,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + WithAttr<LasPointAttributes>
        + LasExtraBytes
        + Clone,
{
    let s = span!("split");

    // queue of nodes, that still need to be split
    let base_node_id = node.node_id().clone();
    let mut queue = vec![node.drain_into_splitter(last_sensor_position)];

    // nodes that are fully split
    let mut fully_split = Vec::new();

    // keep processing nodes that are queued for splitting, until queue is empty
    while let Some(split_node) = queue.pop() {
        // split
        let sensor_pos_hierarchy = meta.sensor_pos_hierarchy();
        let children = split_node.split(sensor_pos_hierarchy);

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
    let mut result_aabbs = Vec::new();
    let (replacement_index, _) = fully_split
        .iter()
        .enumerate()
        .find(|(_, node)| node.replaces_base_node())
        .unwrap();
    let replacement_node = fully_split
        .swap_remove(replacement_index)
        .into_node(&inner.sampling_factory);
    let old_node = mem::replace(node, replacement_node);
    node.mark_clean();
    inner
        .page_manager
        .store(node.node_id(), SensorPosPage::new_from_node(node.clone()));
    result_aabbs.push((node.node_id().clone(), node.bounds().clone()));

    // save
    for store_node in fully_split {
        let node_id = store_node.node_id().clone();
        let simple_points = store_node.into_points();
        result_aabbs.push((node_id.clone(), simple_points.bounds.clone()));
        inner
            .page_manager
            .store(&node_id, SensorPosPage::new_from_points(simple_points));
    }

    // save the old node (it is now empty)
    inner
        .page_manager
        .store(old_node.node_id(), SensorPosPage::new_from_binary(vec![]));

    // update meta tree
    for (node_id, bounds) in &result_aabbs {
        if let Some(aabb) = bounds.clone().into_aabb() {
            meta.set_node_aabb(node_id, &aabb)
        }
    }

    // notify readers
    let update = Update {
        node: base_node_id,
        replaced_by: result_aabbs
            .into_iter()
            .filter_map(|(node_id, bounds)| {
                bounds.into_aabb().map(|aabb| Replacement {
                    replace_with: node_id,
                    bounds: aabb,
                })
            })
            .collect(),
    };
    update_sender.send(update).ok();
    drop(s);
}

pub(super) fn update_thread<Sampl, SamplF, Point>(
    inboxes: Arc<Inboxes<Sampl, Point>>,
    inner: Arc<Inner<SamplF, Point, Sampl>>,
    update_sender: crossbeam_channel::Sender<Update>,
) where
    Sampl: Sampling<Point = Point> + Clone,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + WithAttr<LasPointAttributes>
        + LasExtraBytes
        + Clone,
{
    let s = span!("thread");
    let mut last_refresh = Instant::now();

    loop {
        // wait until next refresh
        let now = Instant::now();
        let next_refresh = last_refresh + inner.max_delay;
        if next_refresh > now {
            let time_to_wait = next_refresh - now;
            thread::sleep(time_to_wait);
            last_refresh = next_refresh;
        } else {
            last_refresh = now;
        }

        // refresh all lods
        let s1 = span!("refresh");
        for lod_level in 0..=inner.max_lod.level() {
            // hold the lock for this node
            {
                let mut l = inboxes.lods[lod_level as usize].index_state.lock().unwrap();
                if l.node.is_dirty() {
                    // store node
                    l.node.mark_clean();
                    let node_id = l.node.node_id().clone();
                    let page = SensorPosPage::new_from_node(l.node.clone());
                    inner.page_manager.store(&node_id, page);

                    // notify readers
                    if let Some(aabb) = l.node.bounds().clone().into_aabb() {
                        let update = Update {
                            node: node_id.clone(),
                            replaced_by: vec![Replacement {
                                replace_with: node_id,
                                bounds: aabb,
                            }],
                        };
                        update_sender.send(update).ok();
                    }
                }
            }

            // notify a worker thread so that it can start processing this lod
            // in case it was skipped before while we had the lock.
        }
        drop(s1);

        // check, if we are supposed to exit
        let s1 = span!("exit check");
        {
            let l = inboxes.wakeup_state.lock().unwrap();
            if l.should_exit {
                break;
            }
        }
        drop(s1);
    }
    drop(s);
}

pub(super) fn notify_readers_thread<SamplF, Point, Sampl>(
    changes_receiver: crossbeam_channel::Receiver<Update>,
    inner: Arc<Inner<SamplF, Point, Sampl>>,
) {
    let s = span!("thread");
    for change in changes_receiver {
        let s1 = span!("change");

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
        drop(s1);
    }

    // persist meta tree
    let s1 = span!("persist meta tree");
    let shared = inner.shared.read().unwrap();
    shared
        .meta_tree
        .write_to_file(&inner.meta_tree_file)
        .unwrap();
    drop(s1);
    drop(s);
}
