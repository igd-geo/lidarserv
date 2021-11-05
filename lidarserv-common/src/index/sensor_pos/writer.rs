use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
use crate::geometry::grid::{GridHierarchy, LodLevel};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{Component, Position};
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::index::sensor_pos::page_manager::{BinDataPage, PageManager};
use crate::index::sensor_pos::point::SensorPositionAttribute;
use crate::index::sensor_pos::{Inner, Update};
use crate::index::Writer;
use crate::las::{Las, LasReadWrite, ReadLasError, WriteLasError};
use crate::lru_cache::pager::{CacheCleanupError, CacheLoadError};
use crate::nalgebra::Scalar;
use crossbeam_channel::{Receiver, Sender};
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
    #[error("Could not read or write page '{key_name}': {source}")]
    PageIo {
        #[source]
        source: CacheLoadError,
        key_name: String,
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
            key_name: format!("{:?}", e.key),
            source: CacheLoadError::IO { source: e.source },
        }
    }
}

trait WithKey<OkValue> {
    fn with_key<K: Debug>(self, key: &K) -> Result<OkValue, IndexError>;
}

impl<OkValue> WithKey<OkValue> for Result<OkValue, CacheLoadError> {
    fn with_key<K: Debug>(self, key: &K) -> Result<OkValue, IndexError> {
        self.map_err(|e| IndexError::PageIo {
            source: e,
            key_name: format!("{:?}", key),
        })
    }
}

pub struct SensorPosWriter<Point, CSys> {
    coordinator_join: Option<JoinHandle<Result<(), IndexError>>>,
    new_points_sender: Option<Sender<Vec<Point>>>,
    coordinate_system: CSys,
}

