use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
use crate::geometry::grid::{GridHierarchy, LodLevel};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{Component, Position};
use crate::geometry::sampling::{
    IntoExactSizeIterator, RawSamplingEntry, Sampling, SamplingFactory,
};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::index::sensor_pos::page_manager::{BinDataPage, PageManager};
use crate::index::sensor_pos::point::SensorPositionAttribute;
use crate::index::sensor_pos::writer::IndexError;
use crate::las::{Las, LasReadWrite, ReadLasError, WorkStealingLas, WriteLasError};
use crate::span;
use crate::utils::thread_pool::Threads;
use crossbeam_deque::Steal;
use crossbeam_utils::Backoff;
use nalgebra::Scalar;
use rand::RngCore;
use std::cell::UnsafeCell;
use std::cmp::min;
#[allow(deprecated)]
use std::hash::{Hash, Hasher, SipHasher};
use std::io::Cursor;
use std::iter::ExactSizeIterator;
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Barrier;
use std::time::Instant;

pub struct PartitionedNode<Sampl, Point, Comp: Scalar> {
    hasher: RustCellHasher,
    bit_mask: u64,
    partitions: Vec<UnsafeSyncCell<Partition<Sampl, Point>>>,
    num_partitions: usize,
    bounds: OptionAABB<Comp>,
    node_id: MetaTreeNodeId,
    dirty_since: Option<Instant>,
}

pub struct PartitionedNodeSplitter<Point, Pos, Raw> {
    node_id: MetaTreeNodeId,
    replaces_base_node_at: Option<Pos>,
    hasher: RustCellHasher,
    bit_mask: u64,
    partitions: Vec<UnsafeSyncCell<SplitterPartition<Point, Raw>>>,
    num_partitions: usize,
}

pub struct PartitionedPoints<Point> {
    partitions: Vec<UnsafeSyncCell<Vec<Point>>>,
}

struct Partition<Sampl, Point> {
    sampling: Sampl,
    bogus: Vec<Point>,
}

struct SplitterPartition<Point, Raw> {
    sampled: Vec<Raw>,
    bogus: Vec<Point>,
}

pub trait CellHasher: Clone {
    fn compute_hash<V: Hash>(&self, cell: &V) -> u64;
}

/// Equivalent to rusts [std::collections::hash_map::RandomState], but it allows
/// access to the random keys, so we can store the used keys in the index settings,
/// so that the hash function will be consistent after restarting the server.
#[derive(Clone)]
pub struct RustCellHasher {
    key0: u64,
    key1: u64,
}

impl CellHasher for RustCellHasher {
    #[inline]
    fn compute_hash<V: Hash>(&self, cell: &V) -> u64 {
        // deprecation: I do not have much choice here...
        // if it actually gets removed in the future, we could still switch to some external crate
        #[allow(deprecated)]
        let mut h = SipHasher::new_with_keys(self.key0, self.key1);
        cell.hash(&mut h);
        h.finish()
    }
}

impl RustCellHasher {
    pub fn new_random() -> Self {
        RustCellHasher {
            key0: rand::thread_rng().next_u64(),
            key1: rand::thread_rng().next_u64(),
        }
    }

    pub fn state(&self) -> (u64, u64) {
        (self.key0, self.key1)
    }

    pub fn from_state(state: (u64, u64)) -> Self {
        let (key0, key1) = state;
        RustCellHasher { key0, key1 }
    }
}

/// Wrapper around UnsafeCell, that is Sync.
/// So that we can do synchronisation for the contained value manually.
struct UnsafeSyncCell<Inner>(UnsafeCell<Inner>);

unsafe impl<Inner> Sync for UnsafeSyncCell<Inner> {}

impl<Inner> UnsafeSyncCell<Inner> {
    pub fn new(inner: Inner) -> Self {
        UnsafeSyncCell(UnsafeCell::new(inner))
    }

    /// Get a mut reference to the inner value.
    ///
    /// Requires the inner value to be Send, so it is safe to rely on compiler errors
    /// regarding trait bounds when calling this from multiple threads.
    ///
    /// However, **the caller still needs to ensure, that there is no mut reference aliasing**,
    /// which is why this method is still **unsafe**.
    #[allow(clippy::mut_from_ref)] // this is the whole point of using an UnsafeCell!
    pub unsafe fn unsafe_get_mut(&self) -> &mut Inner
    where
        Inner: Send,
    {
        &mut *self.0.get()
    }

    /// Get a reference to the inner value.
    pub fn get(&self) -> &Inner {
        unsafe { &*self.0.get() }
    }

    /// Get a mut reference to the inner value.
    pub fn get_mut(&mut self) -> &mut Inner {
        self.0.get_mut()
    }
}

