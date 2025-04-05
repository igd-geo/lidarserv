use super::Inner;
use crate::{
    geometry::{
        grid::{LeveledGridCell, LodLevel},
        position::PositionComponentType,
    },
    index::attribute_index::AttributeIndex,
    lru_cache::pager::PageDirectory,
    query::{ExecutableQuery, LoadKind, NodeQueryResult, Query, QueryContext},
};
use log::{debug, trace};
use pasture_core::containers::{BorrowedBuffer, InterleavedBuffer, OwningBuffer, VectorBuffer};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tracy_client::span;

pub struct OctreeReader {
    inner: Arc<Inner>,
    query_context: QueryContext,
    query: Box<dyn ExecutableQuery>,
    point_filtering: bool,
    frontier: HashMap<LeveledGridCell, FrontierElement>,
    known_root_nodes: HashSet<LeveledGridCell>,
    changed_nodes_receiver: crossbeam_channel::Receiver<LeveledGridCell>,
    generation: u64,
    loaded: HashMap<LeveledGridCell, LoadKind>,
    load_queue: HashMap<LeveledGridCell, LoadKind>,
    remove_queue: HashSet<LeveledGridCell>,
    reload_queue: HashMap<LeveledGridCell, (u64, LoadKind)>,
}

#[derive(Debug)]
struct FrontierElement {
    matches_query: NodeQueryResult,
    exists: bool,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct QueryConfig {
    pub enable_attribute_index: bool,
    pub enable_point_filtering: bool,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            enable_attribute_index: true,
            enable_point_filtering: true,
        }
    }
}

impl OctreeReader {
    /// Creates a new reader for the given octree and query.
    /// All root nodes of the octree are added to the reader.
    pub(super) fn new<Q: Query>(inner: Arc<Inner>, query: Q) -> Result<Self, Q::Error> {
        // add subscription to changes
        let changed_nodes_receiver = {
            let (changed_nodes_sender, changed_nodes_receiver) = crossbeam_channel::unbounded();
            let mut lock = inner.subscriptions.lock().unwrap();
            lock.push(changed_nodes_sender);
            changed_nodes_receiver
        };
        let root_nodes = inner.page_cache.directory().get_root_cells();

        let ctx = QueryContext {
            node_hierarchy: inner.node_hierarchy,
            point_hierarchy: inner.point_hierarchy,
            coordinate_system: inner.coordinate_system,
            component_type: PositionComponentType::from_layout(&inner.point_layout),
            attribute_index: Arc::clone(&inner.attribute_index),
            point_layout: inner.point_layout.clone(),
        };
        let query = Box::new(query.prepare(&ctx)?);

        let mut reader = OctreeReader {
            inner,
            query,
            query_context: ctx,
            frontier: HashMap::default(),
            changed_nodes_receiver,
            remove_queue: HashSet::new(),
            known_root_nodes: HashSet::new(),
            load_queue: HashMap::new(),
            reload_queue: HashMap::new(),
            loaded: HashMap::new(),
            generation: 0,
            point_filtering: true,
        };
        for root_node in root_nodes {
            reader.add_root(root_node);
        }
        Ok(reader)
    }

    /// Adds a new root node to the readers frontier and list of known root nodes.
    /// If the node matches the query, it is also added to the load queue.
    fn add_root(&mut self, cell: LeveledGridCell) {
        let matches_query = self.query.matches_node(cell);
        self.frontier.insert(
            cell,
            FrontierElement {
                matches_query,
                exists: true,
            },
        );
        if let Some(load_kind) = matches_query.should_load(self.point_filtering) {
            self.load_queue.insert(cell, load_kind);
        }
        self.known_root_nodes.insert(cell);
    }

    /// Filters out all points of the given Vector, that do not match the query or filter
    /// returns vector of points that match the query and filter.
    fn filter_points(&self, lod: LodLevel, points: &VectorBuffer) -> VectorBuffer {
        let _s1 = span!("OctreeReader::filter_points");
        let bitmap = self.query.matches_points(lod, points);
        assert!(bitmap.len() == points.len());
        let mut filtered_points =
            VectorBuffer::with_capacity(points.len(), points.point_layout().clone());

        for (i, matches) in bitmap.into_iter().enumerate() {
            if matches {
                // safety: identical layout
                unsafe { filtered_points.push_points(points.get_point_ref(i)) };
            }
        }
        filtered_points
    }

