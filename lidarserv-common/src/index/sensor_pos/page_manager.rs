use crate::geometry::grid::{GridCell, LodLevel};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::lru_cache::pager::{
    CacheLoadError, IoError, PageDirectory, PageFileHandle, PageLoader,
    PageManager as GenericPageManager,
};
use crate::nalgebra::Scalar;
use crate::span;
use std::collections::HashSet;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicU8;

#[derive(Default, Debug)]
pub struct BinDataPage {
    pub exists: bool,
    pub data: Vec<u8>,
    pub error_counter: AtomicU8,
}

pub struct BinDataLoader {
    base_path: PathBuf,
    extension: String,
}

pub struct BinDataFileHandle {
    file_name: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct FileId {
    pub lod: LodLevel,
    pub tree_depth: LodLevel,
    pub grid_cell: GridCell,
    pub thread_index: usize,
}

impl BinDataLoader {
    pub fn new(base_path: PathBuf, extension: String) -> Self {
        BinDataLoader {
            base_path,
            extension,
        }
    }
}

impl PageLoader for BinDataLoader {
    type FileName = FileId;
    type FileHandle = BinDataFileHandle;

    fn open(&self, file: &Self::FileName) -> Self::FileHandle {
        let filename = format!(
            "{}__{}__{}-{}-{}__{}.{}",
            file.lod.level(),
            file.tree_depth.level(),
            file.grid_cell.x,
            file.grid_cell.y,
            file.grid_cell.z,
            file.thread_index,
            self.extension
        );
        let mut path = self.base_path.clone();
        path.push(filename);
        BinDataFileHandle { file_name: path }
    }
}

impl PageFileHandle for BinDataFileHandle {
    type Data = BinDataPage;

    fn load(&mut self) -> Result<Self::Data, CacheLoadError> {
        let mut file = match File::open(&self.file_name) {
            Ok(f) => f,
            Err(e) => {
                return if e.kind() == ErrorKind::NotFound {
                    Ok(BinDataPage {
                        exists: false,
                        data: Vec::new(),
                        error_counter: AtomicU8::new(0),
                    })
                } else {
                    Err(CacheLoadError::IO { source: e.into() })
                }
            }
        };
        let mut result = BinDataPage {
            exists: true,
            data: vec![],
            error_counter: AtomicU8::new(0),
        };
        file.read_to_end(&mut result.data)?;
        Ok(result)
    }

    fn store(&mut self, data: &Self::Data) -> Result<(), IoError> {
        if data.exists {
            let s = span!("BinDataFileHandle::store: write and sync");
            s.emit_value(data.data.len() as u64);
            let mut file = File::create(&self.file_name)?;
            file.write_all(&data.data)?;
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

pub struct FileIdDirectory {
    files: HashSet<FileId>,
}

impl FileIdDirectory {
    pub fn new() -> Self {
        FileIdDirectory {
            files: HashSet::new(),
        }
    }

    pub fn from_meta_tree<GridH, Comp: Scalar>(
        meta_tree: &MetaTree<GridH, Comp>,
        num_threads: usize,
    ) -> Self {
        let mut directory = FileIdDirectory::new();
        for node in meta_tree.nodes() {
            for thread_id in 0..num_threads {
                let file_id = node.file(thread_id);
                directory.files.insert(file_id);
            }
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
    type Key = FileId;

    fn insert(&mut self, key: &Self::Key) {
        self.files.insert(key.clone());
    }

    fn exists(&self, key: &Self::Key) -> bool {
        self.files.contains(key)
    }
}

impl FileId {
    pub fn children(&self) -> [FileId; 8] {
        let node = MetaTreeNodeId::from_file(self);
        let children = node.children();
        children.map(|child| child.file(self.thread_index))
    }
}

pub type PageManager =
    GenericPageManager<BinDataLoader, FileId, BinDataPage, BinDataFileHandle, FileIdDirectory>;
