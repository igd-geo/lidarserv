use crate::geometry::bounding_box::OptionAABB;
use crate::geometry::grid::{GridCell, LodLevel};
use crate::geometry::points::PointType;
use crate::geometry::position::{Component, CoordinateSystem, Position};
use crate::geometry::sampling::{RawSamplingEntry, Sampling, SamplingFactory};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId, Node};
use crate::index::sensor_pos::partitioned_node::{PartitionedNode, RustCellHasher};
use crate::index::sensor_pos::writer::IndexError;
use crate::las::{Las, LasReadWrite, ReadLasError};
use crate::lru_cache::pager::{
    CacheLoadError, IoError, PageDirectory, PageFileHandle, PageLoader,
    PageManager as GenericPageManager,
};
use crate::nalgebra::Scalar;
use crate::span;
use crate::utils::thread_pool::Threads;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Cursor, ErrorKind, Read, Seek, Write};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, Mutex};

pub struct SensorPosPage<Sampl, Point, Comp: Scalar> {
    // mutexes must be always locked in the order 'binary, points, node', in order to avoid deadlocks.
    binary: Mutex<Option<Arc<Vec<u8>>>>,
    points: Mutex<Option<Arc<SimplePoints<Point, Comp>>>>,
    node: Mutex<Option<Arc<PartitionedNode<Sampl, Point, Comp>>>>,
}

pub struct SimplePoints<Point, Comp: Scalar> {
    pub points: Vec<Point>,
    pub bounds: OptionAABB<Comp>,
    pub non_bogus_points: u32,
}

impl<Sampl, Point, Comp: Scalar> Default for SensorPosPage<Sampl, Point, Comp> {
    fn default() -> Self {
        SensorPosPage {
            binary: Mutex::new(Some(Arc::new(vec![]))),
            points: Mutex::new(None),
            node: Mutex::new(None),
        }
    }
}

impl<Sampl, Point, Comp: Scalar> SensorPosPage<Sampl, Point, Comp> {
    pub fn new_from_binary(bin: Vec<u8>) -> Self {
        SensorPosPage {
            binary: Mutex::new(Some(Arc::new(bin))),
            points: Mutex::new(None),
            node: Mutex::new(None),
        }
    }

    pub fn new_from_node(node: PartitionedNode<Sampl, Point, Comp>) -> Self {
        SensorPosPage {
            binary: Mutex::new(None),
            points: Mutex::new(None),
            node: Mutex::new(Some(Arc::new(node))),
        }
    }

