use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
use crate::geometry::grid::{GridHierarchy, LodLevel};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{Component, CoordinateSystem, Position};
use crate::geometry::sampling::{RawSamplingEntry, Sampling, SamplingFactory};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::index::sensor_pos::page_manager::{BinDataPage, PageManager};
use crate::index::sensor_pos::point::SensorPositionAttribute;
use crate::index::sensor_pos::writer::IndexError;
use crate::las::{Las, LasReadWrite, ReadLasError, WriteLasError};
use crate::lru_cache::pager::CacheLoadError;
use crate::span;
use crate::utils::thread_pool::Threads;
use crossbeam_utils::CachePadded;
use nalgebra::Scalar;
use num_traits::Bounded;
use std::cell::UnsafeCell;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::io::Cursor;
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use thiserror::Error;

pub struct PartitionedNode<Sampl, Point, Comp: Scalar> {
    hasher: RandomState,
    bit_mask: u64,
    partitions: Vec<UnsafeSyncCell<Partition<Sampl, Point>>>,
    num_partitions: usize,
    bounds: OptionAABB<Comp>,
    node_id: MetaTreeNodeId,
}

pub struct PartitionedPoints<Point> {
    partitions: Vec<UnsafeSyncCell<Vec<Point>>>,
}

struct Partition<Sampl, Point> {
    sampling: Sampl,
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
        PartitionedNode {
            hasher,
            bit_mask,
            partitions,
            num_partitions,
            bounds: OptionAABB::empty(),
            node_id,
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

    pub fn drain_sampled_points(&mut self) -> PartitionedPoints<Sampl::Raw> {
        let partitions = self
            .partitions
            .iter_mut()
            .map(|partition| partition.get_mut().sampling.drain_raw())
            .collect();
        PartitionedPoints::from_partitions(partitions)
    }
}

impl<Sampl, Point, Comp, Pos, Raw> PartitionedNode<Sampl, Point, Comp>
where
    Sampl: Sampling<Point = Point, Raw = Raw> + Send,
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
    ) -> Result<Self, IndexError>
    where
        SamplF: SamplingFactory<Sampling = Sampl, Param = LodLevel>,
        LasL: LasReadWrite<Point, CSys> + Sync,
        CSys: PartialEq + Sync,
    {
        let s1 = span!("parallel_load: prepare");
        let mut this = Self::new(num_partitions, node_id, sampling_factory);
        assert_eq!(threads.num_threads(), this.num_partitions());

        let partitions = &this.partitions;
        let node_id = &this.node_id;
        drop(s1);
        let partition_bounds = threads
            .execute(|tid| -> Result<OptionAABB<Comp>, IndexError> {
                // load page
                let s1 = span!("parallel_load: load page");
                let file_id = node_id.file(tid);
                let file = page_manager.load(&file_id)?;
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
        mut self,
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
        let bounds = mem::take(&mut self.bounds);
        let node_id = &self.node_id;

        let thread_results = threads
            .execute(|thread_id| -> Result<(), IndexError> {
                // unsafe:
                //      every thread dereferences a different partition
                //      (based on each threads thread id).
                //      So there is exactly one mutable reference for each partition.
                let partition = unsafe { partitions[thread_id].unsafe_get_mut() };

                // prepare what to write to the las file
                let s1 = span!("parallel_store: assemble");
                let bogus_points = Some(partition.bogus.len() as u32);
                let mut points = partition.sampling.drain_points();
                points.append(&mut partition.bogus);
                let bounds = bounds.clone();
                let coordinate_system = coordinate_system.clone();
                let las = Las {
                    points: points.as_slice(),
                    bogus_points,
                    bounds,
                    coordinate_system,
                };
                drop(s1);

                // if there are no points, then we can delete the file
                let exists = !las.points.is_empty();

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
        Patch: Fn(&Point, &mut Point) -> () + Sync,
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

    pub fn parallel_split<GridH, SamplF>(
        &mut self,
        meta_tree: &MetaTree<GridH, Comp>,
        sampling_factory: &SamplF,
        threads: &mut Threads,
    ) -> [Self; 8]
    where
        GridH: GridHierarchy<Position = Pos, Component = Comp>,
        SamplF: SamplingFactory<Sampling = Sampl, Param = LodLevel>,
        Point: WithAttr<SensorPositionAttribute<Pos>>,
    {
        let s0 = span!("parallel_split");
        let s1 = span!("parallel_split: prepare");
        assert_eq!(threads.num_threads(), self.num_partitions());
        let partitions = &self.partitions;

        // center of the node is where to split
        let node_center = meta_tree.node_center(&self.node_id);

        // prepare children to insert points into
        let mut children = self.node_id.children().map(|child| {
            (PartitionedNode {
                hasher: self.hasher.clone(),
                bit_mask: self.bit_mask,
                partitions: (0..self.num_partitions)
                    .map(|_| {
                        UnsafeSyncCell::new(Partition {
                            sampling: sampling_factory.build(child.lod()),
                            bogus: Vec::new(),
                        })
                    })
                    .collect(),
                num_partitions: self.num_partitions,
                bounds: OptionAABB::empty(),
                node_id: child,
            })
        });

        // split every partition in parallel
        drop(s1);
        let partition_child_bounds = threads
            .execute(|thread_id| {
                // partition to split
                // unsafe:
                //      every thread dereferences a different partition
                //      (based on each threads thread id).
                //      So there is exactly one mutable reference for each partition.
                let s1 = span!("parallel_split: prepare thread");
                let partition = unsafe { partitions[thread_id].unsafe_get_mut() };

                // partitions of child nodes to insert split points into
                // unsafe:
                //      same as for `partition`
                let mut target_partitions: Vec<_> = (0..8)
                    .map(|i| unsafe { children[i].partitions[thread_id].unsafe_get_mut() })
                    .collect();

                // bounding boxes of all the partitions
                let mut bounds: Vec<_> = (0..8).map(|_| OptionAABB::empty()).collect();
                drop(s1);

                // split sampled points
                let s1 = span!("parallel_split: split sampled points");
                let raw_points = partition.sampling.drain_raw();
                let mut raw_points_split: Vec<_> = (0..8).map(|_| Vec::new()).collect();
                for point in raw_points {
                    let sensor_pos = point.point().attribute::<SensorPositionAttribute<Pos>>();
                    let child_index = node_select_child(&node_center, &sensor_pos.0);
                    bounds[child_index].extend(point.point().position());
                    raw_points_split[child_index].push(point);
                }
                for (i, raw_points) in raw_points_split.into_iter().enumerate() {
                    let rejected = target_partitions[i]
                        .sampling
                        .insert_raw(raw_points, |_, _| unreachable!());
                    assert!(rejected.is_empty())
                }
                drop(s1);

                // split bogus points
                let s1 = span!("parallel_split: split bogus points");
                let bogus_points = mem::take(&mut partition.bogus);
                for point in bogus_points {
                    let sensor_pos = point.attribute::<SensorPositionAttribute<Pos>>();
                    let child_index = node_select_child(&node_center, &sensor_pos.0);
                    bounds[child_index].extend(point.position());
                    target_partitions[child_index].bogus.push(point);
                }
                drop(s1);

                // return bounds so they can be handled by the main thread
                bounds
            })
            .join();

        for child_bounds in partition_child_bounds {
            for (child_index, bounds) in child_bounds.into_iter().enumerate() {
                children[child_index].bounds.extend_other(&bounds);
            }
        }

        drop(s0);
        children
    }
}

pub fn node_select_child<Pos>(node_center: &Pos, sensor_pos: &Pos) -> usize
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
            partitions: partitions
                .into_iter()
                .map(|points| UnsafeSyncCell::new(points))
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.partitions.iter().all(|b| b.get().is_empty())
    }

    pub fn parallel_split(self) {}
}