impl<Point, Pos, Comp, CSys> SensorPosWriter<Point, CSys>
where
    Point:
        PointType<Position = Pos> + WithAttr<SensorPositionAttribute<Pos>> + Send + Sync + 'static,
    Pos: Position<Component = Comp> + Clone,
    Comp: Component + Send + Sync + Serialize + DeserializeOwned,
    CSys: PartialEq + Send + Sync + 'static + Clone,
{
    pub(super) fn new<GridH, SamplF, LasL>(
        index_inner: Arc<Inner<GridH, SamplF, Comp, LasL, CSys>>,
    ) -> Self
    where
        GridH: GridHierarchy<Position = Pos, Component = Comp> + Clone + Send + Sync + 'static,
        SamplF: SamplingFactory<Point = Point, Param = LodLevel> + Sync + Send + 'static,
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

fn coordinator_thread<GridH, SamplF, Point, Sampl, Pos, Comp, LasL, CSys>(
    inner: Arc<Inner<GridH, SamplF, Comp, LasL, CSys>>,
    new_points_receiver: Receiver<Vec<Point>>,
    mut meta_tree: MetaTree<GridH, Comp>,
) -> Result<(), IndexError>
where
    Comp: Component + Send + Sync + Serialize + DeserializeOwned,
    SamplF: SamplingFactory<Sampling = Sampl, Param = LodLevel> + Send + Sync + 'static,
    Sampl: Sampling<Point = Point>,
    Point: PointType<Position = Pos> + WithAttr<SensorPositionAttribute<Pos>>,
    Pos: Position<Component = Comp> + Clone,
    GridH: GridHierarchy<Component = Comp, Position = Pos> + Send + Sync + 'static,
    LasL: LasReadWrite<Point, CSys> + Send + Sync + 'static,
    CSys: Clone + PartialEq + Send + Sync + 'static,
{
    // start thread that publishes changes to reders
    let (changes_sender, changes_receiver) = crossbeam_channel::unbounded();
    let inner_clone = Arc::clone(&inner);
    let notify_thread = spawn(move || notify_readers_thread(changes_receiver, inner_clone));

    let mut new_points = VecDeque::new();
    let mut loaded_nodes = Vec::<Node<Sampl, Comp, Point>>::new();
    let nr_threads = inner.nr_threads;

    'main: loop {
        // make sure we have points to insert
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
        let first_point = &new_points[0][0];
        let sensor_pos = first_point
            .attribute::<SensorPositionAttribute<_>>()
            .0
            .clone();
        let nodes = meta_tree.query_sensor_position(&sensor_pos);

        // get the points from the head of new_points,
        // that can be inserted into the same nodes.
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

        // transfer points into buffers for individual worker threads
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

        // insert points into each lod, top-to-bottom
        // until no points are left in all worker buffers.
        let mut lod = LodLevel::base();
        while worker_buffers.iter().any(|b| !b.is_empty()) {
            // get sampling for this node
            let node_id = nodes.node_for_lod(&lod);
            let lod_level = lod.level() as usize;

            // load from disk, if needed
            if lod_level >= loaded_nodes.len() || loaded_nodes[lod_level].node_id != node_id {
                // load node data
                let mut pages = Vec::new();
                for thread_id in 0..nr_threads {
                    let file = node_id.file(thread_id);
                    let page = inner.page_manager.load(&file).with_key(&file)?;
                    pages.push(page);
                }

                // read laz
                let node_points: Vec<Vec<Point>> = pages
                    .iter()
                    .map(|page| read_laz(&page.data, &inner.las_loader, &inner.coordinate_system))
                    .collect::<Result<_, IndexError>>()?;

                // create node
                let mut sampling = inner.sampling_factory.build(&lod);
                let mut aabb = OptionAABB::empty();
                let mut bogus_points = Vec::new();
                for mut points in node_points {
                    for point in &points {
                        aabb.extend(point.position());
                    }
                    if lod == inner.max_lod {
                        bogus_points.append(&mut points)
                    } else {
                        let rejected = sampling.insert(points, |_, _| ());
                        assert!(rejected.is_empty()) // should not reject points, that were part of the node.
                    }
                }
                let node = Node {
                    node_id: node_id.clone(),
                    sampling,
                    aabb,
                    bogus_points,
                };

                // keep around in loaded_nodes, so we can re-use it in the following iterations.
                match lod_level.cmp(&loaded_nodes.len()) {
                    Ordering::Less => {
                        // if the newly loaded node replaces a previous one, we  also need to
                        // write that to disk.
                        let mut old_node = mem::replace(&mut loaded_nodes[lod_level], node);
                        if let Some(aabb) = old_node.aabb.clone().into_aabb() {
                            meta_tree.set_node_aabb(&old_node.node_id, &aabb)
                        }
                        write_node_to_cache(
                            {
                                let mut points = old_node.sampling.into_points();
                                points.append(&mut old_node.bogus_points);
                                points
                            },
                            old_node.aabb,
                            &inner.las_loader,
                            &inner.coordinate_system,
                            nr_threads,
                            old_node.node_id,
                            &inner.page_manager,
                        )?;
                    }
                    Ordering::Equal => {
                        loaded_nodes.push(node);
                    }
                    Ordering::Greater => {
                        unreachable!()
                    }
                };

                // make sure we do not overfill the cache while loading nodes
                inner.page_manager.cleanup_one()?;
            }
            let node = &mut loaded_nodes[lod_level];

            // add new points
            for points in &mut worker_buffers {
                for point in &*points {
                    node.aabb.extend(point.position());
                }
                if lod == inner.max_lod {
                    node.bogus_points.append(points);
                } else {
                    let points_owned = mem::take(points);
                    *points = node.sampling.insert(points_owned, |p, q| {
                        *q.attribute_mut::<SensorPositionAttribute<Pos>>() =
                            p.attribute::<SensorPositionAttribute<Pos>>().clone()
                    });
                }
            }

            // check, if we need to split the node
            if node.sampling.len() > inner.max_node_size {
                let mut queue = Vec::new();
                write_node_to_cache(
                    Vec::<Point>::new(),
                    OptionAABB::empty(),
                    &inner.las_loader,
                    &inner.coordinate_system,
                    nr_threads,
                    node_id.clone(),
                    &inner.page_manager,
                )?;
                meta_tree.set_node_aabb(&node_id, &node.aabb.clone().into_aabb().unwrap()); // unwrap: the node cannot be empty, because it exceeded the max node size.
                let mut node_to_split = node_id.clone();
                let mut split_points = node.sampling.drain_points();
                split_points.append(&mut node.bogus_points);
                let mut split_aabb = node.aabb.clone();
                while split_points.len() > inner.max_node_size {
                    let (children, center) =
                        split_node(&mut meta_tree, node_to_split, split_points);
                    let replacement = node_select_child(&center, &sensor_pos);
                    let mut children = Vec::from(children);
                    let (replacement_node_id, replacement_points, replacement_aabb) =
                        children.swap_remove(replacement);
                    node_to_split = replacement_node_id;
                    split_points = replacement_points;
                    split_aabb = replacement_aabb;
                    for (node, points, aabb) in children {
                        queue.push((node, points, aabb))
                    }
                }
                node.node_id = node_to_split;
                node.aabb = split_aabb;
                if lod == inner.max_lod {
                    node.bogus_points.append(&mut split_points)
                } else {
                    node.sampling.insert(split_points, |_, _| ());
                }

                while let Some((node_to_split, split_points, aabb)) = queue.pop() {
                    // no need to split further, if below the max node size
                    if split_points.len() <= inner.max_node_size {
                        write_node_to_cache(
                            split_points,
                            aabb,
                            &inner.las_loader,
                            &inner.coordinate_system,
                            nr_threads,
                            node_to_split,
                            &inner.page_manager,
                        )?;
                    } else {
                        let (children, _) = split_node(&mut meta_tree, node_to_split, split_points);
                        for (node, points, aabb) in IntoIter::new(children) {
                            queue.push((node, points, aabb));
                        }
                    }
                }
            }

            // next lod level in next loop iteration
            lod = lod.finer();
        }
    }

    // write remaining nodes to disk
    for mut node in loaded_nodes {
        if let Some(aabb) = node.aabb.clone().into_aabb() {
            meta_tree.set_node_aabb(&node.node_id, &aabb)
        }
        write_node_to_cache(
            {
                let mut points = node.sampling.into_points();
                points.append(&mut node.bogus_points);
                points
            },
            node.aabb,
            &inner.las_loader,
            &inner.coordinate_system,
            nr_threads,
            node.node_id,
            &inner.page_manager,
        )?;
    }

    // dump metatree to disk
    meta_tree
        .write_to_file(&inner.meta_tree_file)
        .map_err(|e| IndexError::Other(Box::new(e)))?;

    // stop notify thread
    drop(changes_sender);
    notify_thread.join().unwrap();

    Ok(())
}

