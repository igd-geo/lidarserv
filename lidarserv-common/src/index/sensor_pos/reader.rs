use crate::geometry::grid::GridHierarchy;
use crate::geometry::points::PointType;
use crate::geometry::position::{Component, Position};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId, Node};
use crate::index::sensor_pos::page_manager::BinDataPage;
use crate::index::sensor_pos::writer::IndexError;
use crate::index::sensor_pos::{Inner, Update};
use crate::index::Reader;
use crate::lru_cache::pager::CacheLoadError;
use crate::nalgebra::Scalar;
use crate::query::{Query, QueryExt};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct SensorPosReader<GridH, SamplF, Comp: Scalar, LasL, CSys, Pos> {
    query: Box<dyn Query<Pos>>,
    inner: Arc<Inner<GridH, SamplF, Comp, LasL, CSys>>,
    meta_tree: MetaTree<GridH, Comp>,
    updates: crossbeam_channel::Receiver<Update<Comp>>,
    loaded: HashSet<MetaTreeNodeId>,
    update_counter: u32,
    dirty_node_to_replacements: HashMap<MetaTreeNodeId, (u32, HashSet<MetaTreeNodeId>)>,
    replacement_to_dirty_node: HashMap<MetaTreeNodeId, MetaTreeNodeId>,
    update_order: Vec<(MetaTreeNodeId, u32)>,
    nodes_to_load: HashSet<MetaTreeNodeId>,
    load_order: Vec<MetaTreeNodeId>,
    nodes_to_unload: HashSet<MetaTreeNodeId>,
}

impl<GridH, SamplF, Comp, LasL, CSys, Point, Pos> Reader<Point>
    for SensorPosReader<GridH, SamplF, Comp, LasL, CSys, Pos>
where
    Point: PointType,
    Comp: Component,
    GridH: GridHierarchy<Component = Comp, Position = Pos> + Clone,
    Pos: Position<Component = Comp>,
{
    type NodeId = MetaTreeNodeId;
    type Node = Vec<Arc<BinDataPage>>;

    fn update(&mut self) {
        SensorPosReader::update(self);
    }

    fn load_one(&mut self) -> Option<(Self::NodeId, Self::Node)> {
        self.sort_load_order();
        if let Some(node_id) = self.load_order.pop() {
            self.nodes_to_load.remove(&node_id);
            let load_result = self.load_node(&node_id).unwrap_or_else(|_| Vec::new());
            self.loaded.insert(node_id.clone());
            Some((node_id, load_result))
        } else {
            None
        }
    }

    fn remove_one(&mut self) -> Option<Self::NodeId> {
        let node_to_remove = self.nodes_to_unload.iter().next().cloned();
        if let Some(node_id) = &node_to_remove {
            self.nodes_to_unload.remove(node_id);
            self.loaded.remove(node_id);
            if let Some((_, replacements)) = self.dirty_node_to_replacements.remove(node_id) {
                for r in replacements {
                    self.replacement_to_dirty_node.remove(&r);
                }
                self.reset_update_order()
            }
        }
        node_to_remove
    }

    fn update_one(&mut self) -> Option<(Self::NodeId, Vec<(Self::NodeId, Self::Node)>)> {
        self.sort_update_order();
        if let Some((node_id, _)) = self.update_order.pop() {
            // load nodes
            let nodes_to_load = self.query_from(vec![node_id.clone()]);
            let loaded: Vec<_> = nodes_to_load
                .iter()
                .map(|node_id| {
                    (
                        node_id.clone(),
                        self.load_node(node_id).unwrap_or_else(|_| Vec::new()),
                    )
                })
                .collect();

            // update data structure
            self.loaded.remove(&node_id);
            self.loaded.extend(nodes_to_load.iter().cloned());
            let (_, replacements) = self.dirty_node_to_replacements.remove(&node_id).unwrap();
            for replacement in replacements {
                self.replacement_to_dirty_node.remove(&replacement);
            }
            self.nodes_to_unload.remove(&node_id);

            Some((node_id, loaded))
        } else {
            None
        }
    }
}