impl<Sampl, Point, Comp> PartitionedNode<Sampl, Point, Comp>
where
    Sampl: Sampling,
    Comp: Component,
{
    pub fn new<SamplF>(
        num_partitions: usize,
        node_id: MetaTreeNodeId,
        sampling_factory: &SamplF,
        dirty: bool,
        hasher: RustCellHasher,
    ) -> Self
    where
        SamplF: SamplingFactory<Sampling = Sampl>,
    {
        assert!(num_partitions.is_power_of_two());
        assert!(num_partitions > 0);

        let bit_mask = num_partitions as u64 - 1;
        let partitions = (0..num_partitions)
            .map(|_| {
                UnsafeSyncCell::new(Partition {
                    sampling: sampling_factory.build(node_id.lod()),
                    bogus: vec![],
                })
            })
            .collect();
        let dirty_since = if dirty { Some(Instant::now()) } else { None };
        PartitionedNode {
            hasher,
            bit_mask,
            partitions,
            num_partitions,
            bounds: OptionAABB::empty(),
            node_id,
            dirty_since,
        }
    }

    pub fn num_partitions(&self) -> usize {
        self.num_partitions
    }

    pub fn node_id(&self) -> &MetaTreeNodeId {
        &self.node_id
    }

    pub fn bounds(&self) -> &OptionAABB<Comp> {
        &self.bounds
    }

    pub fn nr_bogus_points(&self) -> usize {
        self.partitions
            .iter()
            .map(|partition| partition.get().bogus.len())
            .sum()
    }

    pub fn nr_sampled_points(&self) -> usize {
        self.partitions
            .iter()
            .map(|partition| partition.get().sampling.len())
            .sum()
    }

    pub fn nr_points(&self) -> usize {
        self.nr_sampled_points() + self.nr_bogus_points()
    }

    pub fn mark_dirty(&mut self) {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now())
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_since.is_some()
    }

    pub fn dirty_since(&self) -> &Option<Instant> {
        &self.dirty_since
    }
}