    /// Processes a set of changed nodes.
    /// This includes:
    /// - Add already loaded nodes to the reload queue.
    /// - Add new nodes to the frontier and load queue, when they match the query.
    /// - Add new root nodes
    ///
    /// # Arguments
    /// * `changes` - The set of changed nodes.
    fn process_changes(&mut self, mut changes: HashSet<LeveledGridCell>) {
        let _span = span!("OctreeReader::process_changes");

        // get remaining changes from the channel, if any
        while let Ok(update) = self.changed_nodes_receiver.try_recv() {
            changes.insert(update);
        }

        // schedule all changed nodes, that are already loaded for a reload.
        let reload: Vec<_> = changes
            .iter()
            .copied()
            .filter(|it| !self.reload_queue.contains_key(it))
            .filter_map(|it| self.loaded.get(&it).map(|load_kind| (it, *load_kind)))
            .collect();
        if !reload.is_empty() {
            self.generation += 1;
            for (reload_cell, kind) in reload {
                self.reload_queue
                    .insert(reload_cell, (self.generation, kind));
            }
        }

        // Update the frontier.
        // Any elements that now both exist and match the query get scheduled for their initial load.
        for change in &changes {
            if let Some(elem) = self.frontier.get_mut(change) {
                let should_load_before = elem.exists
                    && elem
                        .matches_query
                        .should_load(self.point_filtering)
                        .is_some();
                elem.exists = true;
                elem.matches_query = self.query.matches_node(*change);
                if !should_load_before {
                    if let Some(load_kind) = elem.matches_query.should_load(self.point_filtering) {
                        self.load_queue.insert(*change, load_kind);
                    }
                }
            }
        }

        // add all new root nodes
        let new_roots: Vec<_> = changes
            .into_iter()
            .filter(|it| it.lod == LodLevel::base())
            .filter(|it| !self.known_root_nodes.contains(it))
            .collect();
        for new_root_cell in new_roots {
            self.add_root(new_root_cell)
        }
    }

    /// Updates the frontier, load and remove queue after new query or filter.
    fn update_new_query(&mut self) {
        // update frontier
        for (cell, elem) in &mut self.frontier {
            elem.matches_query = self.query.matches_node(*cell)
        }

        // update load queue from frontier
        self.load_queue = self
            .frontier
            .iter()
            .filter(|(_, it)| it.exists)
            .filter_map(|(cell, it)| {
                it.matches_query
                    .should_load(self.point_filtering)
                    .map(|load_kind| (*cell, load_kind))
            })
            .collect();

        // search for removable nodes
        let mut removable_cnt = HashMap::new();
        for (cell, elem) in &self.frontier {
            if elem.matches_query == NodeQueryResult::Negative {
                if let Some(parent) = cell.parent() {
                    removable_cnt
                        .entry(parent)
                        .and_modify(|cnt| *cnt += 1)
                        .or_insert(1);
                }
            }
        }

        // remove nodes that are not needed anymore --> add to remove queue
        self.remove_queue = removable_cnt
            .into_iter()
            .filter_map(|(cell, cnt)| if cnt == 8 { Some(cell) } else { None })
            .filter(|cell| self.query.matches_node(*cell) == NodeQueryResult::Negative)
            .collect();

        // reload nodes that are filtered
        self.generation += 1;
        for (loaded, old_kind) in &self.loaded {
            let matches_node = self.query.matches_node(*loaded);
            if let Some(new_kind) = matches_node.should_load(self.point_filtering) {
                if let Some((_, reload_kind)) = self.reload_queue.get_mut(loaded) {
                    *reload_kind = new_kind
                } else if *old_kind == LoadKind::Filter || new_kind == LoadKind::Filter {
                    self.reload_queue
                        .insert(*loaded, (self.generation, new_kind));
                }
            }
        }
    }

    /// Checks the index for any changed or new nodes and schedules corresponding
    /// (re)loads to keep the query result up-to-date.
    /// (In case the query is running at the same time as the indexer)
    /// (Call this regularily!)
    pub fn update(&mut self) {
        let changes = HashSet::new();
        self.process_changes(changes);
    }

    /// Like [Self::update], but blocks until at least one update is received.
    pub fn wait_update(&mut self) {
        let _span = span!("OctreeReader waiting...");
        let mut changes = HashSet::new();
        if let Ok(update) = self.changed_nodes_receiver.recv() {
            changes.insert(update);
        }
        drop(_span);
        self.process_changes(changes);
    }

    /// Like [Self::update], but blocks until at least one update is received, or the `other` channel receives something.
    pub fn wait_update_or<T>(
        &mut self,
        other: &crossbeam_channel::Receiver<T>,
    ) -> Option<Result<T, crossbeam_channel::RecvError>> {
        let _span = span!("OctreeReader waiting...");
        crossbeam_channel::select! {
            recv(other) -> result => Some(result),
            recv(self.changed_nodes_receiver) -> u => {
                let mut changes = HashSet::new();
                if let Ok(update) = u {
                    changes.insert(update);
                }
                drop(_span);
                self.process_changes(changes);
                None
            }
        }
    }

