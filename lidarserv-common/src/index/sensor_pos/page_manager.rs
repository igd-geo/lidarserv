use crate::geometry::bounding_box::OptionAABB;
use crate::geometry::points::PointType;
use crate::geometry::position::{I32CoordinateSystem, I32Position, Position};
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::index::sensor_pos::partitioned_node::PartitionedNode;
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
use std::io::{Cursor, ErrorKind, Read, Write};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct SensorPosPage<Sampl, Point> {
    // mutexes must be always locked in the order 'binary, points, node', in order to avoid deadlocks.
    binary: Mutex<Option<Arc<Vec<u8>>>>,
    points: Mutex<Option<Arc<SimplePoints<Point>>>>,
    node: Mutex<Option<Arc<PartitionedNode<Sampl, Point>>>>,
}

pub struct SimplePoints<Point> {
    pub points: Vec<Point>,
    pub bounds: OptionAABB<i32>,
    pub non_bogus_points: u32,
}

pub struct Loader<LasL, Sampl> {
    base_path: PathBuf,
    extension: &'static str,
    coordinate_system: I32CoordinateSystem,
    las_loader: LasL,
    _phantom: PhantomData<fn() -> Sampl>,
}

pub struct FileIdDirectory {
    files: HashSet<MetaTreeNodeId>,
}

pub struct FileHandle<LasL, Sampl> {
    file_name: PathBuf,
    coordinate_system: I32CoordinateSystem,
    las_loader: LasL,
    _phantom: PhantomData<Sampl>,
}

impl<Sampl, Point> Default for SensorPosPage<Sampl, Point> {
    fn default() -> Self {
        SensorPosPage {
            binary: Mutex::new(Some(Arc::new(vec![]))),
            points: Mutex::new(None),
            node: Mutex::new(None),
        }
    }
}

impl<Sampl, Point> SensorPosPage<Sampl, Point> {
    pub fn new_from_binary(bin: Vec<u8>) -> Self {
        SensorPosPage {
            binary: Mutex::new(Some(Arc::new(bin))),
            points: Mutex::new(None),
            node: Mutex::new(None),
        }
    }

    pub fn new_from_node(node: PartitionedNode<Sampl, Point>) -> Self {
        SensorPosPage {
            binary: Mutex::new(None),
            points: Mutex::new(None),
            node: Mutex::new(Some(Arc::new(node))),
        }
    }
}
impl<Sampl, Point> SensorPosPage<Sampl, Point>
where
    Point: PointType<Position = I32Position> + Clone,
    Sampl: Sampling<Point = Point>,
{
    pub fn exists(&self) -> bool {
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

    fn points_to_binary<LasL>(
        points: &Arc<SimplePoints<Point>>,
        las_loader: &LasL,
        coordinate_system: I32CoordinateSystem,
        for_transmission: bool,
    ) -> Vec<u8>
    where
        LasL: LasReadWrite<Point>,
    {
        let mut data = Vec::new();
        if !points.points.is_empty() {
            let las = Las {
                points: points.points.iter(),
                bounds: points.bounds.clone(),
                non_bogus_points: Some(points.non_bogus_points),
                coordinate_system,
            };
            let cursor = Cursor::new(&mut data);
            if for_transmission {
                las_loader
                    .write_las_force_no_compression(las, cursor)
                    .unwrap(); // unwrap: writing to a cursor does not throw i/o errors
            } else {
                las_loader.write_las(las, cursor).unwrap(); // unwrap: writing to a cursor does not throw i/o errors
            }
        }
        data
    }

    fn node_to_points(node: &Arc<PartitionedNode<Sampl, Point>>) -> SimplePoints<Point> {
        let (points, bounds, nr_non_bogus) = node.get_las_points();
        SimplePoints {
            points,
            bounds,
            non_bogus_points: nr_non_bogus,
        }
    }

    fn points_to_node<SamplF>(
        points: &Arc<SimplePoints<Point>>,
        node_id: MetaTreeNodeId,
        sampling_factory: &SamplF,
    ) -> PartitionedNode<Sampl, Point>
    where
        SamplF: SamplingFactory<Sampling = Sampl>,
    {
        PartitionedNode::from_las_points(
            node_id,
            sampling_factory,
            points.points.clone(),
            points.non_bogus_points as usize,
        )
    }

    fn binary_to_points<LasL>(
        binary: &Arc<Vec<u8>>,
        las_loader: &LasL,
    ) -> Result<SimplePoints<Point>, ReadLasError>
    where
        LasL: LasReadWrite<Point>,
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

    pub fn get_binary<LasL>(
        &self,
        las_loader: &LasL,
        coordinate_system: I32CoordinateSystem,
        for_transmission: bool,
    ) -> Arc<Vec<u8>>
    where
        LasL: LasReadWrite<Point>,
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

        let data = Self::points_to_binary(&points, las_loader, coordinate_system, for_transmission);
        let data = Arc::new(data);
        *bin_lock = Some(Arc::clone(&data));
        data
    }

    pub fn get_points<LasL>(
        &self,
        las_loader: &LasL,
    ) -> Result<Arc<SimplePoints<Point>>, ReadLasError>
    where
        LasL: LasReadWrite<Point>,
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

    pub fn get_node<LasL, SamplF>(
        &self,
        node_id: MetaTreeNodeId,
        sampling_factory: &SamplF,
        las_loader: &LasL,
    ) -> Result<Arc<PartitionedNode<Sampl, Point>>, IndexError>
    where
        LasL: LasReadWrite<Point>,
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
            let node = Self::points_to_node(&points, node_id, sampling_factory);
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
        let node = Self::points_to_node(&points, node_id, sampling_factory);
        let node = Arc::new(node);
        *node_lock = Some(Arc::clone(&node));
        Ok(node)
    }
}