impl<Sampl, Point, Comp, Pos, Raw> PartitionedNode<Sampl, Point, Comp>
where
    Sampl: Sampling<Point = Point, Raw = Raw> + Send,
    for<'a> &'a Sampl: IntoExactSizeIterator<Item = &'a Point>,
    Point: PointType<Position = Pos> + Send,
    Pos: Position<Component = Comp> + Sync,
    Comp: Component + Send + Sync,
    Raw: RawSamplingEntry<Point = Point> + Send,
{
    pub fn parallel_load<LasL, CSys, SamplF>(
        num_partitions: usize,
        node_id: MetaTreeNodeId,
        sampling_factory: &SamplF,
        page_manager: &PageManager,
        las_loader: &LasL,
        coordinate_system: &CSys,
        threads: &mut Threads,
        hasher: RustCellHasher,
    ) -> Result<Self, IndexError>
    where
        SamplF: SamplingFactory<Sampling = Sampl> + Sync,
        LasL: LasReadWrite<Point, CSys> + Sync,
        CSys: PartialEq + Sync,
    {
        let s1 = span!("parallel_load: prepare");
        let mut this = Self::new(num_partitions, node_id, sampling_factory, false, hasher);
        assert_eq!(threads.num_threads(), this.num_partitions());

        let mut messages = Vec::new();
        for _ in 0..num_partitions * num_partitions {
            messages.push(UnsafeSyncCell::new(Vec::new()));
        }
        let barrier = Barrier::new(num_partitions);
        let partitions = &this.partitions;
        let node_id = &this.node_id;
        drop(s1);
        let partition_bounds = threads
            .execute(|tid| -> Result<OptionAABB<Comp>, IndexError> {
                // load page
                let s1 = span!("parallel_load: load page");
                let file_id = node_id.file(tid);
                let file = page_manager.load_or_default(&file_id)?;
                drop(s1);

                // if the file did not exist, we treat it as an empty las file.
                // (in which case we are done - there are no points to insert into the node)
                if !file.exists {
                    return Ok(OptionAABB::empty());
                }

                // parse las
                let s1 = span!("parallel_load: parse las");
                let read = Cursor::new(file.data.as_slice());
                let mut las = las_loader.read_las(read)?;
                if las.coordinate_system != *coordinate_system {
                    return Err(IndexError::ReadLas {
                        source: ReadLasError::FileFormat {
                            desc: "Coordinate system mismatch".to_string(),
                        },
                    });
                }
                drop(s1);

                // split points into "actual" points and bogus points
                let s1 = span!("parallel_load: split of bogus");
                let bogus_start = las
                    .non_bogus_points
                    .map(|b| b as usize)
                    .unwrap_or(las.points.len());
                let bogus = las.points.split_off(bogus_start);
                let points = las.points;
                drop(s1);

                // add points
                // unsafe:
                //      every thread dereferences a different partition
                //      (based on each threads thread id).
                //      So there is exactly one mutable reference for each partition.
                let s1 = span!("parallel_load: sample");
                let partition = unsafe { partitions[tid].unsafe_get_mut() };
                partition.bogus = bogus;
                let mut local_sampl = sampling_factory.build(&node_id.lod());
                let rejected = local_sampl.insert(points, |_, _| ());
                assert!(rejected.is_empty());
                drop(s1);

                let s1 = span!("parallel_load: partition");
                let mut thread_partitions = Vec::new();
                for _ in 0..num_partitions {
                    thread_partitions.push(Vec::new());
                }
                let hasher = &this.hasher;
                let bit_mask = this.bit_mask;
                for raw_entry in local_sampl.into_raw() {
                    let partition_id = (hasher.compute_hash(raw_entry.cell()) & bit_mask) as usize;
                    thread_partitions[partition_id].push(raw_entry);
                }
                for (receiver_thread_id, data) in thread_partitions.into_iter().enumerate() {
                    *unsafe {
                        messages[num_partitions * receiver_thread_id + tid].unsafe_get_mut()
                    } = data;
                }
                drop(s1);
                barrier.wait();
                let s1 = span!("parallel_load: merge");
                for sender_thread_id in 0..num_partitions {
                    let points = mem::take(unsafe {
                        messages[num_partitions * tid + sender_thread_id].unsafe_get_mut()
                    });
                    let rejected = partition.sampling.insert_raw(points, |_, _| ());
                    assert!(rejected.is_empty());
                }
                drop(s1);

                // return aabb,
                // so the main thread can update the node aabb
                Ok(las.bounds)
            })
            .join();

        // update the bounds
        // (we only need to take the bounds from the first thread, because when storing partitioned
        // nodes, we write the same bounds to every file anyways)
        let s1 = span!("parallel_load: finalize");
        let mut iter = partition_bounds.into_iter();
        this.bounds = iter.next().unwrap()?;

        // check the results of the remaining threads
        for result in iter {
            result?;
        }
        drop(s1);
        Ok(this)
    }

    pub fn parallel_store_alt<CSys, LasL>(
        &mut self,
        page_manager: &PageManager,
        las_loader: &LasL,
        coordinate_system: &CSys,
        threads: &mut Threads,
    ) -> Result<(), IndexError>
    where
        CSys: Sync + Clone,
        LasL: LasReadWrite<Point, CSys> + Sync,
        Point: Clone,
    {
        let s0 = span!("parallel_store_alt");
        assert_eq!(threads.num_threads(), self.num_partitions());

        // shortcut for empty nodes...
        if self.nr_points() == 0 {
            for thread_id in 0..self.num_partitions {
                let file_id = self.node_id.file(thread_id);
                page_manager.store(
                    &file_id,
                    BinDataPage {
                        exists: false,
                        data: Vec::new(),
                        error_counter: Default::default(),
                    },
                );
            }
            self.dirty_since = None;
            return Ok(());
        }

        // prepare parallel execution
        let num_partitions = self.num_partitions;
        let partitions = &self.partitions;
        let bounds = self.bounds.clone();
        let node_id = &self.node_id;
        let mut points_worker_queues = Vec::new();
        let mut points_stealers = Vec::new();
        for _ in 0..num_partitions {
            let w = crossbeam_deque::Worker::new_fifo();
            let s = w.stealer();
            points_worker_queues.push(Some(w));
            points_stealers.push(s);
        }
        let mut bogus_worker_queues = Vec::new();
        let mut bogus_stealers = Vec::new();
        for _ in 0..num_partitions {
            let w = crossbeam_deque::Worker::new_fifo();
            let s = w.stealer();
            bogus_worker_queues.push(Some(w));
            bogus_stealers.push(s);
        }
        let mut args = Vec::new();
        for thread_id in 0..num_partitions {
            args.push((
                points_worker_queues[thread_id].take().unwrap(),
                points_stealers.clone(),
                bogus_worker_queues[thread_id].take().unwrap(),
                bogus_stealers.clone(),
            ))
        }
        let barrier = Barrier::new(num_partitions);
        drop(points_stealers);
        drop(bogus_stealers);

        // parallel exec
        let thread_results = threads
            .execute_with_args(
                args,
                |thread_id,
                 (points_queue, points_stealers, bogus_queue, bogus_stealers)|
                 -> Result<(), IndexError> {
                    let partition = partitions[thread_id].get();

                    // prepare what to write to the las file
                    let s1 = span!("parallel_store: prepare");
                    let tasks_size = (partition.sampling.len() + partition.bogus.len()) / 50 + 1;
                    let mut points = partition.sampling.clone_points();
                    while points.len() >= tasks_size {
                        let task = points.split_off(points.len() - tasks_size);
                        points_queue.push(task);
                    }
                    if !points.is_empty() {
                        points_queue.push(points);
                    }
                    let mut points = partition.bogus.clone();
                    while points.len() >= tasks_size {
                        let task = points.split_off(points.len() - tasks_size);
                        bogus_queue.push(task);
                    }
                    if !points.is_empty() {
                        bogus_queue.push(points);
                    }
                    barrier.wait();

                    let bounds = bounds.clone();
                    let coordinate_system = coordinate_system.clone();
                    let las = WorkStealingLas {
                        points_queue,
                        points_stealers,
                        bogus_queue,
                        bogus_stealers,
                        bounds,
                        coordinate_system,
                        bogus_points_vlr: true,
                    };
                    drop(s1);

                    // encode las
                    let s1 = span!("parallel_store: encode las");
                    let mut data = Vec::new();
                    let write = Cursor::new(&mut data);
                    match las_loader.write_las_work_stealing(las, write) {
                        Ok(_) => {}
                        Err(WriteLasError::Io(_)) => {
                            unreachable!("Cursor as write does not throw IO errors")
                        }
                    };
                    drop(s1);

                    // write file
                    let s1 = span!("parallel_store: store page");
                    let file_id = node_id.file(thread_id);
                    page_manager.store(
                        &file_id,
                        BinDataPage {
                            exists: true,
                            data,
                            error_counter: Default::default(),
                        },
                    );
                    drop(s1);

                    Ok(())
                },
            )
            .join();

        // check results
        for result in thread_results {
            result?;
        }

        // update dirtiness
        self.dirty_since = None;

        drop(s0);
        Ok(())
    }

    pub fn parallel_store<CSys, LasL>(
        &mut self,
        page_manager: &PageManager,
        las_loader: &LasL,
        coordinate_system: &CSys,
        threads: &mut Threads,
    ) -> Result<(), IndexError>
    where
        CSys: Sync + Clone,
        LasL: LasReadWrite<Point, CSys> + Sync,
    {
        let s0 = span!("parallel_store");
        assert_eq!(threads.num_threads(), self.num_partitions());

        // shortcut for empty nodes...
        if self.nr_points() == 0 {
            for thread_id in 0..self.num_partitions {
                let file_id = self.node_id.file(thread_id);
                page_manager.store(
                    &file_id,
                    BinDataPage {
                        exists: false,
                        data: Vec::new(),
                        error_counter: Default::default(),
                    },
                );
            }
            self.dirty_since = None;
            return Ok(());
        }

        let partitions = &self.partitions;
        let bounds = self.bounds.clone();
        let node_id = &self.node_id;

        let thread_results = threads
            .execute(|thread_id| -> Result<(), IndexError> {
                let partition = partitions[thread_id].get();

                // prepare what to write to the las file
                let s1 = span!("parallel_store: assemble");
                let non_bogus_points = Some(partition.sampling.len() as u32);
                let bogus_points_iter = partition.bogus.iter();
                let sampled_points_iter = partition.sampling.iter();
                let points_iter = IterChain::new(sampled_points_iter, bogus_points_iter);
                let bounds = bounds.clone();
                let coordinate_system = coordinate_system.clone();
                let las = Las {
                    points: points_iter,
                    non_bogus_points,
                    bounds,
                    coordinate_system,
                };
                drop(s1);

                // if there are no points, then we can delete the file
                let exists = las.points.len() > 0;

                // encode las
                let s1 = span!("parallel_store: encode las");
                s1.emit_value(las.points.len() as u64);
                let mut data = Vec::new();
                if exists {
                    let write = Cursor::new(&mut data);
                    match las_loader.write_las(las, write) {
                        Ok(_) => {}
                        Err(WriteLasError::Io(_)) => {
                            unreachable!("Cursor as write does not throw IO errors")
                        }
                    };
                }
                drop(s1);

                // write file
                let s1 = span!("parallel_store: store page");
                let file_id = node_id.file(thread_id);
                page_manager.store(
                    &file_id,
                    BinDataPage {
                        exists,
                        data,
                        error_counter: Default::default(),
                    },
                );
                drop(s1);

                Ok(())
            })
            .join();

        // check results
        for result in thread_results {
            result?;
        }

        // update dirtiness
        self.dirty_since = None;

        drop(s0);
        Ok(())
    }

    pub fn parallel_insert_multi_lod<SamplF, Patch>(
        selfs: &mut Vec<Self>,
        mut points_partitions: PartitionedPoints<Point>,
        sampling_factory: &SamplF,
        patch_rejected: Patch,
        threads: &mut Threads,
    ) where
        SamplF: SamplingFactory<Point = Point, Sampling = Sampl> + Sync,
        Patch: Fn(&Point, &mut Point) + Sync,
    {
        let s0 = span!("parallel_insert_multi_lod");
        for s in &*selfs {
            assert_eq!(threads.num_threads(), s.num_partitions());
            assert_eq!(points_partitions.num_partitions(), s.num_partitions());
        }
        let num_partitions = selfs.first().unwrap().num_partitions;
        let num_lods = selfs.len();

        // divide points into batches of 2_000 points
        let batch_size = 2_000;
        let mut tasks = Vec::new();
        for thread_id in 0..num_partitions {
            let mut points = mem::take(points_partitions.partitions[thread_id].get_mut());
            while points.len() > batch_size {
                // split of a batch
                let batch_start = points.len() - batch_size;
                let batch = points.split_off(batch_start);
                tasks.push(batch);
            }
            tasks.push(points);
        }

        // shared messages for inter thread communication
        let mut messages = Vec::new();
        for _ in 0..num_partitions * num_partitions * (num_lods - 1) {
            // index = lod_index * num_partitions * num_partitions + receiver_thread_id * num_partitions + sender_thread_id
            messages.push(UnsafeSyncCell::new(Vec::new()));
        }
        let messages_sent = AtomicUsize::new(0);

        // queues for scheduling work between the threads
        let mut local_insert_workers = Vec::new();
        let mut local_insert_stealers = Vec::new();
        for _ in 0..num_partitions {
            let w = crossbeam_deque::Worker::new_fifo();
            let s = w.stealer();
            local_insert_workers.push(Some(w));
            local_insert_stealers.push(s);
        }
        let ready = AtomicUsize::new(0);

        // distribute tasks on worker queues
        for (i, task) in tasks.drain(..).enumerate() {
            let w = local_insert_workers[i % num_partitions].as_mut().unwrap();
            w.push(task);
        }

        // assemble args for each worker thread
        let mut args = Vec::new();
        for thread_id in 0..num_partitions {
            args.push((
                local_insert_stealers.clone(),
                local_insert_workers[thread_id].take().unwrap(),
            ));
        }

        let thread_results = threads
            .execute_with_args(args, |thread_id, (task_stealers, queue)| {
                let mut aabbs = Vec::new();
                let mut local_sample = sampling_factory.build(&LodLevel::from_level(0));
                let mut next_lod_points = Vec::new();
                let mut last_aabb = OptionAABB::empty();
                let last_lod_partition =
                    unsafe { selfs.last().unwrap().partitions[thread_id].unsafe_get_mut() };

                for lod_index in 0..num_lods - 1 {
                    // take tasks from the queue and sample locally

                    'local_insert_points: loop {
                        // get a batch of points
                        let points = match queue.pop() {
                            Some(p) => p,
                            None => {
                                let mut retry = true;
                                let mut stolen = None;
                                'try_steal_task: while retry {
                                    retry = false;
                                    let ready_threads = ready.load(Ordering::Acquire);
                                    for s in &task_stealers {
                                        match s.steal_batch_and_pop(&queue) {
                                            Steal::Empty => {}
                                            Steal::Success(p) => {
                                                stolen = Some(p);
                                                break 'try_steal_task;
                                            }
                                            Steal::Retry => retry = true,
                                        }
                                    }
                                    if !retry && ready_threads < lod_index * num_partitions {
                                        retry = true;
                                        let backoff = Backoff::new();
                                        while ready.load(Ordering::Acquire) == ready_threads {
                                            backoff.snooze();
                                        }
                                    }
                                }
                                if let Some(p) = stolen {
                                    p
                                } else {
                                    break 'local_insert_points;
                                }
                            }
                        };

                        // insert into the node
                        let s1 = span!("parallel_insert_multi_lod2: local sample");
                        let mut rejected = local_sample.insert(points, &patch_rejected);
                        next_lod_points.append(&mut rejected);
                        drop(s1);
                    }

                    // calculate node aabb
                    let s1 = span!("parallel_insert_multi_lod2: aabb");
                    aabbs.push(local_sample.bounding_box());
                    drop(s1);

                    // partition
                    let s1 = span!("parallel_insert_multi_lod2: partition");
                    let mut partitions = Vec::new();
                    for _ in 0..num_partitions {
                        partitions.push(Vec::with_capacity(local_sample.len()));
                    }
                    let hasher = &selfs[lod_index].hasher;
                    let bit_mask = selfs[lod_index].bit_mask;
                    for raw_entry in local_sample.into_raw() {
                        let partition_id =
                            (hasher.compute_hash(raw_entry.cell()) & bit_mask) as usize;
                        partitions[partition_id].push(raw_entry);
                    }
                    drop(s1);

                    // "send" each partition to its thread
                    for (receiver_thread_id, partition) in partitions.into_iter().enumerate() {
                        let message_index = lod_index * num_partitions * num_partitions
                            + receiver_thread_id * num_partitions
                            + thread_id;
                        *unsafe { messages[message_index].unsafe_get_mut() } = partition;
                    }
                    messages_sent.fetch_add(1, Ordering::AcqRel);

                    // start processing the next lod tasks, that we already have while still
                    // waiting for the other threads to send over their points
                    let next_lod_index = lod_index + 1;
                    let mut next_local_sample =
                        sampling_factory.build(&LodLevel::from_level(next_lod_index as u16));
                    let mut next_next_lod_points = Vec::new();
                    let backoff = Backoff::new();
                    while messages_sent.load(Ordering::Acquire) < num_partitions * next_lod_index {
                        if !next_lod_points.is_empty() {
                            if next_lod_index == num_lods - 1 {
                                let s1 = span!("parallel_insert_multi_lod2: max_lod while waiting");
                                for p in &next_lod_points {
                                    last_aabb.extend(p.position());
                                }
                                last_lod_partition.bogus.append(&mut next_lod_points);
                                drop(s1);
                            } else {
                                let num_points = min(next_lod_points.len(), batch_size);
                                let batch_start = next_lod_points.len() - num_points;
                                let batch = next_lod_points.split_off(batch_start);
                                let s1 =
                                    span!("parallel_insert_multi_lod2: local sample while waiting");
                                let mut rejected = next_local_sample.insert(batch, &patch_rejected);
                                next_next_lod_points.append(&mut rejected);
                                drop(s1);
                            }
                        } else {
                            backoff.snooze();
                        }
                    }

                    // "receive" from other threads
                    // (just moves them over to their own vec for our convenience)
                    let mut received = Vec::new();
                    for sender_thread_id in 0..num_partitions {
                        let message_index = lod_index * num_partitions * num_partitions
                            + thread_id * num_partitions
                            + sender_thread_id;
                        let message =
                            mem::take(unsafe { messages[message_index].unsafe_get_mut() });
                        received.push(message);
                    }

                    // merge into node
                    let s1 = span!("parallel_insert_multi_lod2: merge");
                    let node_partition =
                        unsafe { selfs[lod_index].partitions[thread_id].unsafe_get_mut() };
                    for raw in received {
                        let mut rejected = node_partition.sampling.insert_raw(raw, &patch_rejected);
                        next_lod_points.append(&mut rejected);
                    }
                    drop(s1);

                    // create tasks for next lod
                    while next_lod_points.len() > batch_size {
                        let batch_start = next_lod_points.len() - batch_size;
                        let batch = next_lod_points.split_off(batch_start);
                        queue.push(batch);
                    }
                    if next_lod_points.len() > 0 {
                        queue.push(next_lod_points);
                    }
                    ready.fetch_add(1, Ordering::AcqRel);

                    // prepare next iteration
                    local_sample = next_local_sample;
                    next_lod_points = next_next_lod_points;
                }

                // calculate max_lod aabb and store points
                let s1 = span!("parallel_insert_multi_lod2: max_lod");
                while let Some(mut points) = queue.pop() {
                    for p in &points {
                        last_aabb.extend(p.position());
                    }
                    last_lod_partition.bogus.append(&mut points);
                }
                aabbs.push(last_aabb);
                drop(s1);
                aabbs
            })
            .join();

        // apply aabbs and mark dirty
        for aabbs in thread_results {
            for (lod_index, aabb) in aabbs.into_iter().enumerate() {
                selfs[lod_index].bounds.extend_other(&aabb);
                if !aabb.is_empty() {
                    selfs[lod_index].mark_dirty();
                }
            }
        }

        drop(s0);
    }

    pub fn parallel_drain_into_splitter(
        &mut self,
        sensor_position: Pos,
        threads: &mut Threads,
    ) -> PartitionedNodeSplitter<Point, Pos, Raw> {
        let s0 = span!("parallel_drain_into_splitter");
        assert_eq!(threads.num_threads(), self.num_partitions());

        // create empty
        let splitter = PartitionedNodeSplitter {
            node_id: self.node_id.clone(),
            replaces_base_node_at: Some(sensor_position),
            hasher: self.hasher.clone(),
            bit_mask: self.bit_mask,
            partitions: (0..self.num_partitions)
                .map(|_| {
                    UnsafeSyncCell::new(SplitterPartition {
                        sampled: vec![],
                        bogus: vec![],
                    })
                })
                .collect(),
            num_partitions: self.num_partitions,
        };

        // move over points, in parallel for each partition
        let partitions = &mut self.partitions;
        threads
            .execute(|thread_id| {
                // unsafe:
                //      every thread dereferences a different partition
                //      (based on each threads thread id).
                //      So there is exactly one mutable reference for each partition.
                let from_partition = unsafe { partitions[thread_id].unsafe_get_mut() };
                let target_partition = unsafe { splitter.partitions[thread_id].unsafe_get_mut() };

                // transfer points
                target_partition.sampled = from_partition.sampling.drain_raw();
                target_partition.bogus = mem::take(&mut from_partition.bogus);
            })
            .join();

        drop(s0);
        splitter
    }
}

