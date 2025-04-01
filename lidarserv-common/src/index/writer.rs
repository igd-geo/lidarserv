use super::{
    Inner,
    lazy_node::LazyNode,
    live_metrics_collector::{LiveMetricsCollector, MetricName},
    priority_function::TaskPriorityFunction,
};
use crate::{
    geometry::{
        grid::{GridHierarchy, LeveledGridCell, LodLevel},
        position::{Component, WithComponentTypeOnce},
    },
    io::PointIoError,
    lru_cache::pager::CacheCleanupError,
};
use log::info;
use nalgebra::Point3;
use pasture_core::containers::{
    BorrowedBuffer, BorrowedBufferExt, InterleavedBuffer, OwningBuffer, VectorBuffer,
};
use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use thiserror::Error;
use tracy_client::{plot, secondary_frame_mark, span};

pub(super) struct InsertionTask {
    /// Points to insert in the node
    pub points: Vec<VectorBuffer>,

    /// generation of the newest point
    pub max_generation: u32,

    /// generation of the oldest point in the task
    pub min_generation: u32,

    /// generation where the task was created
    pub created_generation: u32,
}

struct LockedTask {
    nr_points: usize,
}

struct Inboxes {
    tasks: HashMap<LeveledGridCell, InsertionTask>,
    locked: HashMap<LeveledGridCell, LockedTask>,
    drain: bool,
    waiter: Arc<Condvar>,
    current_gen: u32,
    current_gen_started: Instant,
    current_gen_points: u32,
    priority_function: TaskPriorityFunction,
    metrics: Arc<LiveMetricsCollector>,
}

pub struct OctreeWriter {
    inboxes: Arc<Mutex<Inboxes>>,
    threads: Vec<JoinHandle<Result<(), IndexingThreadError>>>,
    node_hierarchy: GridHierarchy,
}

#[derive(Error, Debug)]
enum IndexingThreadError {
    #[error("Error while reading or writing points: {0}")]
    PointIoError(#[from] PointIoError),

    #[error("Error while writing cache to disk: {0}")]
    CacheCleanup(#[from] CacheCleanupError<LeveledGridCell, LazyNode, PointIoError>),
}

struct OctreeWorkerThread {
    inner: Arc<Inner>,
    inboxes: Arc<Mutex<Inboxes>>,
    condvar: Arc<Condvar>,
}

// --- impls ---

impl Inboxes {
    fn new(
        waiter: Arc<Condvar>,
        priority_function: TaskPriorityFunction,
        metrics: Arc<LiveMetricsCollector>,
    ) -> Self {
        Inboxes {
            tasks: Default::default(),
            locked: Default::default(),
            drain: false,
            waiter,
            current_gen: 0,
            current_gen_started: Instant::now(),
            current_gen_points: 0,
            priority_function,
            metrics,
        }
    }

    fn add(
        &mut self,
        node: LeveledGridCell,
        add_points: VectorBuffer,
        min_generation: u32,
        max_generation: u32,
    ) {
        if node.lod == LodLevel::base() {
            self.current_gen_points += add_points.len() as u32
        }
        match self.tasks.entry(node) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().points.push(add_points);
                entry.get_mut().max_generation = max_generation;
            }
            Entry::Vacant(entry) => {
                entry.insert(InsertionTask {
                    points: vec![add_points],
                    min_generation,
                    max_generation,
                    created_generation: self.current_gen,
                });
                self.waiter.notify_one();
            }
        }
    }

    pub fn take_and_lock(&mut self) -> Option<(LeveledGridCell, InsertionTask)> {
        let node_id = self
            .tasks
            .iter()
            .filter(|&(cell, _)| !self.locked.contains_key(cell))
            .max_by(|&(cell_a, a), &(cell_b, b)| self.priority_function.cmp(cell_a, a, cell_b, b))
            .map(|(&id, _)| id);

        let node_id = node_id?;
        let task = self.tasks.remove(&node_id).unwrap();
        self.locked.insert(
            node_id,
            LockedTask {
                nr_points: task.nr_points(),
            },
        );
        Some((node_id, task))
    }

    pub fn unlock(&mut self, node_id: LeveledGridCell) {
        self.locked.remove(&node_id);
        self.waiter.notify_all();
    }

    pub fn drain(&mut self) {
        info!("Finishing up... {} tasks remaining", self.tasks.len());
        self.drain = true;
        self.priority_function = TaskPriorityFunction::Cleanup;
        self.waiter.notify_all();
    }

    pub fn should_exit(&self) -> bool {
        self.drain && self.locked.is_empty() && self.tasks.is_empty()
    }