impl<Sampl, Point> SensorPosPage<Sampl, Point>
where
    Sampl: Sampling<Point = Point> + Send + Clone,
    Sampl::Raw: Send,
    Point: PointType<Position = I32Position> + Send + Sync + Clone,
{
    fn points_to_binary_par<LasL>(
        points: &Arc<SimplePoints<Point>>,
        las_loader: &LasL,
        coordinate_system: I32CoordinateSystem,
        threads: &mut Threads,
    ) -> Vec<u8>
    where
        LasL: LasReadWrite<Point>,
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

    fn binary_to_points_par<LasL>(
        binary: &Arc<Vec<u8>>,
        las_loader: &LasL,
        threads: &mut Threads,
    ) -> Result<SimplePoints<Point>, ReadLasError>
    where
        LasL: LasReadWrite<Point>,
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

    pub fn get_binary_par<LasL>(
        &self,
        threads: &mut Threads,
        las_loader: &LasL,
        coordinate_system: I32CoordinateSystem,
    ) -> Arc<Vec<u8>>
    where
        LasL: LasReadWrite<Point> + Sync,
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

    pub fn get_node_par<SamplF, LasL>(
        &self,
        node_id: MetaTreeNodeId,
        sampling_factory: &SamplF,
        las_loader: &LasL,
        threads: &mut Threads,
    ) -> Result<Arc<PartitionedNode<Sampl, Point>>, IndexError>
    where
        LasL: LasReadWrite<Point> + Sync,
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
                let node = Self::points_to_node(&points, node_id, sampling_factory);
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
        let node = Self::points_to_node(&points, node_id, sampling_factory);
        let node = Arc::new(node);
        *node_lock = Some(Arc::clone(&node));
        Ok(node)
    }
}

impl<Sampl, LasL, Point> PageFileHandle for FileHandle<LasL, Sampl>
where
    LasL: LasReadWrite<Point>,
    Point: PointType<Position = I32Position> + Clone,
    Sampl: Sampling<Point = Point>,
{
    type Data = SensorPosPage<Sampl, Point>;

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
        let bytes = data.get_binary(&self.las_loader, self.coordinate_system.clone(), false);
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

impl<LasL, Sampl> Loader<LasL, Sampl> {
    pub fn new(
        base_path: PathBuf,
        compressed: bool,
        coordinate_system: I32CoordinateSystem,
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

impl<LasL, Sampl, Point> PageLoader for Loader<LasL, Sampl>
where
    // ok
    LasL: LasReadWrite<Point> + Clone,
    Point: PointType<Position = I32Position> + Clone,
    Sampl: Sampling<Point = Point>,
{
    type FileName = MetaTreeNodeId;
    type FileHandle = FileHandle<LasL, Sampl>;

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

impl FileIdDirectory {
    pub fn new() -> Self {
        FileIdDirectory {
            files: HashSet::new(),
        }
    }

    pub fn from_meta_tree(meta_tree: &MetaTree) -> Self {
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

pub type PageManager<Point, Sampl, LasL> = GenericPageManager<
    Loader<LasL, Sampl>,
    MetaTreeNodeId,
    SensorPosPage<Sampl, Point>,
    FileHandle<LasL, Sampl>,
    FileIdDirectory,
>;