fn notify_readers_thread<GridH, SamplF, Comp, LasL, CSys>(
    changes_receiver: crossbeam_channel::Receiver<Update<Comp>>,
    inner: Arc<Inner<GridH, SamplF, Comp, LasL, CSys>>,
) where
    Comp: Component,
    GridH: GridHierarchy<Component = Comp>,
{
    for change in changes_receiver {
        let mut shared = inner.shared.write().unwrap();

        // update tree
        for (node, aabb, _) in &change.replaced_by {
            shared.meta_tree.set_node_aabb(node, aabb);
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

type FilledNode<Point, Comp> = (MetaTreeNodeId, Vec<Point>, OptionAABB<Comp>);

fn split_node<GridH, Comp, Point, Pos>(
    meta_tree: &mut MetaTree<GridH, Comp>,
    split_node: MetaTreeNodeId,
    points: Vec<Point>,
) -> ([FilledNode<Point, Comp>; 8], Pos)
where
    Comp: Component,
    GridH: GridHierarchy<Component = Comp, Position = Pos>,
    Pos: Position<Component = Comp>,
    Point: PointType<Position = Pos> + WithAttr<SensorPositionAttribute<Pos>>,
{
    meta_tree.split_node(&split_node);
    let node_center = meta_tree.node_center(&split_node);
    let mut children = split_node
        .children()
        .map(|child| (child, Vec::new(), OptionAABB::empty()));
    for point in points {
        let sensor_pos = point.attribute::<SensorPositionAttribute<Pos>>();
        let child_num = node_select_child(&node_center, &sensor_pos.0);
        children[child_num].2.extend(point.position());
        children[child_num].1.push(point);
    }
    for (node, _points, aabb) in &children {
        if let Some(aabb) = aabb.clone().into_aabb() {
            meta_tree.set_node_aabb(node, &aabb);
        }
    }

    (children, node_center)
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

fn write_node_to_cache<Point, LasL, CSys>(
    points: Vec<Point>,
    bounds: OptionAABB<<Point::Position as Position>::Component>,
    loader: &LasL,
    coordinate_system: &CSys,
    nr_threads: usize,
    node_id: MetaTreeNodeId,
    page_manager: &PageManager,
) -> Result<(), IndexError>
where
    Point: PointType,
    LasL: LasReadWrite<Point, CSys>,
    CSys: Clone,
{
    let mut page = Vec::<u8>::new();
    let write = Cursor::new(&mut page);
    loader.write_las(
        Las {
            points: &points,
            bounds: bounds.clone(),
            bogus_points: None,
            coordinate_system: coordinate_system.clone(),
        },
        write,
    )?;
    let mut empty_page = Vec::<u8>::new();
    let write = Cursor::new(&mut empty_page);
    loader.write_las(
        Las {
            points: &[],
            bounds,
            bogus_points: None,
            coordinate_system: coordinate_system.clone(),
        },
        write,
    )?;

    for thread_id in 0..nr_threads {
        let file_id = node_id.file(thread_id);
        let data = if thread_id == 0 {
            mem::take(&mut page)
        } else {
            empty_page.clone()
        };
        page_manager.store(&file_id, BinDataPage { data });
    }

    Ok(())
}

fn read_laz<Point, CSys, LasL>(
    data: &[u8],
    loader: &LasL,
    coordinate_system: &CSys,
) -> Result<Vec<Point>, IndexError>
where
    LasL: LasReadWrite<Point, CSys>,
    Point: PointType,
    CSys: PartialEq,
{
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let read = Cursor::new(data);
    let Las {
        points,
        coordinate_system: las_coordinate_system,
        ..
    } = loader.read_las(read)?;
    if las_coordinate_system != *coordinate_system {
        return Err(IndexError::ReadLas {
            source: ReadLasError::FileFormat {
                desc: "Wrong coordinate system transform".to_string(),
            },
        });
    }
    Ok(points)
}