    pub fn exists(&self) -> bool
    where
        Sampl: Sampling,
        Comp: Component,
    {
        let data_exists = self.binary.lock().unwrap().as_ref().map(|d| !d.is_empty());
        if let Some(result) = data_exists {
            return result;
        }
        let node_exists = self
            .node
            .lock()
            .unwrap()
            .as_ref()
            .map(|n| n.nr_points() > 0);
        if let Some(result) = node_exists {
            return result;
        }
        self.points
            .lock()
            .unwrap()
            .as_ref()
            .map(|p| !p.points.is_empty())
            .unwrap_or(false)
    }
}
impl<Sampl, Point, Comp, Pos> SensorPosPage<Sampl, Point, Comp>
where
    // ok
    Comp: Component,
    Point: PointType<Position = Pos> + Clone,
    Pos: Position<Component = Comp>,
    Sampl: Sampling<Point = Point>,
{
    fn points_to_binary<LasL, CSys>(
        points: &Arc<SimplePoints<Point, Comp>>,
        las_loader: &LasL,
        coordinate_system: CSys,
    ) -> Vec<u8>
    where
        LasL: LasReadWrite<Point, CSys>,
    {
        let mut data = Vec::new();
        if !points.points.is_empty() {
            las_loader
                .write_las(
                    Las {
                        points: points.points.iter(),
                        bounds: points.bounds.clone(),
                        non_bogus_points: Some(points.non_bogus_points),
                        coordinate_system,
                    },
                    Cursor::new(&mut data),
                )
                .unwrap(); // unwrap: writing to a cursor does not throw i/o errors
        }
        data
    }

    fn node_to_points(
        node: &Arc<PartitionedNode<Sampl, Point, Comp>>,
    ) -> SimplePoints<Point, Comp> {
        let (points, bounds, nr_non_bogus) = node.get_las_points();
        SimplePoints {
            points,
            bounds,
            non_bogus_points: nr_non_bogus,
        }
    }

    fn points_to_node<SamplF>(
        points: &Arc<SimplePoints<Point, Comp>>,
        num_partitions: usize,
        node_id: MetaTreeNodeId,
        sampling_factory: &SamplF,
        hasher: RustCellHasher,
    ) -> PartitionedNode<Sampl, Point, Comp>
    where
        SamplF: SamplingFactory<Sampling = Sampl>,
    {
        PartitionedNode::from_las_points(
            num_partitions,
            node_id,
            sampling_factory,
            hasher,
            points.points.clone(),
            points.non_bogus_points as usize,
        )
    }

    fn binary_to_points<LasL, CSys>(
        binary: &Arc<Vec<u8>>,
        las_loader: &LasL,
    ) -> Result<SimplePoints<Point, Comp>, ReadLasError>
    where
        LasL: LasReadWrite<Point, CSys>,
    {
        if binary.is_empty() {
            Ok(SimplePoints {
                points: vec![],
                bounds: OptionAABB::empty(),
                non_bogus_points: 0,
            })
        } else {
            let las = las_loader.read_las(Cursor::new(binary.as_slice()))?;
            Ok(SimplePoints {
                points: las.points,
                bounds: las.bounds,
                non_bogus_points: las.non_bogus_points.unwrap_or(u32::MAX),
            })
        }
    }

    pub fn get_binary<LasL, CSys>(&self, las_loader: &LasL, coordinate_system: CSys) -> Arc<Vec<u8>>
    where
        LasL: LasReadWrite<Point, CSys>,
    {
        // try to get existing node data
        let mut bin_lock = self.binary.lock().unwrap();
        if let Some(arc) = bin_lock.as_ref() {
            return Arc::clone(arc);
        }

        // try to encode from points
        let points = {
            let mut points_lock = self.points.lock().unwrap();
            if let Some(points) = &*points_lock {
                Arc::clone(points)
            } else {
                // second unwrap: must be Some(_), if self.binary and self.points both are None.
                let node = self.node.lock().unwrap().as_ref().map(Arc::clone).unwrap();
                let points = Self::node_to_points(&node);
                let points = Arc::new(points);
                *points_lock = Some(Arc::clone(&points));
                points
            }
        };

        let data = Self::points_to_binary(&points, las_loader, coordinate_system);
        let data = Arc::new(data);
        *bin_lock = Some(Arc::clone(&data));
        data
    }

    pub fn get_points<LasL, CSys>(
        &self,
        las_loader: &LasL,
    ) -> Result<Arc<SimplePoints<Point, Comp>>, ReadLasError>
    where
        LasL: LasReadWrite<Point, CSys>,
    {
        {
            // try to get existing points data
            let mut points_lock = self.points.lock().unwrap();
            if let Some(arc) = points_lock.as_ref() {
                return Ok(Arc::clone(arc));
            }

            // try to get points from node
            let node = self.node.lock().unwrap().as_ref().map(Arc::clone);
            if let Some(node) = node {
                let points = Self::node_to_points(&node);
                let points = Arc::new(points);
                *points_lock = Some(Arc::clone(&points));
                return Ok(points);
            }
        }

        // get points from binary
        // unwrap: if self.points and self.node both are None, then self.binary must be Some()
        let binary = self
            .binary
            .lock()
            .unwrap()
            .as_ref()
            .map(Arc::clone)
            .unwrap();
        let mut points_lock = self.points.lock().unwrap();
        if let Some(arc) = points_lock.as_ref() {
            return Ok(Arc::clone(arc));
        }
        let points = Self::binary_to_points(&binary, las_loader)?;
        let points = Arc::new(points);
        *points_lock = Some(Arc::clone(&points));
        return Ok(points);
    }

    pub fn get_node<LasL, CSys, SamplF>(
        &self,
        node_id: MetaTreeNodeId,
        num_partitions: usize,
        sampling_factory: &SamplF,
        las_loader: &LasL,
        hasher: RustCellHasher,
    ) -> Result<Arc<PartitionedNode<Sampl, Point, Comp>>, IndexError>
    where
        LasL: LasReadWrite<Point, CSys>,
        SamplF: SamplingFactory<Sampling = Sampl>,
    {
        // try to get existing node
        {
            let node_lock = self.node.lock().unwrap();
            if let Some(node) = node_lock.as_ref() {
                return Ok(Arc::clone(node));
            }
        }

        // try converting from points
        let points = self.points.lock().unwrap().as_ref().map(Arc::clone);
        if let Some(points) = points {
            let mut node_lock = self.node.lock().unwrap();
            if let Some(node) = node_lock.as_ref() {
                return Ok(Arc::clone(node));
            }
            let node =
                Self::points_to_node(&points, num_partitions, node_id, sampling_factory, hasher);
            let node = Arc::new(node);
            *node_lock = Some(Arc::clone(&node));
            return Ok(node);
        }

        // convert from binary
        let binary = Arc::clone(self.binary.lock().unwrap().as_ref().unwrap());
        let mut points_lock = self.points.lock().unwrap();
        let mut node_lock = self.node.lock().unwrap();
        if let Some(node) = node_lock.as_ref() {
            return Ok(Arc::clone(node));
        }
        let points = if let Some(points) = points_lock.as_ref() {
            Arc::clone(points)
        } else {
            let points = Self::binary_to_points(&binary, las_loader)?;
            let points = Arc::new(points);
            *points_lock = Some(Arc::clone(&points));
            points
        };
        drop(points_lock);
        let node = Self::points_to_node(&points, num_partitions, node_id, sampling_factory, hasher);
        let node = Arc::new(node);
        *node_lock = Some(Arc::clone(&node));
        Ok(node)
    }
}

