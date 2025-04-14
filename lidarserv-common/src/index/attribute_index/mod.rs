use config::IndexKind;
use nalgebra::{Vector3, Vector4};
use pasture_core::{
    containers::{BorrowedBufferExt, VectorBuffer},
    layout::{PointAttributeDataType, PointAttributeDefinition, PrimitiveType},
};
use range_index::RangeIndex;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sfc_index::SfcIndex;
use std::{
    collections::{HashMap, hash_map::Entry},
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
    sync::{
        Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{
    geometry::grid::LeveledGridCell,
    query::{
        NodeQueryResult,
        attribute::{AttributeQuery, TestFunction, TestFunctionDyn},
    },
};

pub mod boolvec;
pub mod cmp;
pub mod config;
pub mod range_index;
pub mod sfc_index;

pub trait IndexFunction {
    type AttributeValue;
    type NodeType;

    /// Creates a node "containing" the given attribute values.
    fn index(&self, attribute_values: impl Iterator<Item = Self::AttributeValue>)
    -> Self::NodeType;

    /// Merges two nodes into one.
    /// The resulting node should "contain" all attribute values of both input nodes.
    fn merge(&self, node1: &mut Self::NodeType, node2: Self::NodeType);

    /// Tests, if points in the node are equal to the given operand.
    fn test_eq(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult;

    /// Tests, if points in the node are unequal to the given operand.
    ///
    /// For attributes that are non-scalar (color, point normals):
    /// ALL components have to be unequal to the operand.
    ///
    /// This is unintuitive at first, but required for "complete" query possibilities.
    /// Note, that for non-scalar attributes test_neq is different from not(test_eq).
    /// The latter is probably what you intuitively would want in most case.
    fn test_neq(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult;

    /// Tests, if points in the node are smaller than the given operand.
    ///
    /// For non-scalar attributes, ALL components need to be smaller than the operand components in order for a point to match.
    /// Note that this is different from not(test_greater_eq), which would match if ANY of the components is smaller than a point.
    fn test_less(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult;

    /// Tests, if points in the node are smaller or equal to the given operand.
    ///
    /// For non-scalar attributes, ALL components need to be smaller or equal to the operand components.
    /// Note that this is different from not(test_greater), which would match if ANY of the components are smaller or equal.
    fn test_less_eq(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult;

    /// Tests, if points in the node are larger than the given operand.
    ///
    /// For non-scalar attributes, ALL components need to be larger than their corresponding operand components.
    /// Note that this is different from not(test_less_eq), which would match if ANY of the components is larger.
    fn test_greater(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult;

    /// Tests, if points in the node are larger or equal to the given operand.
    ///
    /// For non-scalar attributes, ALL components need to be larger or equal to the operand components.
    /// Note that this is different from not(test_less), which would match if ANY of the components are larger or equal.
    fn test_greater_eq(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult;

    /// Tests, if points in the node are within the given range. (inclusive version)
    ///
    /// For non-scalar attributes, ALL components need to be in the given range.
    #[inline]
    fn test_range_inclusive(
        &self,
        node: &Self::NodeType,
        op1: &Self::AttributeValue,
        op2: &Self::AttributeValue,
    ) -> NodeQueryResult {
        self.test_greater_eq(node, op1)
            .and(self.test_less_eq(node, op2))
    }

    /// Tests, if points in the node are within the given range. (left-inclusive, right-exclusive version)
    ///
    /// For non-scalar attributes, ALL components need to be in the given range.
    #[inline]
    fn test_range_left_inclusive(
        &self,
        node: &Self::NodeType,
        op1: &Self::AttributeValue,
        op2: &Self::AttributeValue,
    ) -> NodeQueryResult {
        self.test_greater_eq(node, op1)
            .and(self.test_less(node, op2))
    }

    /// Tests, if points in the node are within the given range. (left-exclusive, right-inclusive inclusive version)
    ///
    /// For non-scalar attributes, ALL components need to be in the given range.
    #[inline]
    fn test_range_right_inclusive(
        &self,
        node: &Self::NodeType,
        op1: &Self::AttributeValue,
        op2: &Self::AttributeValue,
    ) -> NodeQueryResult {
        self.test_greater(node, op1)
            .and(self.test_less_eq(node, op2))
    }

    /// Tests, if points in the node are within the given range. (exclusive version)
    ///
    /// For non-scalar attributes, ALL components need to be in the given range.
    #[inline]
    fn test_range_exclusive(
        &self,
        node: &Self::NodeType,
        op1: &Self::AttributeValue,
        op2: &Self::AttributeValue,
    ) -> NodeQueryResult {
        self.test_greater(node, op1).and(self.test_less(node, op2))
    }
}

struct NodeManager<Idx, Node> {
    index: Idx,
    nodes: RwLock<HashMap<LeveledGridCell, Mutex<Node>>>,
    dirty: AtomicBool,
    path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum AttributeIndexError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("The node file {0} is corrupt.")]
    Corrupt(PathBuf),

    #[error("The index config is invalid: {0}")]
    Config(String),
}

#[derive(Serialize, Deserialize)]
struct NodesFile<Node> {
    nodes: Vec<(LeveledGridCell, Node)>,
}

impl<Idx, Node> NodeManager<Idx, Node>
where
    Idx: IndexFunction<NodeType = Node>,
    Node: DeserializeOwned,
{
    pub fn create(index: Idx, path: PathBuf) -> Self {
        NodeManager {
            index,
            nodes: RwLock::new(HashMap::new()),
            dirty: AtomicBool::new(true),
            path,
        }
    }

    pub fn load(index: Idx, path: PathBuf) -> Result<Self, AttributeIndexError> {
        let mut read = BufReader::new(File::open(&path)?);
        let content: NodesFile<Node> = match ciborium::from_reader(&mut read) {
            Ok(o) => o,
            Err(ciborium::de::Error::Io(io_err)) => return Err(AttributeIndexError::Io(io_err)),
            Err(_) => return Err(AttributeIndexError::Corrupt(path)),
        };
        let nodes = content
            .nodes
            .into_iter()
            .map(|(cell, node)| (cell, Mutex::new(node)))
            .collect::<HashMap<_, _>>();

        Ok(NodeManager {
            index,
            nodes: RwLock::new(nodes),
            dirty: AtomicBool::new(false),
            path,
        })
    }

    pub fn load_or_create(index: Idx, path: PathBuf) -> Result<Self, AttributeIndexError> {
        if path.exists() {
            Self::load(index, path)
        } else {
            Ok(Self::create(index, path))
        }
    }
}

trait NodeManagerDyn {
    fn flush(&self) -> Result<(), AttributeIndexError>;
    fn index(
        &self,
        cell: LeveledGridCell,
        points: &VectorBuffer,
        attribute: &PointAttributeDefinition,
    );
    fn test(&self, cell: &LeveledGridCell, test: &TestFunctionDyn) -> NodeQueryResult;
}

impl<Idx, Node> NodeManagerDyn for NodeManager<Idx, Node>
where
    Idx: IndexFunction<NodeType = Node, AttributeValue: PrimitiveType>,
    Node: Serialize + Clone,
{
    fn index(
        &self,
        cell: LeveledGridCell,
        points: &VectorBuffer,
        attribute: &PointAttributeDefinition,
    ) {
        assert_eq!(attribute.datatype(), Idx::AttributeValue::data_type());

        // accumulate points
        let iter = points
            .view_attribute::<Idx::AttributeValue>(attribute)
            .into_iter();
        let accumulated_points = self.index.index(iter);

        // try updating existing node.
        {
            let nodes = self.nodes.read().unwrap();
            if let Some(node) = nodes.get(&cell) {
                let mut node_lock = node.lock().unwrap();
                self.index.merge(&mut node_lock, accumulated_points);
                self.dirty.store(true, Ordering::Release);
                return;
            }
        }

        // insert new node
        {
            let mut nodes = self.nodes.write().unwrap();
            match nodes.entry(cell) {
                Entry::Occupied(mut o) => {
                    let existing_node = o.get_mut().get_mut().unwrap();
                    self.index.merge(existing_node, accumulated_points);
                }
                Entry::Vacant(e) => {
                    e.insert(Mutex::new(accumulated_points));
                }
            }
        }
        self.dirty.store(true, Ordering::Release);
    }

    fn test(&self, cell: &LeveledGridCell, test: &TestFunctionDyn) -> NodeQueryResult {
        assert_eq!(*test.datatype(), Idx::AttributeValue::data_type());
        let test = test.convert_to::<Idx::AttributeValue>();

        let nodes = self.nodes.read().unwrap();
        match nodes.get(cell) {
            Some(node) => {
                let node_lock = node.lock().unwrap();

                match test {
                    TestFunction::Eq(o) => self.index.test_eq(&node_lock, o),
                    TestFunction::Neq(o) => self.index.test_neq(&node_lock, o),
                    TestFunction::Less(o) => self.index.test_less(&node_lock, o),
                    TestFunction::LessEq(o) => self.index.test_less_eq(&node_lock, o),
                    TestFunction::Greater(o) => self.index.test_greater(&node_lock, o),
                    TestFunction::GreaterEq(o) => self.index.test_greater_eq(&node_lock, o),
                    TestFunction::RangeExclusive(o, p) => {
                        self.index.test_range_exclusive(&node_lock, o, p)
                    }
                    TestFunction::RangeLeftInclusive(o, p) => {
                        self.index.test_range_left_inclusive(&node_lock, o, p)
                    }
                    TestFunction::RangeRightInclusive(o, p) => {
                        self.index.test_range_right_inclusive(&node_lock, o, p)
                    }
                    TestFunction::RangeAllInclusive(o, p) => {
                        self.index.test_range_inclusive(&node_lock, o, p)
                    }
                }
            }
            _ => NodeQueryResult::Negative,
        }
    }

    fn flush(&self) -> Result<(), AttributeIndexError> {
        let dirty = self.dirty.swap(false, Ordering::AcqRel);
        if !dirty {
            return Ok(());
        }
        let contents = {
            let lock = self.nodes.read().unwrap();
            NodesFile {
                nodes: lock
                    .iter()
                    .map(|(cell, node)| (*cell, node.lock().unwrap().clone()))
                    .collect(),
            }
        };
        let write = File::create(&self.path)?;
        let mut write = BufWriter::new(write);
        match ciborium::into_writer(&contents, &mut write) {
            Ok(_) => (),
            Err(ciborium::ser::Error::Io(io_err)) => return Err(AttributeIndexError::Io(io_err)),
            Err(_) => return Err(AttributeIndexError::Corrupt(self.path.clone())),
        };
        write.into_inner().map_err(|e| e.into_error())?.sync_all()?;
        Ok(())
    }
}

pub struct AttributeIndex {
    by_attribute: HashMap<PointAttributeDefinition, Vec<Box<dyn NodeManagerDyn + Send + Sync>>>,
}

impl AttributeIndex {
    pub fn new() -> Self {
        AttributeIndex {
            by_attribute: HashMap::new(),
        }
    }

    pub fn add_index<Idx>(
        &mut self,
        attribute: PointAttributeDefinition,
        index: Idx,
        path: PathBuf,
    ) -> Result<(), AttributeIndexError>
    where
        Idx: IndexFunction<
                NodeType: Serialize + DeserializeOwned + Send + Clone,
                AttributeValue: PrimitiveType,
            > + Send
            + Sync
            + 'static,
    {
        assert_eq!(Idx::AttributeValue::data_type(), attribute.datatype());
        let by_attr = self.by_attribute.entry(attribute).or_default();
        by_attr.push(Box::new(NodeManager::load_or_create(index, path)?));
        Ok(())
    }

    pub fn add_index_from_config(
        &mut self,
        attribute: PointAttributeDefinition,
        index: &IndexKind,
        path: PathBuf,
    ) -> Result<(), AttributeIndexError> {
        macro_rules! add_index_common_attr_types {
            ($attribute:expr_2021, $path:expr_2021, $makeidx:expr_2021) => {{
                let attribute = $attribute;
                let path = $path;
                match attribute.datatype() {
                    PointAttributeDataType::U8 => {
                        type T = u8;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::I8 => {
                        type T = i8;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::U16 => {
                        type T = u16;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::I16 => {
                        type T = i16;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::U32 => {
                        type T = u32;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::I32 => {
                        type T = i32;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::U64 => {
                        type T = u64;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::I64 => {
                        type T = i64;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::F32 => {
                        type T = f32;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::F64 => {
                        type T = f64;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::Vec3u8 => {
                        type T = Vector3<u8>;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::Vec3u16 => {
                        type T = Vector3<u16>;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::Vec3f32 => {
                        type T = Vector3<f32>;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::Vec3i32 => {
                        type T = Vector3<i32>;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::Vec3f64 => {
                        type T = Vector3<f64>;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::Vec4u8 => {
                        type T = Vector4<u8>;
                        self.add_index(attribute, $makeidx, path)
                    }
                    PointAttributeDataType::ByteArray(_) => Err(AttributeIndexError::Config(
                        "Byte arrays are not supported.".to_string(),
                    )),
                    PointAttributeDataType::Custom { .. } => Err(AttributeIndexError::Config(
                        "Custom datatype is not supported.".to_string(),
                    )),
                }
            }};
        }

        match index {
            IndexKind::RangeIndex => {
                add_index_common_attr_types!(attribute, path, RangeIndex::<T>::default())
            }
            IndexKind::SfcIndex(sfc_options) => {
                if sfc_options.nr_bins < 1 {
                    Err(AttributeIndexError::Config(
                        "SfcIndex must have at least one bin.".to_string(),
                    ))
                } else {
                    add_index_common_attr_types!(
                        attribute,
                        path,
                        SfcIndex::<T>::new(sfc_options.nr_bins)
                    )
                }
            }
        }
    }

    pub fn index(&self, cell: LeveledGridCell, points: &VectorBuffer) {
        for (attribute, by_attr) in &self.by_attribute {
            for index in by_attr {
                index.index(cell, points, attribute);
            }
        }
    }

    pub fn test<T: PrimitiveType>(
        &self,
        cell: &LeveledGridCell,
        query: &AttributeQuery<T>,
    ) -> NodeQueryResult {
        if let Some(by_attr) = self.by_attribute.get(&query.attribute) {
            let test = TestFunctionDyn::new(&query.test);
            for index in by_attr {
                let result = index.test(cell, &test);
                if result != NodeQueryResult::Partial {
                    return result;
                }
            }
        }
        NodeQueryResult::Partial
    }

    pub fn flush(&self) -> Result<(), AttributeIndexError> {
        for by_attr in self.by_attribute.values() {
            for index in by_attr {
                index.flush()?
            }
        }
        Ok(())
    }
}

impl Default for AttributeIndex {
    fn default() -> Self {
        Self::new()
    }
}
