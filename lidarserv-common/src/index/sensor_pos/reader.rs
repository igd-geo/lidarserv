use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
use crate::geometry::grid::LodLevel;
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::I32Position;
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index;
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::index::sensor_pos::page_manager::{SensorPosPage, SimplePoints};
use crate::index::sensor_pos::point::SensorPositionAttribute;
use crate::index::sensor_pos::{Inner, Update};
use crate::index::{Node, NodeId, Reader};
use crate::las::{LasExtraBytes, LasPointAttributes};
use crate::lru_cache::pager::CacheLoadError;
use crate::query::empty::EmptyQuery;
use crate::query::{Query, QueryExt};
use crossbeam_channel::{Receiver, TryRecvError};
use nalgebra::min;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct SensorPosReader<SamplF, Point, Sampl> {
    query: Arc<dyn Query + Send + Sync>,
    inner: Arc<Inner<SamplF, Point, Sampl>>,
    meta_tree: MetaTree,
    updates: crossbeam_channel::Receiver<Update>,
    lods: Vec<LodReader<SamplF, Point, Sampl>>,
    current_loading_lod: usize,
    update_counter: u32,
}

struct LodReader<SamplF, Point, Sampl> {
    query: Arc<dyn Query + Send + Sync>,
    inner: Arc<Inner<SamplF, Point, Sampl>>,
    lod: LodLevel,
    loaded: HashMap<MetaTreeNodeId, SensorPosNodeCollection<Sampl, Point>>,
    dirty_node_to_replacements: HashMap<MetaTreeNodeId, (u32, HashSet<MetaTreeNodeId>)>,
    replacement_to_dirty_node: HashMap<MetaTreeNodeId, MetaTreeNodeId>,
    update_order: Vec<(MetaTreeNodeId, u32)>,
    nodes_to_load: HashSet<MetaTreeNodeId>,
    nodes_to_unload: HashSet<MetaTreeNodeId>,
}

pub struct SensorPosNodeCollection<Sampl, Point> {
    /// actual nodes as sent from the cache
    nodes: Vec<Arc<SensorPosPage<Sampl, Point>>>,

    /// binary data for the clients
    /// (so we don't have to store the las loader, etc in here.)
    binary: Vec<Arc<Vec<u8>>>,
}