impl<Point, Pos, Raw, Comp> PartitionedNodeSplitter<Point, Pos, Raw>
where
    Raw: RawSamplingEntry<Point = Point> + Send,
    Point: PointType<Position = Pos> + Send,
    Pos: Position<Component = Comp> + Sync,
    Comp: Component + Send,
{
    pub fn node_id(&self) -> &MetaTreeNodeId {
        &self.node_id
    }

    pub fn nr_points(&self) -> usize {
        self.partitions
            .iter()
            .map(|p| p.get().bogus.len() + p.get().sampled.len())
            .sum()
    }

    pub fn replaces_base_node(&self) -> bool {
        self.replaces_base_node_at.is_some()
    }

    pub fn parallel_split<GridH>(
        mut self,
        meta_tree: &MetaTree<GridH, Comp>,
        threads: &mut Threads,
    ) -> [Self; 8]
    where
        GridH: GridHierarchy<Position = Pos, Component = Comp>,
        Point: WithAttr<SensorPositionAttribute<Pos>>,
    {
        let s0 = span!("parallel_split");
        let s1 = span!("parallel_split: prepare");
        assert_eq!(threads.num_threads(), self.num_partitions);
        let partitions = &mut self.partitions;

        // center of the node is where to split
        let node_center = meta_tree.node_center(&self.node_id);

        // prepare children to insert points into
        let mut children = self
            .node_id
            .children()
            .map(|child| PartitionedNodeSplitter {
                node_id: child,
                replaces_base_node_at: None,
                hasher: self.hasher.clone(),
                bit_mask: self.bit_mask,
                partitions: (0..self.num_partitions)
                    .map(|_| {
                        UnsafeSyncCell::new(SplitterPartition {
                            sampled: vec![],
                            bogus: vec![],
                        })
                    })
                    .collect(),
                num_partitions: self.num_partitions,
            });

        // pass down the sensor position
        if let Some(sensor_pos) = self.replaces_base_node_at {
            let replace_child_id = node_select_child(&node_center, &sensor_pos);
            children[replace_child_id].replaces_base_node_at = Some(sensor_pos);
        }

        // split every partition in parallel
        drop(s1);
        threads
            .execute(|thread_id| {
                // partition to split
                // unsafe:
                //      every thread dereferences a different partition
                //      (based on each threads thread id).
                //      So there is exactly one mutable reference for each partition.
                let partition = unsafe { partitions[thread_id].unsafe_get_mut() };
                let s1 = span!("parallel_split: prepare thread");

                // partitions of child nodes to insert split points into
                // unsafe:
                //      same as for `partition`
                let mut target_partitions: Vec<_> = (0..8)
                    .map(|i| unsafe { children[i].partitions[thread_id].unsafe_get_mut() })
                    .collect();

                drop(s1);

                // split sampled points
                let s1 = span!("parallel_split: split sampled points");
                let sampled_points = mem::take(&mut partition.sampled);
                for point in sampled_points {
                    let sensor_pos = point.point().attribute::<SensorPositionAttribute<Pos>>();
                    let child_index = node_select_child(&node_center, &sensor_pos.0);
                    target_partitions[child_index].sampled.push(point);
                }
                drop(s1);

                // split bogus points
                let s1 = span!("parallel_split: split bogus points");
                let bogus_points = mem::take(&mut partition.bogus);
                for point in bogus_points {
                    let sensor_pos = point.attribute::<SensorPositionAttribute<Pos>>();
                    let child_index = node_select_child(&node_center, &sensor_pos.0);
                    target_partitions[child_index].bogus.push(point);
                }
                drop(s1);
            })
            .join();

        drop(s0);
        children
    }

    pub fn parallel_store<CSys, LasL>(
        self,
        coordinate_system: &CSys,
        las_loader: &LasL,
        page_manager: &PageManager,
        threads: &mut Threads,
    ) -> Result<OptionAABB<Comp>, IndexError>
    where
        LasL: LasReadWrite<Point, CSys> + Sync,
        CSys: Clone + Sync,
        Point: Clone,
    {
        let s0 = span!("parallel_store_alt (split)");
        assert_eq!(threads.num_threads(), self.num_partitions);

        // shortcut for empty nodes...
        if self.nr_points() == 0 {
            for thread_id in 0..self.num_partitions {
                let file_id = self.node_id.file(thread_id);
                page_manager.store(
                    &file_id,
                    BinDataPage {
                        exists: false,
                        data: Vec::new(),
                        error_counter: Default::default(),
                    },
                );
            }
            return Ok(OptionAABB::empty());
        }

        // prepare parallel execution
        let num_partitions = self.num_partitions;
        let partitions = &self.partitions;
        let node_id = &self.node_id;
        let mut points_worker_queues = Vec::new();
        let mut points_stealers = Vec::new();
        for _ in 0..num_partitions {
            let w = crossbeam_deque::Worker::new_fifo();
            let s = w.stealer();
            points_worker_queues.push(Some(w));
            points_stealers.push(s);
        }
        let mut bogus_worker_queues = Vec::new();
        let mut bogus_stealers = Vec::new();
        for _ in 0..num_partitions {
            let w = crossbeam_deque::Worker::new_fifo();
            let s = w.stealer();
            bogus_worker_queues.push(Some(w));
            bogus_stealers.push(s);
        }
        let mut args = Vec::new();
        for thread_id in 0..num_partitions {
            args.push((
                points_worker_queues[thread_id].take().unwrap(),
                points_stealers.clone(),
                bogus_worker_queues[thread_id].take().unwrap(),
                bogus_stealers.clone(),
            ))
        }
        let barrier = Barrier::new(num_partitions);
        let mut aabbs = Vec::new();
        for _ in 0..num_partitions {
            aabbs.push(UnsafeSyncCell::new(OptionAABB::empty()));
        }
        drop(points_stealers);
        drop(bogus_stealers);

        // parallel exec
        let thread_results = threads
            .execute_with_args(
                args,
                |thread_id,
                 (points_queue, points_stealers, bogus_queue, bogus_stealers)|
                 -> Result<OptionAABB<Comp>, IndexError> {
                    let partition = partitions[thread_id].get();

                    // calculate bounding box for this partition
                    // todo - the work stealing las writer mixes points from all partitions.
                    //      - we should use the bounding box of all partitions here.
                    let s1 = span!("parallel_store (split): calculate bounds");
                    let bounds = partition.calculate_bounds();
                    *unsafe { aabbs[thread_id].unsafe_get_mut() } = bounds;
                    drop(s1);

                    // prepare what to write to the las file
                    let s1 = span!("parallel_store: prepare");
                    let tasks_size = (partition.sampled.len() + partition.bogus.len()) / 50 + 1;
                    let mut points = partition
                        .sampled
                        .iter()
                        .map(|raw| raw.point().clone())
                        .collect::<Vec<_>>();
                    while points.len() >= tasks_size {
                        let task = points.split_off(points.len() - tasks_size);
                        points_queue.push(task);
                    }
                    if !points.is_empty() {
                        points_queue.push(points);
                    }
                    let mut points = partition.bogus.clone();
                    while points.len() >= tasks_size {
                        let task = points.split_off(points.len() - tasks_size);
                        bogus_queue.push(task);
                    }
                    if !points.is_empty() {
                        bogus_queue.push(points);
                    }
                    barrier.wait();
                    let mut bounds = OptionAABB::empty();
                    for aabb in &aabbs {
                        bounds.extend_other(aabb.get());
                    }

                    let coordinate_system = coordinate_system.clone();
                    let las = WorkStealingLas {
                        points_queue,
                        points_stealers,
                        bogus_queue,
                        bogus_stealers,
                        bounds: bounds.clone(),
                        coordinate_system,
                        bogus_points_vlr: true,
                    };
                    drop(s1);

                    // encode las
                    let s1 = span!("parallel_store: encode las");
                    let mut data = Vec::new();
                    let write = Cursor::new(&mut data);
                    match las_loader.write_las_work_stealing(las, write) {
                        Ok(_) => {}
                        Err(WriteLasError::Io(_)) => {
                            unreachable!("Cursor as write does not throw IO errors")
                        }
                    };
                    drop(s1);

                    // write file
                    let s1 = span!("parallel_store: store page");
                    let file_id = node_id.file(thread_id);
                    page_manager.store(
                        &file_id,
                        BinDataPage {
                            exists: true,
                            data,
                            error_counter: Default::default(),
                        },
                    );
                    drop(s1);

                    Ok(bounds)
                },
            )
            .join();

        // check results
        let mut bounds = OptionAABB::empty();
        for result in thread_results {
            bounds.extend_other(&result?);
        }

        drop(s0);
        Ok(bounds)
    }

    pub fn parallel_into_node<SamplF, Sampl>(
        self,
        sampling_factory: &SamplF,
        threads: &mut Threads,
    ) -> PartitionedNode<Sampl, Point, Comp>
    where
        SamplF: SamplingFactory<Sampling = Sampl>,
        Sampl: Sampling<Point = Point, Raw = Raw> + Send,
    {
        assert_eq!(threads.num_threads(), self.num_partitions);

        // new empty node
        let mut node = PartitionedNode {
            hasher: self.hasher,
            bit_mask: self.bit_mask,
            partitions: (0..self.num_partitions)
                .map(|_| {
                    UnsafeSyncCell::new(Partition {
                        sampling: sampling_factory.build(self.node_id.lod()),
                        bogus: vec![],
                    })
                })
                .collect(),
            num_partitions: self.num_partitions,
            bounds: OptionAABB::empty(),
            node_id: self.node_id,
            dirty_since: Some(Instant::now()),
        };

        // fill with points in parallel
        let partitions = &self.partitions;
        let thread_results = threads
            .execute(|thread_id| {
                // unsafe:
                //      every thread works on a different partition
                //      (based on each threads thread id).
                //      So there is exactly one mutable reference for each partition.
                let from_partition = unsafe { partitions[thread_id].unsafe_get_mut() };
                let to_partition = unsafe { node.partitions[thread_id].unsafe_get_mut() };

                // calculate aabb
                let s1 = span!("parallel_store (split): calculate bounds");
                let bounds = from_partition.calculate_bounds();
                drop(s1);

                // move over points
                to_partition.bogus = mem::take(&mut from_partition.bogus);
                let rejected = to_partition.sampling.insert_raw(
                    mem::take(&mut from_partition.sampled),
                    |_, _| unreachable!(),
                );
                assert!(rejected.is_empty());

                bounds
            })
            .join();

        // merge calculated bounding boxes
        for aabb in thread_results {
            node.bounds.extend_other(&aabb);
        }

        node
    }
}