    /// Sets the query
    pub fn set_query<Q: Query>(&mut self, q: Q, cfg: QueryConfig) -> Result<(), Q::Error> {
        let _span = span!("OctreeReader::set_query");
        debug!("Setting new query: {q:?}");
        let mut ctx_clone;
        let query_context = if cfg.enable_attribute_index {
            &self.query_context
        } else {
            // if the attribute index is disabled for this query,
            // replace it with an empty AttributeIndex in the query context.
            ctx_clone = self.query_context.clone();
            ctx_clone.attribute_index = Arc::new(AttributeIndex::new());
            &ctx_clone
        };
        let query = q.prepare(query_context)?;
        self.query = Box::new(query);
        self.point_filtering = cfg.enable_point_filtering;
        self.update_new_query();
        Ok(())
    }

    /// Reloads a node from the reload queue and removes it from the queue.
    /// Returns LeveledGridCell of old node and new node-page.
    /// Returns None if the update queue is empty.
    pub fn reload_one(&mut self) -> Option<(LeveledGridCell, VectorBuffer)> {
        let _span = span!("OctreeReader::reload_one");
        let (reload, load_kind) = match self.reload_queue.iter().min_by_key(|&(_, (v, _))| *v) {
            None => return None,
            Some((k, (_, v))) => (*k, *v),
        };
        trace!("Reloading node {:?}", reload);
        self.reload_queue.remove(&reload);
        self.loaded.insert(reload, load_kind);
        let node = self.inner.page_cache.load_or_default(&reload).unwrap();
        self.inner.page_cache.cleanup_one_no_write();
        let mut points = node
            .get_points(&*self.inner.codec, &self.inner.point_layout)
            .unwrap();
        if load_kind == LoadKind::Filter {
            points = self.filter_points(reload.lod, &points);
        }
        Some((reload, points))
    }

    /// Loads a node from the load queue.
    /// Adds children of the loaded node to the frontier and schedules them for their initial load.
    /// Returns LeveledGridCell of the loaded node and the node-page.
    /// Returns None if the load queue is empty.
    pub fn load_one(&mut self) -> Option<(LeveledGridCell, VectorBuffer)> {
        let _span = span!("OctreeReader::load_one");

        // get a node to load
        let (load, kind) = match self.load_queue.iter().next() {
            None => return None,
            Some((l, k)) => (*l, *k),
        };
        self.load_queue.remove(&load);
        trace!("Loading node {:?}", load);

        // update the set of loaded nodes
        self.loaded.insert(load, kind);

        // update the frontier (remove this node, but add childresn)
        // and schedule the children that can be loaded immediately for their initial loading
        self.frontier.remove(&load);
        for child in load.children() {
            let exists = self.inner.page_cache.directory().exists(&child);
            let matches_query = self.query.matches_node(child);
            if exists {
                if let Some(load_kind) = matches_query.should_load(self.point_filtering) {
                    self.load_queue.insert(child, load_kind);
                }
            }
            self.frontier.insert(
                child,
                FrontierElement {
                    matches_query,
                    exists,
                },
            );
        }

        // load and return node data
        let node = self.inner.page_cache.load_or_default(&load).unwrap();
        self.inner.page_cache.cleanup_one_no_write();
        let mut points = node
            .get_points(&*self.inner.codec, &self.inner.point_layout)
            .unwrap();
        if kind == LoadKind::Filter {
            points = self.filter_points(load.lod, &points);
        }
        Some((load, points))
    }

    /// Removes a node from the remove queue.
    /// Updates the frontier.
    /// Adds children of the removed node to the remove queue.
    /// Returns LeveledGridCell of the removed node.
    /// Returns None if the remove queue is empty.
    pub fn remove_one(&mut self) -> Option<LeveledGridCell> {
        let _span = span!("OctreeReader::remove_one");

        // get a node to remove
        let remove = match self.remove_queue.iter().next() {
            None => return None,
            Some(e) => *e,
        };
        self.remove_queue.remove(&remove);
        debug!("Removing node {:?}", remove);

        // remove from loaded
        self.loaded.remove(&remove);
        self.reload_queue.remove(&remove);

        // shrink frontier
        self.frontier.insert(
            remove,
            FrontierElement {
                matches_query: NodeQueryResult::Negative,
                exists: true,
            },
        );
        for child in remove.children() {
            self.frontier.remove(&child);
        }

        // check if we also need to unload the parent
        if let Some(parent) = remove.parent() {
            let children_are_leaves = parent.children().iter().all(|c| {
                self.frontier
                    .get(c)
                    .map(|e| e.matches_query == NodeQueryResult::Negative)
                    .unwrap_or(false)
            });
            if children_are_leaves && self.query.matches_node(parent) == NodeQueryResult::Negative {
                self.remove_queue.insert(parent);
            }
        }

        Some(remove)
    }
}