impl<GridH, SamplF, Comp, LasL, CSys, Pos> SensorPosReader<GridH, SamplF, Comp, LasL, CSys, Pos>
where
    Comp: Component,
    GridH: GridHierarchy<Component = Comp, Position = Pos> + Clone,
    Pos: Position<Component = Comp>,
{
    pub(super) fn new<Q>(query: Q, inner: Arc<Inner<GridH, SamplF, Comp, LasL, CSys>>) -> Self
    where
        Q: Query<Pos> + 'static,
    {
        // subscribe to meta tree updates
        let (updates_sender, updates_receiver) = crossbeam_channel::unbounded();
        let meta_tree = {
            let mut write = inner.shared.write().unwrap();
            write.readers.push(updates_sender);
            write.meta_tree.clone()
        };

        let mut result = SensorPosReader {
            query: Box::new(query),
            inner,
            meta_tree,
            updates: updates_receiver,
            loaded: HashSet::new(),
            replacement_to_dirty_node: HashMap::new(),
            dirty_node_to_replacements: HashMap::new(),
            update_counter: 0,
            nodes_to_load: HashSet::new(),
            load_order: Vec::new(),
            nodes_to_unload: HashSet::new(),
            update_order: Vec::new(),
        };
        result.nodes_to_load = result.initial_query();
        result
    }

    fn query_from(&self, start_nodes: Vec<MetaTreeNodeId>) -> HashSet<MetaTreeNodeId> {
        let mut result_nodes = HashSet::new();

        let mut todo: Vec<_> = start_nodes;
        while let Some(node_id) = todo.pop() {
            // get node
            let node = match self.meta_tree.get(&node_id) {
                None => continue,
                Some(n) => n,
            };

            // test against query
            let matches = self
                .query
                .as_ref()
                .matches_node(&node.bounds, node_id.lod());

            if matches {
                if node.is_leaf {
                    // if we found a matching leaf node: Add it to the result
                    result_nodes.insert(node_id);
                } else {
                    // if we found a matching branch node, recurse into children until we reach a leaf node
                    todo.extend(node_id.children());
                }
            }
        }

        result_nodes
    }

    fn initial_query(&self) -> HashSet<MetaTreeNodeId> {
        let root_nodes = self.meta_tree.root_nodes().collect();
        self.query_from(root_nodes)
    }

    fn update_query<Q>(&mut self, new_query: Q)
    where
        Q: Query<Pos> + 'static,
    {
        // execute query to get list of target nodes to load
        self.query = Box::new(new_query);
        let target_nodes = self.initial_query();

        // calculate difference between currently loaded nodes and new nodes
        self.nodes_to_load = HashSet::new();
        self.nodes_to_unload = self.loaded.clone();
        for node in target_nodes {
            if self.loaded.contains(&node) {
                self.nodes_to_unload.remove(&node);
            } else if let Some(base_node) = self.replacement_to_dirty_node.get(&node) {
                self.nodes_to_unload.remove(base_node);
            } else {
                self.nodes_to_load.insert(node);
            }
        }

        self.reset_load_order();
    }

    fn update(&mut self) {
        // get pending updates
        let updates: Vec<_> = self.updates.try_iter().collect();

        // todo dedup updates

        self.update_counter += 1;

        for update in updates {
            // update meta tree
            self.meta_tree.apply_update(&update);

            let is_node_split =
                update.replaced_by.len() != 1 || update.replaced_by[0].replace_with != update.node;

            if self.nodes_to_load.contains(&update.node) {
                // if the update is to be loaded:
                // load the replacement instead.
                // (if this is not a node split, this is a no-op)
                if is_node_split {
                    self.nodes_to_load.remove(&update.node);
                    for replacement in update.replaced_by {
                        let matches = self
                            .query
                            .as_ref()
                            .matches_node(&replacement.bounds, replacement.replace_with.lod());
                        if matches {
                            self.nodes_to_load.insert(replacement.replace_with);
                        }
                    }
                    self.reset_load_order()
                }
            } else if let Some(base_node) = self.replacement_to_dirty_node.get(&update.node) {
                // if the update is already dirty:
                // update the replacement info
                // (also a no-op for non-node-splits)
                if is_node_split {
                    let (_, replacements) =
                        self.dirty_node_to_replacements.get_mut(base_node).unwrap();
                    replacements.remove(&update.node);
                    for replacement in &update.replaced_by {
                        replacements.insert(replacement.replace_with.clone());
                    }
                    let base_node = base_node.clone();
                    self.replacement_to_dirty_node.remove(&update.node);
                    for replacement in &update.replaced_by {
                        self.replacement_to_dirty_node
                            .insert(replacement.replace_with.clone(), base_node.clone());
                    }
                }
            } else if self.loaded.contains(&update.node) {
                // if the node is not yet marked as dirty, but loaded
                // initially mark as dirty
                self.dirty_node_to_replacements.insert(
                    update.node.clone(),
                    (
                        self.update_counter,
                        update
                            .replaced_by
                            .iter()
                            .map(|r| r.replace_with.clone())
                            .collect(),
                    ),
                );
                for replacement in update.replaced_by {
                    self.replacement_to_dirty_node
                        .insert(replacement.replace_with, update.node.clone());
                }
                self.reset_update_order()
            } else {
                // if the node is neither loaded, nor scheduled to be loaded
                // check if it still does not match the query and eventually add it to the load list
                for replacement in update.replaced_by {
                    let matches = self
                        .query
                        .as_ref()
                        .matches_node(&replacement.bounds, replacement.replace_with.lod());
                    if matches {
                        self.nodes_to_load.insert(replacement.replace_with);
                    }
                }
                self.reset_load_order();
            }
        }
    }

    fn reset_load_order(&mut self) {
        self.load_order.clear();
    }

    fn reset_update_order(&mut self) {
        self.update_order.clear();
    }

    fn sort_load_order(&mut self) {
        // only re-sort, if the load order has been cleared
        if !self.load_order.is_empty() {
            return;
        }

        // sort nodes_to_load by lod
        // so that coarse lod levels are loaded first
        self.load_order.extend(self.nodes_to_load.iter().cloned());
        self.load_order
            .sort_by_key(|node| u16::MAX - node.lod().level());
    }

    fn sort_update_order(&mut self) {
        // only re-sort, if the update order has been cleared
        if !self.update_order.is_empty() {
            return;
        }

        // insert pending updates
        self.update_order.extend(
            self.dirty_node_to_replacements
                .iter()
                .map(|(node_id, (update_number, _))| (node_id.clone(), *update_number)),
        );

        // sort by update number
        // so that the oldest updates are processed first
        self.update_order.sort_by_key(|(node_id, update_number)| {
            (u32::MAX - *update_number, u16::MAX - node_id.lod().level())
        });
    }

    fn load_node(&self, node_id: &MetaTreeNodeId) -> Result<Vec<Arc<BinDataPage>>, CacheLoadError> {
        let mut loaded = Vec::with_capacity(self.inner.nr_threads);
        let mut files_to_load: Vec<_> = (0..self.inner.nr_threads)
            .map(|thread_id| node_id.file(thread_id))
            .collect();
        while let Some(file_id) = files_to_load.pop() {
            if let Some(file) = self.inner.page_manager.load(&file_id)? {
                if file.exists {
                    loaded.push(file);
                } else {
                    files_to_load.extend(file_id.children().into_iter())
                }
            }
        }
        Ok(loaded)
    }
}
