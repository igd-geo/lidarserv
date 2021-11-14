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
use crate::las::{Las, LasReadWrite, ReadLasError, WriteLasError};
use crate::span;
use crate::utils::thread_pool::Threads;
use crossbeam_utils::CachePadded;
use nalgebra::Scalar;
use std::cell::UnsafeCell;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::io::Cursor;
use std::iter::ExactSizeIterator;
use std::mem;
use std::sync::Barrier;
use std::time::Instant;

pub struct PartitionedNode<Sampl, Point, Comp: Scalar> {
    hasher: RandomState,
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
    hasher: RandomState,
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
    ) -> Self
    where
        SamplF: SamplingFactory<Sampling = Sampl, Param = LodLevel>,
    {
        assert!(num_partitions.is_power_of_two());
        assert!(num_partitions > 0);

        let hasher = RandomState::new();
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

    pub fn drain_bogus_points(&mut self) -> PartitionedPoints<Point> {
        let partitions = self
            .partitions
            .iter_mut()
            .map(|partition| mem::take(&mut partition.get_mut().bogus))
            .collect();
        PartitionedPoints::from_partitions(partitions)
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
    pub fn clone_points(&self) -> Vec<Point>
    where
        Point: Clone,
    {
        self.partitions
            .iter()
            .flat_map(|p| {
                let mut points = p.get().bogus.clone();
                points.append(&mut p.get().sampling.clone_points());
                points.into_iter()
            })
            .collect()
    }

    pub fn parallel_load<LasL, CSys, SamplF>(
        num_partitions: usize,
        node_id: MetaTreeNodeId,
        sampling_factory: &SamplF,
        page_manager: &PageManager,
        las_loader: &LasL,
        coordinate_system: &CSys,
        threads: &mut Threads,
    ) -> Result<Self, IndexError>
    where
        SamplF: SamplingFactory<Sampling = Sampl, Param = LodLevel>,
        LasL: LasReadWrite<Point, CSys> + Sync,
        CSys: PartialEq + Sync,
    {
        let s1 = span!("parallel_load: prepare");
        let mut this = Self::new(num_partitions, node_id, sampling_factory, false);
        assert_eq!(threads.num_threads(), this.num_partitions());

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
                let bogus_start = las.points.len() - las.bogus_points.unwrap_or(0) as usize;
                let bogus = las.points.split_off(bogus_start);
                let points = las.points;
                drop(s1);

                // add points
                // unsafe:
                //      every thread dereferences a different partition
                //      (based on each threads thread id).
                //      So there is exactly one mutable reference for each partition.
                let s1 = span!("parallel_load: add points");
                let partition = unsafe { partitions[tid].unsafe_get_mut() };
                let rejected = partition.sampling.insert(points, |_, _| ());
                assert!(rejected.is_empty());
                partition.bogus = bogus;
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
        let partitions = &self.partitions;
        let bounds = self.bounds.clone();
        let node_id = &self.node_id;

        let thread_results = threads
            .execute(|thread_id| -> Result<(), IndexError> {
                let partition = partitions[thread_id].get();

                // prepare what to write to the las file
                let s1 = span!("parallel_store: assemble");
                let bogus_points_iter = partition.bogus.iter();
                let sampled_points_iter = partition.sampling.iter();
                let points_iter = IterChain::new(sampled_points_iter, bogus_points_iter);
                let bogus_points = Some(partition.bogus.len() as u32);
                let bounds = bounds.clone();
                let coordinate_system = coordinate_system.clone();
                let las = Las {
                    points: points_iter,
                    bogus_points,
                    bounds,
                    coordinate_system,
                };
                drop(s1);

                // if there are no points, then we can delete the file
                let exists = las.points.len() > 0;

                // encode las
                let s1 = span!("parallel_store: encode las");
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
                page_manager.store(&file_id, BinDataPage { exists, data });
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

    pub fn parallel_insert<SamplF, Patch>(
        &mut self,
        points_partitions: PartitionedPoints<Point>,
        sampling_factory: &SamplF,
        patch_rejected: Patch,
        threads: &mut Threads,
    ) where
        SamplF: SamplingFactory<Point = Point, Sampling = Sampl, Param = LodLevel> + Sync,
        Patch: Fn(&Point, &mut Point) + Sync,
    {
        let s0 = span!("parallel_insert");
        assert_eq!(threads.num_threads(), self.num_partitions());
        assert_eq!(points_partitions.num_partitions(), self.num_partitions());
        let num_partitions = self.num_partitions;
        let partitions = &self.partitions;
        let hasher = &self.hasher;

        // communication between threads
        let mut messages = Vec::new();
        for _ in 0..num_partitions * num_partitions {
            messages.push(UnsafeSyncCell::new(CachePadded::new(Vec::new())));
        }
        let barrier = Barrier::new(num_partitions);

        let aabbs = threads
            .execute(|thread_id| {
                // unsafe: every thread works on a different point partition (based on each threads
                // thread id), so there are no two references to the same partition.
                let points =
                    unsafe { mem::take(points_partitions.partitions[thread_id].unsafe_get_mut()) };

                // calculate aabb
                let s1 = span!("parallel_insert: calculate aabb");
                let mut aabb = OptionAABB::empty();
                for point in &points {
                    aabb.extend(point.position());
                }
                drop(s1);

                // sample points locally first
                let s1 = span!("parallel_insert: sample locally");
                let mut threadlocal_sample = sampling_factory.build(self.node_id.lod());
                let mut rejected = threadlocal_sample.insert(points, &patch_rejected);
                drop(s1);

                // partition local node
                let s1 = span!("parallel_insert: partition");
                let mut partitioned = Vec::new();
                for _ in 0..num_partitions {
                    partitioned.push(Vec::with_capacity(threadlocal_sample.len()));
                }
                for raw_entry in threadlocal_sample.into_raw() {
                    let mut hash = hasher.build_hasher();
                    raw_entry.cell().hash(&mut hash);
                    let partition_id = (hash.finish() & self.bit_mask) as usize;
                    partitioned[partition_id].push(raw_entry);
                }
                drop(s1);

                // share results with other threads
                // so that thread 1 receives partition 1 from every thread,
                // thread 2 receives partition 2 from every thread,
                // and similarly for all other threads.
                let s1 = span!("parallel_insert: exchange messages");
                for (receiver_thread_id, points) in partitioned.into_iter().enumerate() {
                    let index = receiver_thread_id * num_partitions + thread_id;

                    // unsafe: every thread accesses different indices
                    //  (index i is accessed by thread t, if i % num_threads == t )
                    // so no mut aliasing occurs.
                    unsafe {
                        **messages[index].unsafe_get_mut() = points;
                    }
                }
                let s2 = span!("parallel_insert: barrier.wait()");
                barrier.wait();
                drop(s2);
                let mut raw_points = Vec::new();
                for sender_thread_id in 0..num_partitions {
                    let index = thread_id * num_partitions + sender_thread_id;
                    // unsafe: every thread accesses different indices
                    //  (thread t accesses the slice of indices t * num_threads <= i < t * num_threads + num_threads  )
                    // so no mut aliasing occurs.
                    // Also no aliasing with the previous access to `messages`, because it is
                    // protected by a barrier in between.
                    unsafe {
                        raw_points.push(mem::take(&mut **messages[index].unsafe_get_mut()));
                    }
                }
                drop(s1);

                // merge results from different threads
                // unsafe:
                //      every thread dereferences a different partition
                //      (based on each threads thread id).
                //      So there is exactly one mutable reference for each partition.
                let s1 = span!("parallel_insert: merge");
                let partition = unsafe { partitions[thread_id].unsafe_get_mut() };
                partition.bogus.append(&mut rejected);
                for block in raw_points {
                    rejected = partition.sampling.insert_raw(block, &patch_rejected);
                    partition.bogus.append(&mut rejected);
                }
                drop(s1);
                aabb
            })
            .join();

        // extend aabb
        for aabb in aabbs {
            self.bounds.extend_other(&aabb);
        }
        drop(s0);
    }

    pub fn parallel_insert_bogus(
        &mut self,
        points_partitions: PartitionedPoints<Point>,
        threads: &mut Threads,
    ) {
        let s0 = span!("parallel_insert_bogus");
        assert_eq!(threads.num_threads(), self.num_partitions());
        let partitions = &self.partitions;
        let thread_bounds = threads
            .execute(|thread_id| {
                // unsafe:
                //      every thread dereferences a different partition
                //      (based on each threads thread id).
                //      So there is exactly one mutable reference for each partition.
                let partition = unsafe { partitions[thread_id].unsafe_get_mut() };
                let points = unsafe { points_partitions.partitions[thread_id].unsafe_get_mut() };

                // calculate aabb
                let s1 = span!("parallel_insert_bogus: calculate bounds");
                let mut bounds = OptionAABB::empty();
                for point in &*points {
                    bounds.extend(point.position());
                }
                drop(s1);

                // insert
                let s1 = span!("parallel_insert_bogus: append");
                partition.bogus.append(points);
                drop(s1);

                bounds
            })
            .join();

        let s1 = span!("parallel_insert_bogus: merge bounds");
        for aabb in thread_bounds {
            self.bounds.extend_other(&aabb);
        }
        drop(s1);
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
        let mut splitter = PartitionedNodeSplitter {
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
    {
        let s0 = span!("parallel_store (split)");
        assert_eq!(threads.num_threads(), self.num_partitions);
        let partitions = &self.partitions;
        let node_id = &self.node_id;

        let thread_results = threads
            .execute(|thread_id| -> Result<_, IndexError> {
                let partition = partitions[thread_id].get();

                // calculate bounding box for this partition
                let s1 = span!("parallel_store (split): calculate bounds");
                let mut bounds = partition.calculate_bounds();
                drop(s1);

                // prepare what to write to the las file
                let s1 = span!("parallel_store (split): assemble");
                let sampled_points_iter = partition.sampled.iter().map(|raw| raw.point());
                let bogus_points_iter = partition.bogus.iter();
                let points_iter = IterChain::new(sampled_points_iter, bogus_points_iter);
                let bogus_points = Some(partition.bogus.len() as u32);
                let coordinate_system = coordinate_system.clone();
                let las = Las {
                    points: points_iter,
                    bogus_points,
                    bounds: bounds.clone(),
                    coordinate_system,
                };
                drop(s1);

                // if there are no points, then we can delete the file
                let exists = las.points.len() > 0;

                // encode las
                let s1 = span!("parallel_store (split): encode las");
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
                let s1 = span!("parallel_store (split): store page");
                let file_id = node_id.file(thread_id);
                page_manager.store(&file_id, BinDataPage { exists, data });
                drop(s1);

                Ok(bounds)
            })
            .join();

        // check results, merge aabbs
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
        SamplF: SamplingFactory<Sampling = Sampl, Param = LodLevel>,
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
                let mut bounds = from_partition.calculate_bounds();
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