impl<Point, Raw, Pos, Comp> SplitterPartition<Point, Raw>
where
    Raw: RawSamplingEntry<Point = Point>,
    Point: PointType<Position = Pos>,
    Pos: Position<Component = Comp>,
    Comp: Component,
{
    fn calculate_bounds(&self) -> OptionAABB<Comp> {
        let mut bounds = OptionAABB::empty();
        for point in &self.sampled {
            bounds.extend(point.point().position());
        }
        for point in &self.bogus {
            bounds.extend(point.position());
        }
        bounds
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

impl<Point> PartitionedPoints<Point> {
    pub fn new(num_partitions: usize) -> Self {
        let mut partitions = Vec::new();
        for _ in 0..num_partitions {
            partitions.push(UnsafeSyncCell::new(Vec::new()));
        }
        PartitionedPoints { partitions }
    }

    pub fn num_partitions(&self) -> usize {
        self.partitions.len()
    }

    pub fn from_partitions(partitions: Vec<Vec<Point>>) -> PartitionedPoints<Point> {
        PartitionedPoints {
            partitions: partitions.into_iter().map(UnsafeSyncCell::new).collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.partitions.iter().all(|b| b.get().is_empty())
    }

    pub fn parallel_split(self) {}
}

/// Like iter::chain, just that it also works with ExactSizeIterator
pub struct IterChain<I1, I2> {
    i1: I1,
    i2: I2,
    state: bool,
}

impl<I1, I2> IterChain<I1, I2> {
    pub fn new(i1: I1, i2: I2) -> Self {
        IterChain {
            i1,
            i2,
            state: true,
        }
    }
}

impl<I1, I2, Item> Iterator for IterChain<I1, I2>
where
    I1: Iterator<Item = Item>,
    I2: Iterator<Item = Item>,
{
    type Item = Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.state {
            if let Some(val) = self.i1.next() {
                return Some(val);
            }
            self.state = false;
        }
        self.i2.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min1, max1) = self.i1.size_hint();
        let (min2, max2) = self.i2.size_hint();
        let min = min1 + min2;
        let max = match (max1, max2) {
            (Some(m1), Some(m2)) => Some(m1 + m2),
            _ => None,
        };
        (min, max)
    }
}

impl<I1, I2, Item> ExactSizeIterator for IterChain<I1, I2>
where
    I1: ExactSizeIterator + Iterator<Item = Item>,
    I2: ExactSizeIterator + Iterator<Item = Item>,
{
}