    pub fn nr_of_points(&self) -> usize {
        let locked_tasks_points: usize = self.locked.values().map(|l| l.nr_points).sum();
        let pending_tasks_points: usize = self.tasks.values().map(|t| t.nr_points()).sum();
        locked_tasks_points + pending_tasks_points
    }

    pub fn maybe_next_generation(&mut self) {
        let now = Instant::now();
        let generation_duration: Duration = Duration::from_secs_f64(0.1);
        while now.duration_since(self.current_gen_started) > generation_duration {
            plot!(
                "Incoming points per generation",
                self.current_gen_points as f64
            );
            self.current_gen += 1;
            self.current_gen_points = 0;
            self.current_gen_started += generation_duration;
            secondary_frame_mark!("mno generation");
        }
    }

    #[inline]
    fn plot_tasks_len(&self) {
        let val = self.tasks.len() as f64;
        self.metrics.metric(MetricName::NrIncomingTasks, val);
        plot!("Task queue length", val);
    }

    #[inline]
    fn plot_nr_of_points(&self) {
        let val = self.nr_of_points() as f64;
        self.metrics.metric(MetricName::NrIncomingPoints, val);
        plot!("Task queue length in points", val);
    }
}

impl OctreeWorkerThread {
    /// The indexing thread
    pub fn thread(&self) -> Result<(), IndexingThreadError> {
        loop {
            // get a task
            let _span = span!("OctreeWorkerThread::thread - wait for task");
            let mut lock = self.inboxes.lock().unwrap();
            let (node_id, task) = loop {
                // get task
                if let Some(task) = lock.take_and_lock() {
                    lock.plot_tasks_len();
                    break task;
                }

                // if there are no tasks... maybe we are done!
                if lock.should_exit() {
                    return Ok(());
                }

                // also not exit?
                // wait for things to change
                lock = self.condvar.wait(lock).unwrap();
            };
            drop(lock);
            drop(_span);

            // process task
            let _span = span!("OctreeWorkerThread::thread - process task");
            let is_max_lod = self.inner.max_lod == node_id.lod;
            let task_min_generation = task.min_generation;
            let task_max_generation = task.max_generation;
            let (child_tasks, should_notify) = self.writer_task(node_id, task, is_max_lod)?;
            debug_assert!(!is_max_lod || child_tasks.is_none()); // if we are at the max lod, no more children are allowed

            // unlock the node, create child tasks
            {
                let mut lock = self.inboxes.lock().unwrap();
                lock.unlock(node_id);

                if let Some(tasks) = child_tasks {
                    for (child_id, child_points) in tasks {
                        if !child_points.is_empty() {
                            lock.add(
                                child_id,
                                child_points,
                                task_min_generation,
                                task_max_generation,
                            );
                        }
                    }
                }
                lock.plot_nr_of_points();
            }
            drop(_span);

            // notify subscriptions
            let _span = span!("OctreeWorkerThread::thread - notify subscriptions");
            if should_notify {
                let mut lock = self.inner.subscriptions.lock().unwrap();
                let mut it_end = lock.len();
                let mut it = 0;
                while it < it_end {
                    let sender = &mut lock[it];
                    let result = sender.send(node_id);
                    if result.is_ok() {
                        it += 1
                    } else {
                        it_end -= 1;
                        lock.swap_remove(it);
                    }
                }
            }
            drop(_span);

            // Free space in page cache
            self.inner.page_cache.cleanup()?;
        }
    }