impl<Sampl, Point, Comp, Raw, Pos> SensorPosPage<Sampl, Point, Comp>
where
    Sampl: Sampling<Point = Point, Raw = Raw> + Send + Clone,
    Point: PointType<Position = Pos> + Send + Sync + Clone,
    Comp: Component + Send + Sync,
    Raw: RawSamplingEntry<Point = Point> + Send,
    Pos: Position<Component = Comp> + Sync,
{
    fn points_to_binary_par<LasL, CSys>(
        points: &Arc<SimplePoints<Point, Comp>>,
        las_loader: &LasL,
        coordinate_system: CSys,
        threads: &mut Threads,
    ) -> Vec<u8>
    where
        LasL: LasReadWrite<Point, CSys>,
    {
        if points.points.is_empty() {
            Vec::new()
        } else {
            las_loader.write_las_par(
                Las {
                    points: &points.points,
                    bounds: points.bounds.clone(),
                    non_bogus_points: Some(points.non_bogus_points),
                    coordinate_system,
                },
                threads,
            )
        }
    }

    fn binary_to_points_par<LasL, CSys>(
        binary: &Arc<Vec<u8>>,
        las_loader: &LasL,
        threads: &mut Threads,
    ) -> Result<SimplePoints<Point, Comp>, ReadLasError>
    where
        LasL: LasReadWrite<Point, CSys>,
    {
        if binary.is_empty() {
            Ok(SimplePoints {
                points: vec![],
                bounds: OptionAABB::empty(),
                non_bogus_points: 0,
            })
        } else {
            let las = las_loader.read_las_par(binary.as_slice(), threads)?;
            Ok(SimplePoints {
                points: las.points,
                bounds: las.bounds,
                non_bogus_points: las.non_bogus_points.unwrap_or(u32::MAX),
            })
        }
    }

    pub fn get_binary_par<LasL, CSys>(
        &self,
        threads: &mut Threads,
        las_loader: &LasL,
        coordinate_system: CSys,
    ) -> Arc<Vec<u8>>
    where
        CSys: Clone + Sync,
        LasL: LasReadWrite<Point, CSys> + Sync,
    {
        // try to get existing node data
        let mut binary_lock = self.binary.lock().unwrap();
        if let Some(arc) = binary_lock.as_ref() {
            return Arc::clone(arc);
        }

        // try to convert from points
        let points = {
            let mut points_lock = self.points.lock().unwrap();
            if let Some(points) = points_lock.as_ref() {
                Arc::clone(points)
            } else {
                // second unwrap: must be Some(_), if self.binary and self.points both are None.
                let node = Arc::clone(self.node.lock().unwrap().as_ref().unwrap());
                let points = Self::node_to_points(&node);
                let points = Arc::new(points);
                *points_lock = Some(Arc::clone(&points));
                points
            }
        };
        let binary = Self::points_to_binary_par(&points, las_loader, coordinate_system, threads);
        let binary = Arc::new(binary);
        *binary_lock = Some(Arc::clone(&binary));
        binary
    }

    pub fn get_node_par<SamplF, LasL, CSys>(
        &self,
        node_id: MetaTreeNodeId,
        num_partitions: usize,
        sampling_factory: &SamplF,
        las_loader: &LasL,
        hasher: RustCellHasher,
        threads: &mut Threads,
    ) -> Result<Arc<PartitionedNode<Sampl, Point, Comp>>, IndexError>
    where
        LasL: LasReadWrite<Point, CSys> + Sync,
        CSys: PartialEq + Sync,
        SamplF: SamplingFactory<Sampling = Sampl> + Sync,
    {
        // try to get existing node
        {
            let node_lock = self.node.lock().unwrap();
            if let Some(node) = node_lock.as_ref() {
                return Ok(Arc::clone(node));
            }
        }

        // try to convert from points
        {
            let points = self.points.lock().unwrap().as_ref().map(Arc::clone);
            let mut node_lock = self.node.lock().unwrap();
            if let Some(node) = node_lock.as_ref() {
                return Ok(Arc::clone(node));
            }
            if let Some(points) = points {
                let node = Self::points_to_node(
                    &points,
                    num_partitions,
                    node_id,
                    sampling_factory,
                    hasher,
                );
                let node = Arc::new(node);
                *node_lock = Some(Arc::clone(&node));
                return Ok(node);
            }
        }

        // convert from binary
        let binary = Arc::clone(self.binary.lock().unwrap().as_ref().unwrap());
        let mut points_lock = self.points.lock().unwrap();
        let mut node_lock = self.node.lock().unwrap();
        if let Some(node) = node_lock.as_ref() {
            return Ok(Arc::clone(node));
        }
        let points = if let Some(points) = points_lock.as_ref() {
            Arc::clone(points)
        } else {
            let points = Self::binary_to_points_par(&binary, las_loader, threads)?;
            let points = Arc::new(points);
            *points_lock = Some(Arc::clone(&points));
            points
        };
        drop(points_lock);
        let node = Self::points_to_node(&points, num_partitions, node_id, sampling_factory, hasher);
        let node = Arc::new(node);
        *node_lock = Some(Arc::clone(&node));
        Ok(node)
    }
}

