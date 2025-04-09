use std::sync::{Arc, RwLock};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use pasture_core::{
    containers::{MakeBufferFromLayout, VectorBuffer},
    layout::PointLayout,
};
use tracy_client::span;

use crate::{
    geometry::{
        grid::{GridHierarchy, LodLevel},
        sampling::{Sampling, create_sampling, create_sampling_from_points},
    },
    io::{InMemoryPointCodec, PointIoError},
};

pub type Node = Arc<dyn Sampling + Send + Sync>;
pub type NodeData = Arc<[u8]>;

/// Lazy Octree Node.
///
/// The node can be in one of the representations
///  - raw binary data
///  - points buffer
///  - actual sampled octree node (type [Node])
///
/// After creation, the generation of the other
/// representations is lazy, i.e. only happens once accessed.
/// The results are memorized though.
pub struct LazyNode {
    binary: RwLock<Option<Result<NodeData, PointIoError>>>,
    node: RwLock<Option<Result<Node, PointIoError>>>,
}

impl LazyNode {
    /// Create a new page from binary representation.
    pub fn from_binary(data: Vec<u8>) -> Self {
        LazyNode {
            binary: RwLock::new(Some(Ok(data.into()))),
            node: RwLock::new(None),
        }
    }

    /// Create a new page from node struct.
    pub fn from_node(node: Node) -> Self {
        LazyNode {
            binary: RwLock::new(None),
            node: RwLock::new(Some(Ok(node))),
        }
    }

    fn node_to_binary(
        codec: &(impl InMemoryPointCodec + ?Sized),
        node: Node,
    ) -> Result<NodeData, PointIoError> {
        let mut data = Vec::<u8>::new();
        codec.write_points(node.points(), &mut data)?;
        data.write_u64::<LittleEndian>(node.nr_bogus_points() as u64)?;
        Ok(data.into())
    }

    fn binary_to_node(
        codec: &(impl InMemoryPointCodec + ?Sized),
        layout: &PointLayout,
        point_hierarchy: &GridHierarchy,
        lod: LodLevel,
        binary: NodeData,
    ) -> Result<Node, PointIoError> {
        // read points
        let (points, mut read) = codec.read_points(&binary, layout)?;

        // read nr of bogus points
        let nr_bogus_points = read.read_u64::<LittleEndian>()? as usize;

        // create sampling
        let node = create_sampling_from_points(*point_hierarchy, lod, points, nr_bogus_points);
        Ok(node.into())
    }

    /// Return the binary representation of the page.
    /// If not present, convert the node to binary.
    pub fn get_binary(
        &self,
        codec: &(impl InMemoryPointCodec + ?Sized),
    ) -> Result<NodeData, PointIoError> {
        let _span = span!("LazyNode::get_binary");

        // try getting existing
        {
            let read_lock = self.binary.read().unwrap();
            if let Some(result) = &*read_lock {
                _span.emit_text("existing");
                return result.clone();
            }
        }

        // if this is unsuccessful, make binary representation from node
        let node = {
            _span.emit_text("write_points");
            let read_lock = self.node.read().unwrap();
            // unwrap: if self.binary is None, self.node MUST be a Some.
            Arc::clone(read_lock.as_ref().unwrap().as_ref().unwrap())
        };
        let mut write_lock = self.binary.write().unwrap();
        let result = Self::node_to_binary(codec, node);
        *write_lock = Some(result.clone());
        result
    }

    /// Return the node struct of the page.
    /// If not present, parse the binary representation.
    pub fn get_node(
        &self,
        codec: &(impl InMemoryPointCodec + ?Sized),
        layout: &PointLayout,
        point_hierarchy: &GridHierarchy,
        lod: LodLevel,
    ) -> Result<Node, PointIoError> {
        let _span = span!("LazyNode::get_node");

        // try getting existing
        {
            let read_lock = self.node.read().unwrap();
            if let Some(result) = &*read_lock {
                _span.emit_text("existing");
                return result.clone();
            }
        }

        // if this is unsuccessful, parse the binary data to obtain the node
        let binary = {
            let read_lock = self.binary.read().unwrap();
            // unwrap: if self.binary is None, self.node MUST be a Some(Ok(_)).
            Arc::clone(read_lock.as_ref().unwrap().as_ref().unwrap())
        };
        let mut write_lock = self.node.write().unwrap();

        // empty node
        if binary.is_empty() {
            _span.emit_text("new_empty");
            let node: Node = create_sampling(*point_hierarchy, lod, layout).into();
            *write_lock = Some(Ok(Arc::clone(&node)));
            return Ok(node);
        }

        // read points
        _span.emit_text("read_points");
        let result = Self::binary_to_node(codec, layout, point_hierarchy, lod, binary);
        *write_lock = Some(result.clone());
        result
    }

    /// Return all points in the page.
    /// If node is not present, parse the binary representation.
    pub fn get_points(
        &self,
        codec: &(impl InMemoryPointCodec + ?Sized),
        layout: &PointLayout,
    ) -> Result<VectorBuffer, PointIoError> {
        let _span = span!("LazyNode::get_points");

        // try to get points from node
        {
            let read_lock = self.node.read().unwrap();
            if let Some(result) = &*read_lock {
                let result = result.clone();
                drop(read_lock);
                _span.emit_text("clone_from_node");
                return result.map(|n| n.clone_points());
            }
        }

        // else: parse las
        let data = {
            let read_lock = self.binary.read().unwrap();
            // unwrap: if self.node is None, then self.binary MUST be a Some.
            Arc::clone(read_lock.as_ref().unwrap().as_ref().unwrap())
        };
        if data.is_empty() {
            _span.emit_text("new_empty");
            Ok(VectorBuffer::new_from_layout(layout.clone()))
        } else {
            _span.emit_text("read_points");
            codec
                .read_points(&data, layout)
                .map(|(points, _rest)| points)
        }
    }

    pub fn set_binary_arc(&mut self, binary: NodeData) {
        self.node = RwLock::new(None);
        *self.binary.get_mut().unwrap() = Some(Ok(binary));
    }

    pub fn set_node_arc(&mut self, node: Node) {
        self.binary = RwLock::new(None);
        self.node = RwLock::new(Some(Ok(node)));
    }

    pub fn set_binary(&mut self, binary: Vec<u8>) {
        self.set_binary_arc(binary.into())
    }

    pub fn set_node(&mut self, node: Box<dyn Sampling + Send + Sync>) {
        self.set_node_arc(node.into())
    }
}

impl Default for LazyNode {
    fn default() -> Self {
        Self {
            binary: RwLock::new(Some(Ok(Arc::new([])))),
            node: RwLock::new(None),
        }
    }
}