    #[allow(clippy::type_complexity)] // only internal anyways
    fn writer_task(
        &self,
        node_id: LeveledGridCell,
        task: InsertionTask,
        is_max_lod: bool,
    ) -> Result<(Option<[(LeveledGridCell, VectorBuffer); 8]>, bool), IndexingThreadError> {
        // update attribute indexes
        for points in &task.points {
            self.inner.attribute_index.index(node_id, points);
        }

        // get points
        let _span = span!("OctreeWorkerThread::writer_task - get points");
        let node_arc = self.inner.page_cache.load_or_default(&node_id)?.get_node(
            &*self.inner.codec,
            &self.inner.point_layout,
            &self.inner.point_hierarchy,
            node_id.lod,
        )?;
        let _s2 = span!("OctreeWorkerThread::writer_task - get points - clone()");
        let mut node = (*node_arc).dyn_clone();
        drop(_s2);
        drop(_span);

        // update attribute index
        /*
        let _span = span!("OctreeWorkerThread::writer_task - update attribute index");
        if self.inner.attribute_index.is_some() {
            if self.inner.enable_histogram_acceleration {
                // WITH HISTOGRAMS
                let mut bounds: LasPointAttributeBounds = LasPointAttributeBounds::new();
                let mut histogram: LasPointAttributeHistograms =
                    LasPointAttributeHistograms::new(&self.inner.histogram_settings);
                let _ = &task.points.iter().for_each(|p| {
                    bounds.update_by_attributes(p.attribute());
                    histogram.fill_with(p.attribute());
                });
                self.inner
                    .attribute_index
                    .as_ref()
                    .unwrap()
                    .update_bounds_and_histograms(
                        node_id.lod,
                        &node_id.pos,
                        &bounds,
                        &Some(histogram),
                    );
            } else {
                // NO HISTOGRAMS
                let mut bounds: LasPointAttributeBounds = LasPointAttributeBounds::new();
                let _ = &task
                    .points
                    .iter()
                    .for_each(|p| bounds.update_by_attributes(p.attribute()));
                self.inner
                    .attribute_index
                    .as_ref()
                    .unwrap()
                    .update_bounds_and_histograms(node_id.lod, &node_id.pos, &bounds, &None);
            }
        }
        drop(_span);
        */

        // insert new points
        let _span = span!("OctreeWorkerThread::writer_task - insert new points");
        node.reset_dirty();
        node.insert_multi(&task.points);
        let should_notify_clients = node.is_dirty();

        // only create child tasks, if we have a considerable amount of points.
        let make_children = if is_max_lod {
            false
        } else {
            let is_leaf = self.inner.page_cache.directory().is_leaf_node(&node_id);
            let max_bogus = if is_leaf {
                self.inner.max_bogus_leaf
            } else {
                self.inner.max_bogus_inner
            };
            node.nr_bogus_points() > max_bogus
        };
        let children_points = if make_children {
            Some(node.take_bogus_points())
        } else {
            None
        };

        // write back to cache
        let page = LazyNode::from_node(node.into());
        self.inner.page_cache.store(&node_id, page);

        // split into 8 child nodes
        if !make_children {
            return Ok((None, should_notify_clients));
        }
        let children_points =
            children_points.expect("if make_children is true, there should be children_points.");
        let children =
            Self::split_points_into_children(&children_points, self.inner.node_hierarchy, node_id);

        Ok((Some(children), should_notify_clients))
    }

    fn split_points_into_children(
        points: &VectorBuffer,
        node_hierarchy: GridHierarchy,
        node_id: LeveledGridCell,
    ) -> [(LeveledGridCell, VectorBuffer); 8] {
        struct Wct<'a> {
            points: &'a VectorBuffer,
            node_hierarchy: GridHierarchy,
            node_id: LeveledGridCell,
        }

        impl WithComponentTypeOnce for Wct<'_> {
            type Output = [(LeveledGridCell, VectorBuffer); 8];

            fn run_once<C: crate::geometry::position::Component>(self) -> Self::Output {
                let Self {
                    points,
                    node_hierarchy,
                    node_id,
                } = self;

                // create child buffers
                let make_child =
                    || VectorBuffer::with_capacity(points.len(), points.point_layout().clone());
                let mut children = [
                    make_child(),
                    make_child(),
                    make_child(),
                    make_child(),
                    make_child(),
                    make_child(),
                    make_child(),
                    make_child(),
                ];

                // get center
                let center = node_hierarchy
                    .get_leveled_cell_bounds::<C>(node_id)
                    .center()
                    .expect("Grid AABBs can't be empty.");

                // split points
                let position_attribute_range = points
                    .point_layout()
                    .get_attribute(&C::position_attribute())
                    .expect("missing position attribute")
                    .byte_range_within_point();
                for rd in 0..points.len() {
                    let point_bytes = points.get_point_ref(rd);

                    // read position
                    let position_bytes = &point_bytes[position_attribute_range.clone()];
                    let mut position = C::PasturePrimitive::default();
                    bytemuck::cast_slice_mut::<C::PasturePrimitive, u8>(std::slice::from_mut(
                        &mut position,
                    ))
                    .copy_from_slice(position_bytes);
                    let position: Point3<C> = C::pasture_to_position(position);

                    // find correct child
                    let child_x = if position.x < center.x { 0 } else { 1 };
                    let child_y = if position.y < center.y { 0 } else { 2 };
                    let child_z = if position.z < center.z { 0 } else { 4 };
                    let child_index = child_x | child_y | child_z;
                    let child = &mut children[child_index];

                    // add point to child
                    // unsafe: layouts between both point buffers are identical.
                    unsafe { child.push_points(point_bytes) };
                }

                // Grid cells for the 8 children
                let child_node_ids = node_id.children();
                let [c1, c2, c3, c4, c5, c6, c7, c8] = children;
                let [i1, i2, i3, i4, i5, i6, i7, i8] = child_node_ids;
                [
                    (i1, c1),
                    (i2, c2),
                    (i3, c3),
                    (i4, c4),
                    (i5, c5),
                    (i6, c6),
                    (i7, c7),
                    (i8, c8),
                ]
            }
        }

        Wct {
            points,
            node_hierarchy,
            node_id,
        }
        .for_layout_once(points.point_layout())
    }
}

