use crate::geometry::grid::LeveledGridCell;
use crate::geometry::sampling::Sampling;
use crate::index::octree::grid_cell_directory::GridCellDirectory;
use crate::las::{I32LasReadWrite, Las, LasPointAttributes, ReadLasError};
use crate::lru_cache::pager::{CacheLoadError, IoError, PageFileHandle, PageLoader, PageManager};
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracy_client::span;

type SharedNodeResult<Sampl, Point> = Result<Arc<Node<Sampl, Point>>, ReadLasError>;

pub struct OctreePageLoader<Page> {
    loader: I32LasReadWrite,
    base_path: PathBuf,
    _phantom: PhantomData<fn(&Page) -> Page>,
}

pub struct OctreeFileHandle<Page> {
    file_name: PathBuf,
    loader: I32LasReadWrite,
    _phantom: PhantomData<fn(&Page) -> Page>,
}

/// Thread safe octree Page in memory, either as binary or as node.
pub struct Page<Sampl, Point> {
    binary: RwLock<Option<Arc<Vec<u8>>>>,
    node: RwLock<Option<SharedNodeResult<Sampl, Point>>>,
}

#[derive(Clone)]
/// Octree Node representation.
/// Sampling contains the points.
pub struct Node<Sampl, Point> {
    pub sampling: Sampl,
    pub bogus_points: Vec<Point>,
    pub bounding_box: OptionAABB<i32>,
    pub coordinate_system: I32CoordinateSystem,
}

impl<Sampl, Point> Default for Page<Sampl, Point>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + Clone,
    Sampl: Sampling<Point = Point>,
{
    fn default() -> Self {
        Page::from_binary(vec![])
    }
}

impl<Sampl, Point> Page<Sampl, Point>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + Clone,
    Sampl: Sampling<Point = Point>,
{
    /// Create a new page from binary representation.
    pub fn from_binary(data: Vec<u8>) -> Self {
        Page {
            binary: RwLock::new(Some(Arc::new(data))),
            node: RwLock::new(None),
        }
    }

    /// Create a new page from node struct.
    pub fn from_node(node: Node<Sampl, Point>) -> Self {
        Page {
            binary: RwLock::new(None),
            node: RwLock::new(Some(Ok(Arc::new(node)))),
        }
    }

    /// Return the binary representation of the page.
    /// If not present, convert the node to binary.
    pub fn get_binary(&self, loader: &I32LasReadWrite) -> Arc<Vec<u8>> {
        // try getting existing
        {
            let read_lock = self.binary.read().unwrap();
            if let Some(arc) = &*read_lock {
                return Arc::clone(arc);
            }
        }

        // if this is unsuccessful, make binary representation from node
        let node = {
            let read_lock = self.node.read().unwrap();
            // double unwrap: if self.binary is None, self.node MUST be a Some(Ok(arc)).
            Arc::clone(read_lock.as_ref().unwrap().as_ref().unwrap())
        };
        let mut write_lock = self.binary.write().unwrap();
        let mut points = node.sampling.clone_points();
        points.append(&mut node.bogus_points.clone());
        let data = loader.write_las::<Point, _>(Las {
            points: points.iter(),
            bounds: node.bounding_box.clone(),
            non_bogus_points: Some(node.sampling.len() as u32),
            coordinate_system: node.coordinate_system,
        });
        let arc = Arc::new(data);
        *write_lock = Some(Arc::clone(&arc));
        arc
    }

    /// Return all points in the page.
    /// If node is not present, parse the binary representation.
    pub fn get_points(&self, loader: &I32LasReadWrite) -> Result<Vec<Point>, ReadLasError> {
        let _span = span!("Page::get_points");
        // try to get points from node
        {
            let read_lock = self.node.read().unwrap();
            if let Some(node) = &*read_lock {
                return node.as_ref().map_err(|e| e.clone()).map(|n| {
                    let mut points = n.sampling.clone_points();
                    points.append(&mut n.bogus_points.clone());
                    points
                });
            }
        }

        // else: parse las
        let read_lock = self.binary.read().unwrap();
        // unwrap: if self.node is None, then self.binary MUST be a Some.
        let cursor = Cursor::new(read_lock.as_ref().unwrap().as_slice());
        loader.read_las(cursor).map(|las| las.points)
    }

    /// Return the node struct of the page.
    /// If not present, parse the binary representation.
    pub fn get_node<F>(
        &self,
        loader: &I32LasReadWrite,
        make_sampling: F,
        coordinate_system: &I32CoordinateSystem,
    ) -> Result<Arc<Node<Sampl, Point>>, ReadLasError>
    where
        F: FnOnce() -> Sampl,
    {
        let _span = span!("Page::get_node");
        // try getting existing
        {
            let read_lock = self.node.read().unwrap();
            if let Some(result) = &*read_lock {
                return result
                    .as_ref()
                    .map_err(|e| e.to_owned())
                    .map_or_else(Err, |arc| {
                        if arc.coordinate_system == *coordinate_system {
                            Ok(Arc::clone(arc))
                        } else {
                            Err(ReadLasError::FileFormat {
                                desc: "Coordinate system mismatch".to_string(),
                            })
                        }
                    });
            }
        }

        // if this is unsuccessful, parse the binary data to obtain the node
        let binary = {
            let read_lock = self.binary.read().unwrap();
            // unwrap: if self.binary is None, self.node MUST be a Some.
            Arc::clone(read_lock.as_ref().unwrap())
        };
        if binary.len() == 0 {
            return Ok(Arc::new(Node {
                sampling: make_sampling(),
                bogus_points: vec![],
                bounding_box: OptionAABB::empty(),
                coordinate_system: *coordinate_system,
            }));
        }
        let mut write_lock = self.node.write().unwrap();
        let cursor = Cursor::new(binary.as_slice());
        let mut las_data = match loader.read_las(cursor) {
            Ok(v) => v,
            Err(e) => {
                *write_lock = Some(Err(e.clone()));
                return Err(e);
            }
        };
        let las_coordinate_system = las_data.coordinate_system;
        let bounding_box = las_data.bounds;
        let bogus_start_pos = las_data
            .non_bogus_points
            .map(|b| b as usize)
            .unwrap_or(las_data.points.len());
        let bogus_points = las_data.points.split_off(bogus_start_pos);
        let mut sampling = make_sampling();
        let rejected = sampling.insert(las_data.points, |_, _| ());
        assert_eq!(rejected.len(), 0);
        let node = Node {
            sampling,
            bogus_points,
            bounding_box,
            coordinate_system: las_coordinate_system,
        };
        let arc = Arc::new(node);
        *write_lock = Some(Ok(Arc::clone(&arc)));
        if arc.coordinate_system == *coordinate_system {
            Ok(arc)
        } else {
            Err(ReadLasError::FileFormat {
                desc: "Coordinate system mismatch".to_string(),
            })
        }
    }

    pub fn set_binary(&mut self, binary: Vec<u8>) {
        self.node = RwLock::new(None);
        *self.binary.get_mut().unwrap() = Some(Arc::new(binary));
    }

    pub fn set_node(&mut self, node: Node<Sampl, Point>) {
        self.binary = RwLock::new(None);
        self.node = RwLock::new(Some(Ok(Arc::new(node))));
    }
}