impl<SamplF, Point, Sampl> SensorPosReader<SamplF, Point, Sampl>
where
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + WithAttr<LasPointAttributes>
        + LasExtraBytes
        + Clone,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
    Sampl: Sampling<Point = Point>,
{
    pub(super) fn new<Q>(query: Q, inner: Arc<Inner<SamplF, Point, Sampl>>) -> Self
    where
        Q: Query + 'static + Send + Sync,
    {
        // subscribe to meta tree updates
        let (updates_sender, updates) = crossbeam_channel::unbounded();
        let meta_tree = {
            let mut write = inner.shared.write().unwrap();
            write.readers.push(updates_sender);
            write.meta_tree.clone()
        };

        // create reader
        let query: Arc<dyn Query + Send + Sync> = Arc::new(query);
        let mut result = SensorPosReader {
            query,
            inner,
            meta_tree,
            updates,
            lods: vec![],
            current_loading_lod: 0,
            update_counter: 0,
        };

        // create lod level readers
        for lod_level in 0..=result.inner.max_lod.level() {
            result.lods.push(LodReader::new(
                LodLevel::from_level(lod_level),
                Arc::clone(&result.inner),
            ));
        }

        // set query, so that we start to load the query result in the first lod
        result.set_query_arc(Arc::clone(&result.query));

        result
    }

    #[allow(clippy::type_complexity)] // only internal anyways
    fn lod_layer_and_coarse(
        lods: &mut Vec<LodReader<SamplF, Point, Sampl>>,
        coarse_lod_steps: usize,
        lod_index: usize,
    ) -> (
        &mut LodReader<SamplF, Point, Sampl>,
        Option<&mut LodReader<SamplF, Point, Sampl>>,
    ) {
        let coarse_lod_level = if coarse_lod_steps <= lod_index {
            lod_index - coarse_lod_steps
        } else if lod_index > 0 {
            0
        } else {
            return (&mut lods[lod_index], None);
        };
        let (first, second) = lods.split_at_mut(lod_index as usize);
        (&mut second[0], Some(&mut first[coarse_lod_level]))
    }

    fn update_coarse(&mut self, updated_node: &MetaTreeNodeId) {
        let influenced_lod_index =
            updated_node.lod().level() as usize + self.inner.coarse_lod_steps;
        if *updated_node.lod() == LodLevel::base() {
            let iter_to = min(influenced_lod_index, self.current_loading_lod);
            for index in 1..=iter_to {
                let (fine, coarse) =
                    Self::lod_layer_and_coarse(&mut self.lods, self.inner.coarse_lod_steps, index);
                assert_eq!(coarse.as_ref().unwrap().lod, *updated_node.lod());
                fine.refresh_coarse(updated_node, coarse, &self.meta_tree)
            }
        } else if influenced_lod_index <= self.current_loading_lod {
            let (fine, coarse) = Self::lod_layer_and_coarse(
                &mut self.lods,
                self.inner.coarse_lod_steps,
                influenced_lod_index,
            );
            assert_eq!(coarse.as_ref().unwrap().lod, *updated_node.lod());
            fine.refresh_coarse(updated_node, coarse, &self.meta_tree)
        }
    }

    fn set_query_arc(&mut self, query: Arc<dyn Query + Send + Sync>) {
        self.query = query;
        self.current_loading_lod = 0;
        if !self.lods.is_empty() {
            self.lods[0].set_query(Arc::clone(&self.query), &self.meta_tree, None);
        }
        self.advance_current_loading_lod();
    }

    fn advance_current_loading_lod(&mut self) {
        // load the next lod, if the current one is done.
        while self.lods[self.current_loading_lod].is_fully_loaded()
            && self.current_loading_lod + 1 < self.lods.len()
        {
            self.current_loading_lod += 1;
            let (next_loading_lod, coarse) = Self::lod_layer_and_coarse(
                &mut self.lods,
                self.inner.coarse_lod_steps,
                self.current_loading_lod,
            );
            next_loading_lod.set_query(Arc::clone(&self.query), &self.meta_tree, coarse);
        }
    }
}