pub struct FileHandle<LasL, CSys, Sampl> {
    file_name: PathBuf,
    coordinate_system: CSys,
    las_loader: LasL,
    _phantom: PhantomData<Sampl>,
}

impl<Sampl, LasL, CSys, Point> PageFileHandle for FileHandle<LasL, CSys, Sampl>
where
    // ok
    CSys: Clone,
    LasL: LasReadWrite<Point, CSys>,
    Point: PointType + Clone,
    Sampl: Sampling<Point = Point>,
{
    type Data = SensorPosPage<Sampl, Point, <Point::Position as Position>::Component>;

    fn load(&mut self) -> Result<Self::Data, CacheLoadError> {
        let mut file = match File::open(&self.file_name) {
            Ok(f) => f,
            Err(e) => {
                return if e.kind() == ErrorKind::NotFound {
                    Ok(SensorPosPage::new_from_binary(vec![]))
                } else {
                    Err(CacheLoadError::IO { source: e.into() })
                }
            }
        };
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let page = SensorPosPage::new_from_binary(data);
        Ok(page)
    }

    fn store(&mut self, data: &Self::Data) -> Result<(), IoError> {
        let bytes = data.get_binary(&self.las_loader, self.coordinate_system.clone());
        if !bytes.is_empty() {
            let s = span!("BinDataFileHandle::store: write and sync");
            s.emit_value(bytes.len() as u64);
            let mut file = File::create(&self.file_name)?;
            file.write_all(bytes.as_slice())?;
            file.sync_all()?;
            drop(s);
        } else {
            let s = span!("BinDataFileHandle::store: delete");
            match std::fs::remove_file(&self.file_name) {
                Ok(_) => (),
                Err(e) => {
                    return if e.kind() == ErrorKind::NotFound {
                        // if the file did not exist to begin with, it is OK.
                        Ok(())
                    } else {
                        Err(e.into())
                    };
                }
            }
            drop(s);
        }
        Ok(())
    }
}

