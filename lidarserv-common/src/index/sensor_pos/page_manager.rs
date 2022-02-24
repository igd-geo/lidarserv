use crate::geometry::bounding_box::OptionAABB;
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{I32CoordinateSystem, I32Position};
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::index::sensor_pos::partitioned_node::PartitionedNode;
use crate::index::sensor_pos::writer::IndexError;
use crate::las::{I32LasReadWrite, Las, LasExtraBytes, LasPointAttributes, ReadLasError};
use crate::lru_cache::pager::{
    CacheLoadError, IoError, PageDirectory, PageFileHandle, PageLoader,
    PageManager as GenericPageManager,
};
use crate::span;
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

pub struct Loader<Sampl> {
    base_path: PathBuf,
    extension: &'static str,
    coordinate_system: I32CoordinateSystem,
    las_loader: I32LasReadWrite,
    _phantom: PhantomData<fn() -> Sampl>,
}

pub struct FileIdDirectory {
    files: HashSet<MetaTreeNodeId>,
}

pub struct FileHandle<Sampl> {
    file_name: PathBuf,
    coordinate_system: I32CoordinateSystem,
    las_loader: I32LasReadWrite,
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

    pub fn new_from_points(points: SimplePoints<Point>) -> Self {
        SensorPosPage {
            binary: Mutex::new(None),
            points: Mutex::new(Some(Arc::new(points))),
            node: Mutex::new(None),
        }
    }
}
impl<Sampl, Point> SensorPosPage<Sampl, Point>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes + Clone,
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

    fn points_to_binary(
        points: &Arc<SimplePoints<Point>>,
        las_loader: &I32LasReadWrite,
        coordinate_system: I32CoordinateSystem,
    ) -> Vec<u8> {
        if !points.points.is_empty() {
            las_loader.write_las::<Point, _>(Las {
                points: points.points.iter(),
                bounds: points.bounds.clone(),
                non_bogus_points: Some(points.non_bogus_points),
                coordinate_system,
            })
        } else {
            Vec::new()
        }
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

    fn binary_to_points(
        binary: &Arc<Vec<u8>>,
        las_loader: &I32LasReadWrite,
    ) -> Result<SimplePoints<Point>, ReadLasError> {
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

    pub fn get_binary(
        &self,
        las_loader: &I32LasReadWrite,
        coordinate_system: I32CoordinateSystem,
    ) -> Arc<Vec<u8>> {
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

    pub fn get_points(
        &self,
        las_loader: &I32LasReadWrite,
    ) -> Result<Arc<SimplePoints<Point>>, ReadLasError> {
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
        Ok(points)
    }

    pub fn get_node<SamplF>(
        &self,
        node_id: MetaTreeNodeId,
        sampling_factory: &SamplF,
        las_loader: &I32LasReadWrite,
    ) -> Result<Arc<PartitionedNode<Sampl, Point>>, IndexError>
    where
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

impl<Sampl, Point> PageFileHandle for FileHandle<Sampl>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes + Clone,
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

impl<Sampl> Loader<Sampl> {
    pub fn new(
        base_path: PathBuf,
        compressed: bool,
        coordinate_system: I32CoordinateSystem,
        las_loader: I32LasReadWrite,
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

impl<Sampl, Point> PageLoader for Loader<Sampl>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes + Clone,
    Sampl: Sampling<Point = Point>,
{
    type FileName = MetaTreeNodeId;
    type FileHandle = FileHandle<Sampl>;

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

pub type PageManager<Point, Sampl> = GenericPageManager<
    Loader<Sampl>,
    MetaTreeNodeId,
    SensorPosPage<Sampl, Point>,
    FileHandle<Sampl>,
    FileIdDirectory,
>;
