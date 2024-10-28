use pasture_core::{
    containers::{BorrowedBufferExt, VectorBuffer},
    layout::{PointAttributeDefinition, PrimitiveType},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::{hash_map::Entry, HashMap},
    fs::File,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, RwLock,
    },
};

use crate::{
    geometry::grid::LeveledGridCell,
    query::{
        attribute::{AttributeQuery, TestFunction, TestFunctionDyn},
        NodeQueryResult,
    },
};

pub mod range_index;

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
        let read = File::open(&path)?;
        let content: NodesFile<Node> = match ciborium::from_reader(&read) {
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
        if let Some(node) = nodes.get(cell) {
            let node_lock = node.lock().unwrap();

            match test {
                TestFunction::Eq(o) => self.index.test_eq(&node_lock, o),
                TestFunction::Neq(o) => self.index.test_neq(&node_lock, o),
                TestFunction::Less(o) => self.index.test_less(&node_lock, o),
                TestFunction::LessEq(o) => self.index.test_less_eq(&node_lock, o),
                TestFunction::Greater(o) => self.index.test_greater(&node_lock, o),
                TestFunction::GreaterEq(o) => self.index.test_greater_eq(&node_lock, o),
                TestFunction::RangeExclusive(o, p) => self
                    .index
                    .test_greater(&node_lock, o)
                    .and(self.index.test_less(&node_lock, p)),
                TestFunction::RangeLeftInclusive(o, p) => self
                    .index
                    .test_greater_eq(&node_lock, o)
                    .and(self.index.test_less(&node_lock, p)),
                TestFunction::RangeRightInclusive(o, p) => self
                    .index
                    .test_greater(&node_lock, o)
                    .and(self.index.test_less_eq(&node_lock, p)),
                TestFunction::RangeAllInclusive(o, p) => self
                    .index
                    .test_greater_eq(&node_lock, o)
                    .and(self.index.test_less_eq(&node_lock, p)),
            }
        } else {
            NodeQueryResult::Negative
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
        match ciborium::into_writer(&contents, &write) {
            Ok(_) => (),
            Err(ciborium::ser::Error::Io(io_err)) => return Err(AttributeIndexError::Io(io_err)),
            Err(_) => return Err(AttributeIndexError::Corrupt(self.path.clone())),
        };
        write.sync_all()?;
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
