use crate::geometry::grid::{I32GridHierarchy, LeveledGridCell, LodLevel};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{I32Position, Position};
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index::octree::page_manager::Page;
use crate::index::octree::Inner;
use crate::index::Writer;
use crate::las::{LasExtraBytes, LasPointAttributes, ReadLasError};
use crate::lru_cache::pager::{CacheCleanupError, CacheLoadError};
use log::info;
use serde::{Deserialize, Serialize};
use std::cmp::{max, Ordering};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use std::{mem, thread};
use thiserror::Error;
use tracy_client::{create_plot, Plot};

static TASKS_DEFAULT_PLOT: Plot = create_plot!("Task queue length");
static POINTS_DEFAULT_PLOT: Plot = create_plot!("Task queue length in points");
static POINT_RATE_DEFAULT_PLOT: Plot = create_plot!("Incoming points per generation");

struct InsertionTask<Point> {
    points: Vec<Point>,
    max_generation: u32, // generation of the newest point
    min_generation: u32, // generation of the oldest point in the task
    created_generation: u32,
}

struct LockedTask {
    nr_points: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TaskPriorityFunction {
    NrPoints,
    Lod,
    OldestPoint,
    NewestPoint,
    TaskAge,
    NrPointsWeightedByTaskAge,
    NrPointsWeightedByOldestPoint,
    NrPointsWeightedByNegNewestPoint,
}

struct Inboxes<Point> {
    tasks: HashMap<LeveledGridCell, InsertionTask<Point>>,
    locked: HashMap<LeveledGridCell, LockedTask>,
    drain: bool,
    waiter: Arc<Condvar>,
    current_gen: u32,
    current_gen_started: Instant,
    current_gen_points: u32,
    tasks_plot: &'static Plot,
    points_plot: &'static Plot,
    incoming_points_plot: &'static Plot,
    priority_function: TaskPriorityFunction,
}

pub struct OctreeWriter<Point> {
    inboxes: Arc<Mutex<Inboxes<Point>>>,
    threads: Vec<thread::JoinHandle<Result<(), IndexingThreadError>>>,
    node_hierarchy: I32GridHierarchy,
}

#[derive(Error, Debug)]
enum IndexingThreadError {
    #[error("Error in indexing task: {0}")]
    WriterTaskError(String),

    #[error("Error while writing cache to disk: {0}")]
    CacheCleanup(String),
}

#[derive(Error, Debug)]
enum WriterTaskError {
    #[error(transparent)]
    CacheLoadError(#[from] CacheLoadError),

    #[error(transparent)]
    ReadLasError(#[from] ReadLasError),
}

struct OctreeWorkerThread<Point, Sampl, SamplF> {
    inner: Arc<Inner<Point, Sampl, SamplF>>,
    inboxes: Arc<Mutex<Inboxes<Point>>>,

    /// cond var for waking up worker thread,
    /// shared (and triggered by) inboxes.
    condvar: Arc<Condvar>,
}

impl<K: Debug, V> From<CacheCleanupError<K, V>> for IndexingThreadError {
    fn from(e: CacheCleanupError<K, V>) -> Self {
        IndexingThreadError::CacheCleanup(format!("{}", e.source))
    }
}

impl From<WriterTaskError> for IndexingThreadError {
    fn from(e: WriterTaskError) -> Self {
        IndexingThreadError::WriterTaskError(format!("{}", e))
    }
}

impl TaskPriorityFunction {
    fn cmp<P>(
        &self,
        cell_1: &LeveledGridCell,
        task_1: &InsertionTask<P>,
        cell_2: &LeveledGridCell,
        task_2: &InsertionTask<P>,
    ) -> Ordering {
        match self {
            TaskPriorityFunction::NrPoints => task_1.points.len().cmp(&task_2.points.len()),
            TaskPriorityFunction::Lod => cell_1.lod.cmp(&cell_2.lod),
            TaskPriorityFunction::OldestPoint => task_2.min_generation.cmp(&task_1.min_generation),
            TaskPriorityFunction::NewestPoint => task_2.max_generation.cmp(&task_1.max_generation),
            TaskPriorityFunction::TaskAge => {
                task_2.created_generation.cmp(&task_1.created_generation)
            }
            TaskPriorityFunction::NrPointsWeightedByTaskAge => {
                let base = max(task_1.created_generation, task_2.created_generation);
                let l = task_1.points.len() as f64
                    * 2.0_f64.powi((base - task_1.created_generation) as i32);
                let r = task_2.points.len() as f64
                    * 2.0_f64.powi((base - task_2.created_generation) as i32);
                l.partial_cmp(&r).unwrap_or_else(|| unreachable!())
            }
            TaskPriorityFunction::NrPointsWeightedByOldestPoint => {
                let base = max(task_1.min_generation, task_2.min_generation);
                let l = task_1.points.len() as f64
                    * 2.0_f64.powi((base - task_1.min_generation) as i32);
                let r = task_2.points.len() as f64
                    * 2.0_f64.powi((base - task_2.min_generation) as i32);
                l.partial_cmp(&r).unwrap_or_else(|| unreachable!())
            }
            TaskPriorityFunction::NrPointsWeightedByNegNewestPoint => {
                let base = max(task_1.max_generation, task_2.max_generation);
                let l = task_1.points.len() as f64
                    * 2.0_f64.powi((base - task_1.max_generation) as i32);
                let r = task_2.points.len() as f64
                    * 2.0_f64.powi((base - task_2.max_generation) as i32);
                l.partial_cmp(&r).unwrap_or_else(|| unreachable!())
            }
        }
    }
}

impl<P> Inboxes<P> {
    fn new(waiter: Arc<Condvar>, priority_function: TaskPriorityFunction) -> Self {
        Inboxes {
            tasks: Default::default(),
            locked: Default::default(),
            drain: false,
            waiter,
            current_gen: 0,
            current_gen_started: Instant::now(),
            current_gen_points: 0,
            tasks_plot: &TASKS_DEFAULT_PLOT,
            points_plot: &POINTS_DEFAULT_PLOT,
            incoming_points_plot: &POINT_RATE_DEFAULT_PLOT,
            priority_function,
        }
    }

    fn add(
        &mut self,
        node: LeveledGridCell,
        mut add_points: Vec<P>,
        min_generation: u32,
        max_generation: u32,
    ) {
        if node.lod == LodLevel::base() {
            self.current_gen_points += add_points.len() as u32
        }
        match self.tasks.entry(node) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().points.append(&mut add_points);
                entry.get_mut().max_generation = max_generation;
            }
            Entry::Vacant(entry) => {
                entry.insert(InsertionTask {
                    points: add_points,
                    min_generation,
                    max_generation,
                    created_generation: self.current_gen,
                });
                self.waiter.notify_one();
            }
        }
    }