impl<SamplF, Point, Sampl> Reader<Point> for SensorPosReader<SamplF, Point, Sampl>
where
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + WithAttr<LasPointAttributes>
        + LasExtraBytes
        + Clone,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
    Sampl: Sampling<Point = Point>,
{
    type NodeId = MetaTreeNodeId;
    type Node = SensorPosNodeCollection<Sampl, Point>;

    fn set_query<Q: Query + 'static + Send + Sync>(&mut self, query: Q) {
        let query: Arc<dyn Query + Send + Sync> = Arc::new(query);
        self.set_query_arc(query)
    }

    fn update(&mut self) {
        // get pending updates
        let updates: Vec<_> = self.updates.try_iter().collect();

        // deduplicate updates
        let mut merged_updates = Vec::new();
        let mut replacement_position = HashMap::new();
        for update in updates {
            let index = if let Some(index) = replacement_position.get(&update.node) {
                let base_update: &mut Update = &mut merged_updates[*index];
                let mut i = 0;
                while i < base_update.replaced_by.len() {
                    if base_update.replaced_by[i].replace_with == update.node {
                        base_update.replaced_by.swap_remove(i);
                    } else {
                        i += 1;
                    }
                }
                base_update
                    .replaced_by
                    .append(&mut update.replaced_by.clone());
                *index
            } else {
                let index = merged_updates.len();
                merged_updates.push(update.clone());
                index
            };
            for replacement in update.replaced_by {
                replacement_position.insert(replacement.replace_with, index);
            }
        }

        // process each update
        for update in merged_updates {
            // update meta tree
            self.meta_tree.apply_update(&update);

            let lod_index = update.node.lod().level() as usize;
            if lod_index < self.lods.len() {
                self.update_counter += 1;
                if lod_index <= self.current_loading_lod {
                    let (lod_layer, coarse_layer) = Self::lod_layer_and_coarse(
                        &mut self.lods,
                        self.inner.coarse_lod_steps,
                        lod_index,
                    );
                    lod_layer.apply_update(
                        &update,
                        &self.meta_tree,
                        coarse_layer,
                        self.update_counter,
                    )
                } else {
                    self.lods[lod_index].apply_update_noquery(&update, self.update_counter);
                }
            }
        }
    }

    fn blocking_update(&mut self, queries: &mut Receiver<Box<dyn Query + Send + Sync>>) -> bool {
        loop {
            // make sure we have the most recent query
            let mut query = None;
            loop {
                match queries.try_recv() {
                    Ok(q) => query = Some(q),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        // if the channel is disconnected: return.
                        // but still process the last query we received
                        if query.is_some() {
                            break;
                        } else {
                            return false;
                        }
                    }
                }
            }
            if let Some(new_query) = query {
                self.set_query_arc(Arc::from(new_query));
            }

            // make sure we have the most recent updates from the writer
            self.update();

            // if there are things to do (load_one, remove_one, update_one) return early.
            for (lod_index, lod) in self.lods.iter().enumerate() {
                if lod_index <= self.current_loading_lod {
                    if lod.is_dirty() {
                        return true;
                    }
                } else if lod.has_outdated_nodes() {
                    return true;
                }
            }

            // if there is nothing to do:
            // wait for something to happen (either a new query, or an update to come in).
            let mut sel = crossbeam_channel::Select::new();
            sel.recv(&self.updates);
            sel.recv(queries);
            sel.ready();
        }
    }

    fn load_one(&mut self) -> Option<(Self::NodeId, Self::Node)> {
        // load
        // start loading at lod0 and progressively get finer.
        let mut result = None;
        for lod_index in 0..=self.current_loading_lod {
            if let Some(load) = self.lods[lod_index].load_one() {
                result = Some(load);
                break;
            }
        }

        // load the next lod, if the current one is done.
        self.advance_current_loading_lod();

        // if this lod is a coarse layer for another lod
        // notify that finer lod of the update so it can check, if the update influenced the query
        // result.
        if let Some((loaded_node, _)) = &result {
            self.update_coarse(loaded_node);
        }
        result
    }

    fn remove_one(&mut self) -> Option<Self::NodeId> {
        for lod_index in (0..=self.current_loading_lod).rev() {
            if let Some(remove) = self.lods[lod_index].remove_one() {
                return Some(remove);
            }
        }
        None
    }

    fn update_one(&mut self) -> Option<index::Update<Self::NodeId, Self::Node>> {
        // choose the lod level t update, that "has to offer" the oldest update
        // so that updates that have been pending for the longest time are prioritized
        let lod_index = self
            .lods
            .iter_mut()
            .map(|lod_reader| lod_reader.next_update_number())
            .enumerate()
            .min_by_key(|(_, update_number)| update_number.unwrap_or(u32::MAX))
            .map(|(index, _)| index);

        // update!
        let option_update = if let Some(lod_index) = lod_index {
            let (lod_layer, coarse) =
                Self::lod_layer_and_coarse(&mut self.lods, self.inner.coarse_lod_steps, lod_index);
            lod_layer.update_one(coarse, &self.meta_tree)
        } else {
            None
        };

        // if this lod is a coarse layer for another lod
        // notify that finer lod of the update so it can check, if the update influenced the query
        // result of the finer lod.
        if let Some((updated_node, _)) = &option_update {
            self.update_coarse(updated_node);
        }
        option_update
    }
}

