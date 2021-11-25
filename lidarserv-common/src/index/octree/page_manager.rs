use crate::geometry::bounding_box::OptionAABB;
use crate::geometry::grid::LeveledGridCell;
use crate::geometry::points::PointType;
use crate::geometry::position::{Component, CoordinateSystem, Position};
use crate::geometry::sampling::Sampling;
use crate::index::octree::grid_cell_directory::GridCellDirectory;
use crate::las::{Las, LasReadWrite, ReadLasError};
use crate::lru_cache::pager::{CacheLoadError, IoError, PageFileHandle, PageLoader, PageManager};
use nalgebra::Scalar;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

type SharedNodeResult<Sampl, Point, Comp, CSys> =
    Result<Arc<Node<Sampl, Point, Comp, CSys>>, ReadLasError>;

pub struct Page<Sampl, Point, Comp: Scalar, CSys> {
    binary: RwLock<Option<Arc<Vec<u8>>>>,
    node: RwLock<Option<SharedNodeResult<Sampl, Point, Comp, CSys>>>,
}

#[derive(Clone)]
pub struct Node<Sampl, Point, Comp: Scalar, CSys> {
    pub sampling: Sampl,
    pub bogus_points: Vec<Point>,
    pub bounding_box: OptionAABB<Comp>,
    pub coordinate_system: CSys,
}

impl<Sampl, Point, Comp, CSys> Default for Page<Sampl, Point, Comp, CSys>
where
    Point: PointType + Clone,
    Point::Position: Position<Component = Comp>,
    Comp: Component,
    Sampl: Sampling<Point = Point>,
    CSys: Clone + PartialEq,
{
    fn default() -> Self {
        Page::from_binary(vec![])
    }
}

impl<Sampl, Point, Comp, Pos, CSys> Page<Sampl, Point, Comp, CSys>
where
    Point: PointType<Position = Pos> + Clone,
    Pos: Position<Component = Comp>,
    Comp: Component,
    Sampl: Sampling<Point = Point>,
    CSys: Clone + PartialEq,
{
    pub fn from_binary(data: Vec<u8>) -> Self {
        Page {
            binary: RwLock::new(Some(Arc::new(data))),
            node: RwLock::new(None),
        }
    }

    pub fn from_node(node: Node<Sampl, Point, Comp, CSys>) -> Self {
        Page {
            binary: RwLock::new(None),
            node: RwLock::new(Some(Ok(Arc::new(node)))),
        }
    }

    pub fn get_binary<LasL>(&self, loader: &LasL) -> Arc<Vec<u8>>
    where
        LasL: LasReadWrite<Point, CSys>,
    {
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
        let mut data = Vec::new();
        {
            let write = Cursor::new(&mut data);
            loader
                .write_las(
                    Las {
                        points: points.iter(),
                        bounds: node.bounding_box.clone(),
                        non_bogus_points: Some(node.sampling.len() as u32),
                        coordinate_system: node.coordinate_system.clone(),
                    },
                    write,
                )
                // unwrap: write_las only fails on I/O errors. With a Cursor as a writer, however, every operation succeeds.
                .unwrap();
        }
        let arc = Arc::new(data);
        *write_lock = Some(Arc::clone(&arc));
        arc
    }

    pub fn get_node<LasL, F>(
        &self,
        loader: &LasL,
        make_sampling: F,
        coordinate_system: &CSys,
    ) -> Result<Arc<Node<Sampl, Point, Comp, CSys>>, ReadLasError>
    where
        LasL: LasReadWrite<Point, CSys>,
        F: FnOnce() -> Sampl,
    {
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
                coordinate_system: coordinate_system.clone(),
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

    pub fn set_node(&mut self, node: Node<Sampl, Point, Comp, CSys>) {
        self.binary = RwLock::new(None);
        self.node = RwLock::new(Some(Ok(Arc::new(node))));
    }
}

pub struct OctreePageLoader<LasL, Page> {
    loader: LasL,
    base_path: PathBuf,
    _phantom: PhantomData<fn(&Page) -> Page>,
}

pub struct OctreeFileHandle<LasL, Page> {
    file_name: PathBuf,
    loader: LasL,
    _phantom: PhantomData<fn(&Page) -> Page>,
}

impl<LasL, Page> OctreePageLoader<LasL, Page> {
    pub fn new(loader: LasL, base_path: PathBuf) -> Self {
        OctreePageLoader {
            loader,
            base_path,
            _phantom: PhantomData,
        }
    }
}

impl<LasL, Page> PageLoader for OctreePageLoader<LasL, Page>
where
    LasL: Clone,
    OctreeFileHandle<LasL, Page>: PageFileHandle,
{
    type FileName = LeveledGridCell;
    type FileHandle = OctreeFileHandle<LasL, Page>;

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

impl<LasL, CSys, Point, Sampl, Pos, Comp> PageFileHandle
    for OctreeFileHandle<LasL, Page<Sampl, Point, Comp, CSys>>
where
    LasL: LasReadWrite<Point, CSys>,
    CSys: CoordinateSystem<Position = Pos> + Clone + PartialEq,
    Point: PointType<Position = Pos> + Clone,
    Sampl: Sampling<Point = Point>,
    Pos: Position<Component = Comp>,
    Comp: Component,
{
    type Data = Page<Sampl, Point, Comp, CSys>;

    fn load(&mut self) -> Result<Self::Data, CacheLoadError> {
        let mut file = File::open(&self.file_name)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let page = Page::from_binary(data);
        Ok(page)
    }

    fn store(&mut self, page: &Self::Data) -> Result<(), IoError> {
        let data = page.get_binary(&self.loader);
        let mut file = File::create(&self.file_name)?;
        file.write_all(data.as_slice())?;
        file.sync_all()?;
        Ok(())
    }
}

pub type LasPageManager<LasL, Sampl, Point, Comp, CSys> = PageManager<
    OctreePageLoader<LasL, Page<Sampl, Point, Comp, CSys>>,
    LeveledGridCell,
    Page<Sampl, Point, Comp, CSys>,
    OctreeFileHandle<LasL, Page<Sampl, Point, Comp, CSys>>,
    GridCellDirectory,
>;