    pub fn take_and_lock(&mut self) -> Option<(LeveledGridCell, InsertionTask<P>)> {
        let node_id = self
            .tasks
            .iter()
            .filter(|&(cell, _)| !self.locked.contains_key(cell))
            .max_by(|&(cell_a, a), &(cell_b, b)| self.priority_function.cmp(cell_a, a, cell_b, b))
            .map(|(&id, _)| id);

        let node_id = match node_id {
            None => return None,
            Some(i) => i,
        };
        let task = self.tasks.remove(&node_id).unwrap();
        self.locked.insert(
            node_id,
            LockedTask {
                nr_points: task.points.len(),
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
        self.drain = true;
        self.waiter.notify_all();
    }

    pub fn should_exit(&self) -> bool {
        self.drain && self.locked.is_empty() && self.tasks.is_empty()
    }

    pub fn nr_of_points(&self) -> usize {
        let locked_tasks_points: usize = self.locked.values().map(|l| l.nr_points).sum();
        let pending_tasks_points: usize = self.tasks.values().map(|t| t.points.len()).sum();
        locked_tasks_points + pending_tasks_points
    }

    pub fn maybe_next_generation(&mut self) {
        let now = Instant::now();
        let generation_duration: Duration = Duration::from_secs_f64(0.1);
        while now.duration_since(self.current_gen_started) > generation_duration {
            self.incoming_points_plot
                .point(self.current_gen_points as f64);
            self.current_gen += 1;
            self.current_gen_points = 0;
            self.current_gen_started += generation_duration;
            tracy_client::finish_continuous_frame!("mno generation");
        }
    }

    #[inline]
    fn plot_tasks_len(&self) {
        self.tasks_plot.point(self.tasks.len() as f64);
    }

    #[inline]
    fn plot_nr_of_points(&self) {
        self.points_plot.point(self.nr_of_points() as f64);
    }
}

impl<Point, Sampl, SamplF> OctreeWorkerThread<Point, Sampl, SamplF>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes + Clone,
    Sampl: Sampling<Point = Point> + Clone,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
{
    pub fn thread(&self) -> Result<(), IndexingThreadError> {
        loop {
            // get a task
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

            // process task
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
                    for mut task in tasks {
                        let child_id = task.0;
                        let child_points = mem::take(&mut task.1);
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

            // notify subscriptions
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

            // Free space in page cache
            self.inner.page_cache.cleanup()?;
        }
    }

    #[allow(clippy::type_complexity)] // only internal anyways
    fn writer_task(
        &self,
        node_id: LeveledGridCell,
        task: InsertionTask<Point>,
        is_max_lod: bool,
    ) -> Result<(Option<[(LeveledGridCell, Vec<Point>); 8]>, bool), WriterTaskError> {
        // get points
        let node_arc = self.inner.page_cache.load_or_default(&node_id)?.get_node(
            &self.inner.loader,
            || self.inner.sample_factory.build(&node_id.lod),
            &self.inner.coordinate_system,
        )?;
        let mut node = (*node_arc).clone();

        // insert new points
        node.sampling.reset_dirty();
        let initial_nr_bogus = node.bogus_points.len();
        let mut next_lod_points = node.sampling.insert(task.points, |_, _| ());
        node.bogus_points.append(&mut next_lod_points);

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
            node.bogus_points.len() > max_bogus
        };
        let children_points = if make_children {
            mem::take(&mut node.bogus_points)
        } else {
            Vec::new()
        };

        let final_nr_bogus = node.bogus_points.len();
        let should_notify_clients = node.sampling.is_dirty() || final_nr_bogus < initial_nr_bogus;

        // write back to cache
        let page = Page::from_node(node);
        self.inner.page_cache.store(&node_id, page);

        // split into 8 child nodes
        if !make_children {
            return Ok((None, should_notify_clients));
        }
        let mut children: [Vec<Point>; 8] = [
            Vec::with_capacity(children_points.len()),
            Vec::with_capacity(children_points.len()),
            Vec::with_capacity(children_points.len()),
            Vec::with_capacity(children_points.len()),
            Vec::with_capacity(children_points.len()),
            Vec::with_capacity(children_points.len()),
            Vec::with_capacity(children_points.len()),
            Vec::with_capacity(children_points.len()),
        ];
        let center: I32Position = self
            .inner
            .node_hierarchy
            .get_leveled_cell_bounds(&node_id)
            .center();
        for point in children_points {
            let pos = point.position();
            let child_x = if pos.x() < center.x() { 0 } else { 1 };
            let child_y = if pos.y() < center.y() { 0 } else { 2 };
            let child_z = if pos.z() < center.z() { 0 } else { 4 };
            let child_index = child_x | child_y | child_z;
            let child = &mut children[child_index];
            child.push(point);
        }

        // Grid cells for the 8 children
        let child_node_ids = node_id.children();
        let result = [
            (child_node_ids[0], mem::take(&mut children[0])),
            (child_node_ids[1], mem::take(&mut children[1])),
            (child_node_ids[2], mem::take(&mut children[2])),
            (child_node_ids[3], mem::take(&mut children[3])),
            (child_node_ids[4], mem::take(&mut children[4])),
            (child_node_ids[5], mem::take(&mut children[5])),
            (child_node_ids[6], mem::take(&mut children[6])),
            (child_node_ids[7], mem::take(&mut children[7])),
        ];
        Ok((Some(result), should_notify_clients))
    }
}

impl<Point> OctreeWriter<Point>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes,
{
    pub(super) fn new<Sampl, SamplF>(inner: Arc<Inner<Point, Sampl, SamplF>>) -> Self
    where
        Point: Clone + Send + Sync + 'static,
        Sampl: Sampling<Point = Point> + Clone + Send + Sync + 'static,
        SamplF: SamplingFactory<Point = Point, Sampling = Sampl> + Send + Sync + 'static,
    {
        let condvar = Arc::new(Condvar::new());
        let inboxes = Arc::new(Mutex::new(Inboxes::new(
            Arc::clone(&condvar),
            inner.priority_function.clone(),
        )));
        let threads = (0..inner.num_threads)
            .into_iter()
            .map(|thread_id| {
                let thread = OctreeWorkerThread {
                    inner: Arc::clone(&inner),
                    inboxes: Arc::clone(&inboxes),
                    condvar: Arc::clone(&condvar),
                };
                thread::spawn(move || {
                    tracy_client::set_thread_name(&format!("worker thread #{}", thread_id));
                    thread.thread()
                })
            })
            .collect::<Vec<_>>();

        OctreeWriter {
            inboxes,
            threads,
            node_hierarchy: inner.node_hierarchy.clone(),
        }
    }

    pub fn insert_many(&mut self, points: Vec<Point>) {
        let mut points_by_cell: HashMap<LeveledGridCell, Vec<Point>> = HashMap::new();
        for point in points.into_iter() {
            let cell = self
                .node_hierarchy
                .level(&LodLevel::base())
                .leveled_cell_at(point.position());
            match points_by_cell.entry(cell) {
                Entry::Occupied(mut o) => {
                    o.get_mut().push(point);
                }
                Entry::Vacant(v) => {
                    v.insert(vec![point]);
                }
            }
        }

        let mut lock = self.inboxes.lock().unwrap();
        let generation = lock.current_gen;
        for (cell, points) in points_by_cell {
            lock.add(cell, points, generation, generation);
        }
        lock.maybe_next_generation();
        lock.plot_tasks_len();
        lock.plot_nr_of_points();
    }

    pub fn nr_points_waiting(&self) -> usize {
        let lock = self.inboxes.lock().unwrap();
        lock.nr_of_points()
    }
}

impl<Point> Writer<Point> for OctreeWriter<Point>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes + Clone,
{
    fn backlog_size(&self) -> usize {
        self.nr_points_waiting()
    }

    fn insert(&mut self, points: Vec<Point>) {
        self.insert_many(points)
    }
}

impl<Point> Drop for OctreeWriter<Point> {
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