impl<SamplF, Point, Sampl> LodReader<SamplF, Point, Sampl>
where
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + WithAttr<LasPointAttributes>
        + LasExtraBytes
        + Clone,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
    Sampl: Sampling<Point = Point>,
{
    pub fn new(lod: LodLevel, inner: Arc<Inner<SamplF, Point, Sampl>>) -> Self {
        LodReader {
            lod,
            inner,
            query: Arc::new(EmptyQuery::new()),
            loaded: HashMap::new(),
            dirty_node_to_replacements: HashMap::new(),
            replacement_to_dirty_node: HashMap::new(),
            update_order: Vec::new(),
            nodes_to_load: HashSet::new(),
            nodes_to_unload: HashSet::new(),
        }
    }

    pub fn set_query(
        &mut self,
        new_query: Arc<dyn Query + Send + Sync>,
        meta_tree: &MetaTree,
        coarse_level: Option<&mut Self>,
    ) {
        // execute query to get list of target nodes to load
        self.query = new_query;
        let root_nodes = meta_tree.root_nodes_for_lod(&self.lod).collect();
        let target_nodes = self.evaluate_query(root_nodes, meta_tree, coarse_level);

        // calculate difference between currently loaded nodes and new nodes
        self.nodes_to_load = HashSet::new();
        self.nodes_to_unload = self.loaded.keys().cloned().collect();
        for node in target_nodes {
            if self.loaded.contains_key(&node) {
                self.nodes_to_unload.remove(&node);
            } else if let Some(base_node) = self.replacement_to_dirty_node.get(&node) {
                self.nodes_to_unload.remove(base_node);
            } else {
                self.nodes_to_load.insert(node);
            }
        }
    }

    pub fn load_one(&mut self) -> Option<(MetaTreeNodeId, SensorPosNodeCollection<Sampl, Point>)> {
        if let Some(node_id) = self.nodes_to_load.iter().cloned().next() {
            self.nodes_to_load.remove(&node_id);
            let load_result = self.load_node(&node_id).unwrap_or_default();
            self.loaded.insert(node_id.clone(), load_result.clone());
            Some((node_id, load_result))
        } else {
            None
        }
    }

    pub fn remove_one(&mut self) -> Option<MetaTreeNodeId> {
        let node_to_remove = self.nodes_to_unload.iter().next().cloned();
        if let Some(node_id) = &node_to_remove {
            self.nodes_to_unload.remove(node_id);
            self.loaded.remove(node_id);
            if let Some((_, replacements)) = self.dirty_node_to_replacements.remove(node_id) {
                for r in replacements {
                    self.replacement_to_dirty_node.remove(&r);
                }
                self.update_order.clear();
            }
        }
        node_to_remove
    }

    pub fn apply_update_noquery(&mut self, update: &Update, update_counter: u32) {
        let is_node_split =
            update.replaced_by.len() != 1 || update.replaced_by[0].replace_with != update.node;

        if let Some(base_node) = self.replacement_to_dirty_node.get(&update.node) {
            // if the update is already dirty:
            // update the replacement info
            if is_node_split {
                let (_, replacements) = self.dirty_node_to_replacements.get_mut(base_node).unwrap();
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
        } else if self.loaded.contains_key(&update.node) {
            // if the node is not yet marked as dirty, but loaded -> mark as dirty
            self.dirty_node_to_replacements.insert(
                update.node.clone(),
                (
                    update_counter,
                    update
                        .replaced_by
                        .iter()
                        .map(|r| r.replace_with.clone())
                        .collect(),
                ),
            );
            for replacement in &update.replaced_by {
                self.replacement_to_dirty_node
                    .insert(replacement.replace_with.clone(), update.node.clone());
            }

            // reset update order, so it gets re-calculated next time update_one() is called.
            self.update_order.clear();
        }
    }

    pub fn apply_update(
        &mut self,
        update: &Update,
        meta_tree: &MetaTree,
        coarse_level: Option<&mut Self>,
        update_counter: u32,
    ) {
        let is_node_split =
            update.replaced_by.len() != 1 || update.replaced_by[0].replace_with != update.node;

        if self.nodes_to_load.contains(&update.node) {
            // if the update is to be loaded:
            // load the replacement instead.
            // (if this is not a node split, this is a no-op)
            if is_node_split {
                self.nodes_to_load.remove(&update.node);

                let matching_replacements =
                    self.evaluate_query(vec![update.node.clone()], meta_tree, coarse_level);
                self.nodes_to_load.extend(matching_replacements);
            }
        } else if let Some(base_node) = self.replacement_to_dirty_node.get(&update.node) {
            // if the update is already dirty:

            // if the node was scheduled to unload, but it now matches the query, then we should not unload it any more
            if self.nodes_to_unload.contains(base_node)
                && self.matches_node(base_node, meta_tree, coarse_level)
            {
                self.nodes_to_unload.remove(base_node);
            }

            // update the replacement info
            if is_node_split {
                let (_, replacements) = self.dirty_node_to_replacements.get_mut(base_node).unwrap();
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
        } else if self.loaded.contains_key(&update.node) {
            // if the node is not yet marked as dirty, but loaded.

            // if the node was scheduled to unload, but it now matches the query, then we should not unload it any more
            if self.nodes_to_unload.contains(&update.node)
                && self.matches_node(&update.node, meta_tree, coarse_level)
            {
                self.nodes_to_unload.remove(&update.node);
            }

            // mark as dirty
            self.dirty_node_to_replacements.insert(
                update.node.clone(),
                (
                    update_counter,
                    update
                        .replaced_by
                        .iter()
                        .map(|r| r.replace_with.clone())
                        .collect(),
                ),
            );
            for replacement in &update.replaced_by {
                self.replacement_to_dirty_node
                    .insert(replacement.replace_with.clone(), update.node.clone());
            }

            // reset update order, so it gets re-calculated next time update_one() is called.
            self.update_order.clear();
        } else {
            // if the node is neither loaded, nor scheduled to be loaded
            // check if it still does not match the query and eventually add it to the load list
            let matching_replacements =
                self.evaluate_query(vec![update.node.clone()], meta_tree, coarse_level);
            self.nodes_to_load.extend(matching_replacements);
        }
    }

    pub fn next_update_number(&mut self) -> Option<u32> {
        self.ensure_update_order();
        self.update_order.last().map(|(_, i)| *i)
    }

    pub fn update_one(
        &mut self,
        coarse: Option<&mut Self>,
        meta_tree: &MetaTree,
    ) -> Option<index::Update<MetaTreeNodeId, SensorPosNodeCollection<Sampl, Point>>> {
        self.ensure_update_order();

        if let Some((node_id, _)) = self.update_order.pop() {
            // load nodes
            let nodes_to_load = self.evaluate_query(vec![node_id.clone()], meta_tree, coarse);
            let loaded: Vec<_> = nodes_to_load
                .iter()
                .map(|node_id| (node_id.clone(), self.load_node(node_id).unwrap_or_default()))
                .collect();

            // update data structure
            self.loaded.remove(&node_id);
            self.loaded.extend(loaded.iter().cloned());
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

    pub fn is_fully_loaded(&self) -> bool {
        self.nodes_to_load.is_empty()
    }

    pub fn has_outdated_nodes(&self) -> bool {
        !self.dirty_node_to_replacements.is_empty()
    }

    pub fn is_dirty(&self) -> bool {
        if !self.nodes_to_load.is_empty() {
            return true;
        }
        if !self.nodes_to_unload.is_empty() {
            return true;
        }
        if !self.dirty_node_to_replacements.is_empty() {
            return true;
        }
        false
    }

    fn refresh_coarse(
        &mut self,
        coarse_node: &MetaTreeNodeId,
        coarse: Option<&mut Self>,
        meta_tree: &MetaTree,
    ) {
        // get the equivalent to the coarse node on this lod.
        let mut query_root = coarse_node.clone().with_lod(self.lod);
        while meta_tree.get(&query_root).is_none() {
            query_root = if let Some(p) = query_root.parent() {
                p
            } else {
                // no node in this lod overlaps with the given node from the coarse layer.
                // in this case we have nothing to do.
                return;
            }
        }

        // re-evaluate query for that node
        let target_nodes = self.evaluate_query(vec![query_root], meta_tree, coarse);

        // ensure that all nodes in the query result are indeed loaded, or at least on the list of nodes to load later
        for node in target_nodes {
            if let Some(loaded_node) = self.replacement_to_dirty_node.get(&node) {
                self.nodes_to_unload.remove(loaded_node);
            } else if self.loaded.contains_key(&node) {
                self.nodes_to_unload.remove(&node);
            } else {
                self.nodes_to_load.insert(node);
            }
        }
    }

    fn ensure_update_order(&mut self) {
        if self.update_order.is_empty() {
            let updates = self
                .dirty_node_to_replacements
                .iter()
                .map(|(node_id, (update_number, _))| (node_id.clone(), *update_number));
            self.update_order.extend(updates);
            self.update_order.sort_by_key(|(_, i)| u32::MAX - *i)
        }
    }

    fn evaluate_query_using_aabbs(
        &self,
        start_from_nodes: Vec<MetaTreeNodeId>,
        meta_tree: &MetaTree,
    ) -> Vec<MetaTreeNodeId> {
        // start with the given start nodes and an empty result
        let mut todo = start_from_nodes;
        let mut result_nodes = Vec::new();

        // keep processing nodes, until all nodes are processed.
        while let Some(node_id) = todo.pop() {
            // get node
            let node = match meta_tree.get(&node_id) {
                None => continue,
                Some(n) => n,
            };

            // test against query
            let matches = self.query.as_ref().matches_node(
                &node.bounds,
                &self.inner.coordinate_system,
                node_id.lod(),
            );

            if matches {
                if node.is_leaf {
                    // if we found a matching leaf node: Add it to the result
                    result_nodes.push(node_id);
                } else {
                    // if we found a matching branch node, recurse into children until we reach a leaf node
                    todo.extend(node_id.children());
                }
            }
        }
        result_nodes
    }

    fn coarse_check(&mut self, meta_tree: &MetaTree, node: &MetaTreeNodeId) -> bool {
        // search in the meta tree for all leaf nodes on this lod,
        // that are overlapping with the given node
        let mut overlapping_nodes = Vec::new();
        let mut to_search: Vec<_> = meta_tree.root_nodes_for_lod(&self.lod).collect();
        while let Some(current_node_id) = to_search.pop() {
            if current_node_id.tree_node().overlaps_with(node.tree_node()) {
                if let Some(current_node) = meta_tree.get(&current_node_id) {
                    if current_node.is_leaf {
                        overlapping_nodes.push(current_node_id);
                    } else {
                        to_search.extend(current_node_id.children().into_iter());
                    }
                }
            }
        }

        // map to the nodes that are loaded.
        // (the loaded nodes are not necessarily leaf nodes - only if all node splits are fully applied)
        // (also, some might just not be loaded, because they don't match the query.)
        // (also, we are collect()ing into a HashSet, to get rid of duplicates)
        let loaded_overlapping_nodes: HashSet<_> = overlapping_nodes
            .into_iter()
            .filter_map(|overlapping_leaf| {
                if let Some(loaded) = self.replacement_to_dirty_node.get(&overlapping_leaf) {
                    Some(loaded.clone())
                } else if self.loaded.contains_key(&overlapping_leaf) {
                    Some(overlapping_leaf)
                } else {
                    None
                }
            })
            .collect();

        // parse the las data of these nodes
        let points = loaded_overlapping_nodes.into_iter().flat_map(|node_id| {
            let node = &self.loaded[&node_id];
            let me = &*self;
            node.nodes.iter().map(move |page| {
                page.get_points(&me.inner.las_loader).unwrap_or_else(|_| {
                    Arc::new(SimplePoints {
                        points: vec![],
                        bounds: OptionAABB::empty(),
                        non_bogus_points: 0,
                    })
                })
            })
        });

        // filter points based on sensor pos
        // to get only points belonging to the node
        let sensor_pos_bounds = meta_tree
            .sensor_grid_hierarchy()
            .get_leveled_cell_bounds(node.tree_node());
        let node_bounds = meta_tree.get(node).unwrap().bounds.clone();
        let points = points.flat_map(|f| {
            f.points
                .iter()
                .filter(|p| {
                    node_bounds.contains(p.position())
                        && sensor_pos_bounds.contains(&p.attribute::<SensorPositionAttribute>().0)
                })
                .cloned()
                .collect::<Vec<_>>()
        });

        // calculate the max lod for each point,
        // and check, that at least one point justifies the lod level from the node
        let sampl = self.inner.sampling_factory.build(&self.lod);
        points
            .filter_map(|p| {
                let position = p.position();
                let bounds = sampl.cell_aabb(position);
                self.query
                    .max_lod_area(&bounds, &self.inner.coordinate_system)
                //.max_lod_position(position, &self.inner.coordinate_system)
            })
            .any(|max_lod| *node.lod() <= max_lod)
    }

    fn filter_nodes_using_coarse<'a>(
        &self,
        nodes: Vec<MetaTreeNodeId>,
        coarse: &'a mut Self,
        meta_tree: &'a MetaTree,
    ) -> impl Iterator<Item = MetaTreeNodeId> + 'a {
        nodes
            .into_iter()
            .filter(|node_id| coarse.coarse_check(meta_tree, node_id))
    }

    fn evaluate_query(
        &self,
        start_from_nodes: Vec<MetaTreeNodeId>,
        meta_tree: &MetaTree,
        coarse: Option<&mut Self>,
    ) -> Vec<MetaTreeNodeId> {
        let aabb_result = self.evaluate_query_using_aabbs(start_from_nodes, meta_tree);
        if let Some(coarse_level) = coarse {
            self.filter_nodes_using_coarse(aabb_result, coarse_level, meta_tree)
                .collect()
        } else {
            aabb_result
        }
    }

    fn matches_node(
        &self,
        node: &MetaTreeNodeId,
        meta_tree: &MetaTree,
        coarse: Option<&mut Self>,
    ) -> bool {
        let bounds = meta_tree
            .sensor_grid_hierarchy()
            .get_leveled_cell_bounds(node.tree_node());
        let matches_aabb =
            self.query
                .matches_node(&bounds, &self.inner.coordinate_system, node.lod());
        if let Some(coarse_level) = coarse {
            coarse_level.coarse_check(meta_tree, node)
        } else {
            matches_aabb
        }
    }

    fn load_node(
        &self,
        node_id: &MetaTreeNodeId,
    ) -> Result<SensorPosNodeCollection<Sampl, Point>, CacheLoadError> {
        let mut loaded = Vec::with_capacity(self.inner.nr_threads);
        let mut files_to_load = vec![node_id.clone()];
        while let Some(file_id) = files_to_load.pop() {
            if let Some(file) = self.inner.page_manager.load(&file_id)? {
                if file.exists() {
                    loaded.push(file);
                } else {
                    files_to_load.extend(file_id.children().into_iter())
                }
            }
            self.inner.page_manager.cleanup_one_no_write();
        }
        Ok(SensorPosNodeCollection {
            binary: loaded
                .iter()
                .map(|it| {
                    it.get_binary(&self.inner.las_loader, self.inner.coordinate_system.clone())
                })
                .collect(),
            nodes: loaded,
        })
    }
}

impl<Sampl, Point> Node for SensorPosNodeCollection<Sampl, Point> {
    fn las_files(&self) -> Vec<Arc<Vec<u8>>> {
        self.binary.clone()
    }
}
impl<Sampl, Point> Clone for SensorPosNodeCollection<Sampl, Point> {
    fn clone(&self) -> Self {
        SensorPosNodeCollection {
            nodes: self.nodes.clone(),
            binary: self.binary.clone(),
        }
    }
}

impl<Sampl, Point> Default for SensorPosNodeCollection<Sampl, Point> {
    fn default() -> Self {
        SensorPosNodeCollection {
            nodes: vec![],
            binary: vec![],
        }
    }
}

impl NodeId for MetaTreeNodeId {
    fn lod(&self) -> LodLevel {
        *MetaTreeNodeId::lod(self)
    }
}
