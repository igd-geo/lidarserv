use crate::geometry::grid::LeveledGridCell;
use crate::io::{InMemoryPointCodec, PointIoError};
use crate::lru_cache::pager::PageLoader;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tracy_client::span;

use super::lazy_node::LazyNode;

/// Loader for the Lru Cache
/// that loads pages from/to disk
pub struct OctreeLoader {
    pub base_path: PathBuf,
    pub codec: Arc<dyn InMemoryPointCodec + Send + Sync>,
}

impl OctreeLoader {
    pub fn new(base_path: PathBuf, codec: Arc<dyn InMemoryPointCodec + Send + Sync>) -> Self {
        OctreeLoader { base_path, codec }
    }

    fn file_name(&self, key: &LeveledGridCell) -> PathBuf {
        let mut path = self.base_path.clone();
        path.push(format!(
            "{}__{}-{}-{}.bin",
            key.lod.level(),
            key.pos.x,
            key.pos.y,
            key.pos.z,
        ));
        path
    }
}

impl PageLoader for OctreeLoader {
    type Key = LeveledGridCell;
    type Data = LazyNode;
    type Error = PointIoError;

    fn load(&self, key: &Self::Key) -> Result<Self::Data, Self::Error> {
        let _span = span!("PageFileHandle::load");
        let _span2 = span!("PageFileHandle::load - read file");
        let file_name = self.file_name(key);
        let mut file = File::open(file_name)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        drop(_span2);
        let page = LazyNode::from_binary(data);
        Ok(page)
    }

    fn store(&self, key: &Self::Key, data: &Self::Data) -> Result<(), Self::Error> {
        let _span = span!("PageFileHandle::store");
        let data = data.get_binary(&*self.codec)?;
        {
            let _span_2 = span!("PageFileHandle::store - write file");
            let file_name = self.file_name(key);
            let mut file = File::create(file_name)?;
            file.write_all(&data)?;
            file.sync_all()?;
        }
        Ok(())
    }
}