pub struct Loader<CSys, LasL, Sampl> {
    base_path: PathBuf,
    extension: &'static str,
    coordinate_system: CSys,
    las_loader: LasL,
    _phantom: PhantomData<fn() -> Sampl>,
}

impl<CSys, LasL, Sampl> Loader<CSys, LasL, Sampl> {
    pub fn new(
        base_path: PathBuf,
        compressed: bool,
        coordinate_system: CSys,
        las_loader: LasL,
    ) -> Self {
        Loader {
            base_path,
            extension: if compressed { "laz" } else { "las" },
            coordinate_system,
            las_loader,
            _phantom: PhantomData,
        }
    }
}

impl<CSys, LasL, Sampl, Point> PageLoader for Loader<CSys, LasL, Sampl>
where
    // ok
    CSys: Clone,
    LasL: LasReadWrite<Point, CSys> + Clone,
    Point: PointType + Clone,
    Sampl: Sampling<Point = Point>,
{
    type FileName = MetaTreeNodeId;
    type FileHandle = FileHandle<LasL, CSys, Sampl>;

    fn open(&self, file: &Self::FileName) -> Self::FileHandle {
        let filename = format!(
            "{}__{}__{}-{}-{}.{}",
            file.lod().level(),
            file.tree_depth().level(),
            file.grid_cell().x,
            file.grid_cell().y,
            file.grid_cell().z,
            self.extension
        );
        let mut path = self.base_path.clone();
        path.push(filename);
        FileHandle {
            file_name: path,
            coordinate_system: self.coordinate_system.clone(),
            las_loader: self.las_loader.clone(),
            _phantom: PhantomData,
        }
    }
}

pub struct FileIdDirectory {
    files: HashSet<MetaTreeNodeId>,
}

impl FileIdDirectory {
    pub fn new() -> Self {
        FileIdDirectory {
            files: HashSet::new(),
        }
    }

    pub fn from_meta_tree<GridH, Comp: Scalar>(meta_tree: &MetaTree<GridH, Comp>) -> Self {
        let mut directory = FileIdDirectory::new();
        for node in meta_tree.nodes() {
            directory.files.insert(node);
        }
        directory
    }
}

impl Default for FileIdDirectory {
    fn default() -> Self {
        FileIdDirectory::new()
    }
}

impl PageDirectory for FileIdDirectory {
    type Key = MetaTreeNodeId;

    fn insert(&mut self, key: &Self::Key) {
        self.files.insert(key.clone());
    }

    fn exists(&self, key: &Self::Key) -> bool {
        self.files.contains(key)
    }
}

/*pub type PageManager = GenericPageManager<
    BinDataLoader,
    MetaTreeNodeId,
    BinDataPage,
    BinDataFileHandle,
    FileIdDirectory,
>;*/
pub type PageManager<Point, Comp, Sampl, LasL, CSys> = GenericPageManager<
    Loader<CSys, LasL, Sampl>,
    MetaTreeNodeId,
    SensorPosPage<Sampl, Point, Comp>,
    FileHandle<LasL, CSys, Sampl>,
    FileIdDirectory,
>;