impl<Page> OctreePageLoader<Page> {
    pub fn new(loader: I32LasReadWrite, base_path: PathBuf) -> Self {
        OctreePageLoader {
            loader,
            base_path,
            _phantom: PhantomData,
        }
    }
}

impl<Page> PageLoader for OctreePageLoader<Page>
where
    OctreeFileHandle<Page>: PageFileHandle,
{
    type FileName = LeveledGridCell;
    type FileHandle = OctreeFileHandle<Page>;

    fn open(&self, file: &Self::FileName) -> Self::FileHandle {
        let mut path = self.base_path.clone();
        path.push(format!(
            "{}__{}-{}-{}.laz",
            file.lod.level(),
            file.pos.x,
            file.pos.y,
            file.pos.z,
        ));
        OctreeFileHandle {
            file_name: path,
            loader: self.loader.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<Point, Sampl> PageFileHandle for OctreeFileHandle<Page<Sampl, Point>>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + Clone,
    Sampl: Sampling<Point = Point>,
{
    type Data = Page<Sampl, Point>;

    fn load(&mut self) -> Result<Self::Data, CacheLoadError> {
        let _span = span!("PageFileHandle::load");
        let mut file = File::open(&self.file_name)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let page = Page::from_binary(data);
        Ok(page)
    }

    fn store(&mut self, page: &Self::Data) -> Result<(), IoError> {
        let _span = span!("PageFileHandle::store");
        let _span_1 = span!("PageFileHandle::store::get_binary");
        let data = page.get_binary(&self.loader);
        drop(_span_1);
        let _span_2 = span!("PageFileHandle::store::write_all");
        let mut file = File::create(&self.file_name)?;
        drop(_span_2);
        let _span_3 = span!("PageFileHandle::store::write_all");
        file.write_all(data.as_slice())?;
        drop(_span_3);
        let _span_4 = span!("PageFileHandle::store::sync_all");
        file.sync_all()?;
        drop(_span_4);
        Ok(())
    }
}

pub type LasPageManager<Sampl, Point> = PageManager<
    OctreePageLoader<Page<Sampl, Point>>,
    LeveledGridCell,
    Page<Sampl, Point>,
    OctreeFileHandle<Page<Sampl, Point>>,
    GridCellDirectory,
>;