impl OctreeWriter {
    pub(super) fn new(inner: Arc<Inner>) -> Self {
        let condvar = Arc::new(Condvar::new());
        let inboxes = Arc::new(Mutex::new(Inboxes::new(
            Arc::clone(&condvar),
            inner.priority_function,
            Arc::clone(&inner.metrics),
        )));
        let threads = (0..inner.num_threads)
            .map(|thread_id| {
                let thread = OctreeWorkerThread {
                    inner: Arc::clone(&inner),
                    inboxes: Arc::clone(&inboxes),
                    condvar: Arc::clone(&condvar),
                };
                thread::spawn(move || {
                    if let Some(tracy) = tracy_client::Client::running() {
                        let thread_name = format!("worker thread #{}", thread_id);
                        tracy.set_thread_name(&thread_name);
                    }
                    thread.thread()
                })
            })
            .collect::<Vec<_>>();

        OctreeWriter {
            inboxes,
            threads,
            node_hierarchy: inner.node_hierarchy,
        }
    }

    pub fn insert(&mut self, points: &VectorBuffer) {
        let nr_points = points.len() as f64;

        struct Wct<'a> {
            points: &'a VectorBuffer,
            node_hierarchy: GridHierarchy,
        }

        impl WithComponentTypeOnce for Wct<'_> {
            type Output = HashMap<LeveledGridCell, VectorBuffer>;

            fn run_once<C: Component>(self) -> Self::Output {
                let Self {
                    points,
                    node_hierarchy,
                } = self;

                let mut points_by_cell: HashMap<LeveledGridCell, VectorBuffer> = HashMap::new();

                let grid = node_hierarchy.level::<C>(LodLevel::base());

                let positions =
                    points.view_attribute::<C::PasturePrimitive>(&C::position_attribute());

                for rd in 0..points.len() {
                    let position = C::pasture_to_position(positions.at(rd));
                    let cell = grid.cell_at(position);
                    let node = LeveledGridCell {
                        lod: LodLevel::base(),
                        pos: cell,
                    };
                    let nr_cells = points_by_cell.len();
                    let cell_points = points_by_cell.entry(node).or_insert_with(|| {
                        let capacity = (points.len() / (nr_cells + 1) * 5).min(points.len());
                        VectorBuffer::with_capacity(capacity, points.point_layout().clone())
                    });
                    // safety: both point buffers have the same point layout.
                    unsafe { cell_points.push_points(points.get_point_ref(rd)) };
                }

                points_by_cell
            }
        }

        let layout = points.point_layout().clone();
        let points_by_cell = Wct {
            points,
            node_hierarchy: self.node_hierarchy,
        }
        .for_layout_once(&layout);

        let mut lock = self.inboxes.lock().unwrap();
        let generation = lock.current_gen;
        for (cell, points) in points_by_cell {
            lock.add(cell, points, generation, generation);
        }
        lock.metrics.metric(MetricName::NrPointsAdded, nr_points);
        lock.maybe_next_generation();
        lock.plot_tasks_len();
        lock.plot_nr_of_points();
    }

    pub fn nr_points_waiting(&self) -> usize {
        let lock = self.inboxes.lock().unwrap();
        lock.nr_of_points()
    }

    pub fn nr_nodes_waiting(&self) -> usize {
        self.inboxes.lock().unwrap().tasks.len()
    }
}

impl Drop for OctreeWriter {
    fn drop(&mut self) {
        // tell worker threads to stop
        {
            let mut lock = self.inboxes.lock().unwrap();
            lock.drain();
        }

        // wait for workers to finish
        for thread in self.threads.drain(..) {
            thread.join().unwrap().unwrap()
        }
    }
}

impl InsertionTask {
    #[inline]
    pub fn nr_points(&self) -> usize {
        self.points.iter().map(|buf| buf.len()).sum::<usize>()
    }
}
