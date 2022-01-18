use crate::geometry::grid::{LeveledGridCell, LodLevel};
use crate::geometry::points::PointType;
use crate::geometry::position::I32Position;
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index::octree::page_manager::Page;
use crate::index::octree::Inner;
use crate::index::{Node, NodeId, Reader, Update};
use crate::las::LasReadWrite;
use crate::lru_cache::pager::PageDirectory;
use crate::query::{Query, QueryExt};
use crossbeam_channel::Receiver;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct OctreeReader<Point, LasL, Sampl, SamplF> {
    pub(super) inner: Arc<Inner<Point, LasL, Sampl, SamplF>>,
    update_cnt: u64,
    query: Box<dyn Query + Send + Sync>,
    changed_nodes_receiver: crossbeam_channel::Receiver<LeveledGridCell>,
    loaded: HashSet<LeveledGridCell>,
    frontier: HashMap<LeveledGridCell, FrontierElement>,
    load_queue: HashSet<LeveledGridCell>,
    reload_queue: HashMap<LeveledGridCell, u64>,
    remove_queue: HashSet<LeveledGridCell>,
    known_root_nodes: HashSet<LeveledGridCell>,
}

#[derive(Debug)]
struct FrontierElement {
    matches_query: bool,
    exists: bool,
}

pub struct OctreePage<Sampl, Point, LasL> {
    page: Arc<Page<Sampl, Point>>,
    loader: LasL,
}

impl<Point, LasL, Sampl, SamplF> OctreeReader<Point, LasL, Sampl, SamplF>
where
    Point: PointType<Position = I32Position> + Clone,
    LasL: LasReadWrite<Point> + Clone,
    Sampl: Sampling<Point = Point>,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
{
    pub(super) fn new<Q>(query: Q, inner: Arc<Inner<Point, LasL, Sampl, SamplF>>) -> Self
    where
        Q: Query + Send + Sync + 'static,
    {
        // add subscription to changes
        let changed_nodes_receiver = {
            let (changed_nodes_sender, changed_nodes_receiver) = crossbeam_channel::unbounded();
            let mut lock = inner.subscriptions.lock().unwrap();
            lock.push(changed_nodes_sender);
            changed_nodes_receiver
        };
        let root_nodes = inner.page_cache.directory().get_root_cells();
        let mut reader = OctreeReader {
            inner,
            update_cnt: 0,
            query: Box::new(query),
            changed_nodes_receiver,
            loaded: HashSet::new(),
            frontier: HashMap::default(),
            load_queue: HashSet::new(),
            reload_queue: HashMap::new(),
            remove_queue: HashSet::new(),
            known_root_nodes: HashSet::new(),
        };
        for root_node in root_nodes {
            reader.add_root(root_node);
        }
        reader
    }

    fn add_root(&mut self, cell: LeveledGridCell) {
        let matches_query = self.cell_matches_query(&cell);
        self.frontier.insert(
            cell.clone(),
            FrontierElement {
                matches_query,
                exists: true,
            },
        );
        if matches_query {
            self.load_queue.insert(cell.clone());
        }
        self.known_root_nodes.insert(cell);
    }

    fn cell_matches_query(&self, cell: &LeveledGridCell) -> bool {
        Self::cell_matches_query_impl(cell, &self.query, self.inner.as_ref())
    }

    fn cell_matches_query_impl(
        cell: &LeveledGridCell,
        query: &Box<dyn Query + Send + Sync>,
        inner: &Inner<Point, LasL, Sampl, SamplF>,
    ) -> bool {
        let bounds = inner.node_hierarchy.get_leveled_cell_bounds(cell);
        let lod = cell.lod;
        query.matches_node(&bounds, &inner.coordinate_system, &lod)
    }

    fn process_changes(&mut self, mut changes: HashSet<LeveledGridCell>) {
        // get remaining changes from the channel, if any
        while let Ok(update) = self.changed_nodes_receiver.try_recv() {
            changes.insert(update);
        }

        // schedule all changed nodes, that are already loaded for a reload.
        let reload: Vec<_> = self.loaded.intersection(&changes).cloned().collect();
        if !reload.is_empty() {
            self.update_cnt += 1;
        }
        for reload_cell in reload {
            self.reload_queue
                .entry(reload_cell)
                .or_insert(self.update_cnt);
        }

        // Update the frontier.
        // Any elements that now both exist and match the query get scheduled for their initial load.
        for change in &changes {
            if let Some(elem) = self.frontier.get_mut(change) {
                elem.exists = true;
                if elem.matches_query {
                    self.load_queue.insert(change.clone());
                }
            }
        }

        // add all new root nodes
        let new_root: Vec<_> = changes
            .iter()
            .filter(|it| it.lod == LodLevel::base())
            .filter(|it| !self.known_root_nodes.contains(it))
            .cloned()
            .collect();
        for new_root_cell in new_root {
            self.add_root(new_root_cell)
        }
    }

    pub fn update(&mut self) {
        let changes = HashSet::new();
        self.process_changes(changes);
    }

    pub fn wait_update(&mut self) {
        let mut changes = HashSet::new();
        if let Ok(update) = self.changed_nodes_receiver.recv() {
            changes.insert(update);
        }
        self.process_changes(changes);
    }

    pub fn wait_update_or<T>(
        &mut self,
        other: &crossbeam_channel::Receiver<T>,
    ) -> Option<Result<T, crossbeam_channel::RecvError>> {
        loop {
            crossbeam_channel::select! {
                recv(other) -> result => return Some(result),
                recv(self.changed_nodes_receiver) -> u => {
                    let mut changes = HashSet::new();
                    if let Ok(update) = u {
                        changes.insert(update);
                    }
                    self.process_changes(changes);
                    return None
                }
            }
        }
    }

    pub fn set_query(&mut self, q: Box<dyn Query + Send + Sync + 'static>) {
        self.query = q;

        {
            let Self {
                frontier, query, ..
            } = self;
            for (cell, elem) in frontier {
                elem.matches_query =
                    Self::cell_matches_query_impl(cell, query, self.inner.as_ref());
            }
        }

        self.load_queue = self
            .frontier
            .iter()
            .filter_map(|(cell, elem)| {
                if elem.exists && elem.matches_query {
                    Some(cell.clone())
                } else {
                    None
                }
            })
            .collect();

        let mut removable_cnt = HashMap::new();
        for (cell, elem) in &self.frontier {
            if !elem.matches_query {
                if let Some(parent) = cell.parent() {
                    removable_cnt
                        .entry(parent)
                        .and_modify(|cnt| *cnt += 1)
                        .or_insert(1);
                }
            }
        }
        self.remove_queue = removable_cnt
            .into_iter()
            .filter_map(|(cell, cnt)| if cnt == 8 { Some(cell) } else { None })
            .filter(|cell| !self.cell_matches_query(cell))
            .collect();
    }

    pub fn reload_one(&mut self) -> Option<(LeveledGridCell, Arc<Page<Sampl, Point>>)> {
        let reload = match self.reload_queue.iter().min_by_key(|&(_, v)| *v) {
            None => return None,
            Some((k, _)) => *k,
        };
        self.reload_queue.remove(&reload);
        let node = self.inner.page_cache.load_or_default(&reload).unwrap();
        Some((reload, node))
    }

    pub fn load_one(&mut self) -> Option<(LeveledGridCell, Arc<Page<Sampl, Point>>)> {
        // get a node to load
        let load = match self.load_queue.iter().next() {
            None => return None,
            Some(e) => e.clone(),
        };
        self.load_queue.remove(&load);

        // update the set of loaded nodes
        self.loaded.insert(load.clone());

        // update the frontier (remove this node, but add children)
        // and schedule the children that can be loaded immediately for their initial loading
        self.frontier.remove(&load);
        for child in load.children() {
            let exists = self.inner.page_cache.directory().exists(&child);
            let matches_query = self.cell_matches_query(&child);
            if exists && matches_query {
                self.load_queue.insert(child.clone());
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
        Some((load, node))
    }

    pub fn remove_one(&mut self) -> Option<LeveledGridCell> {
        // get a node to remove
        let remove = match self.remove_queue.iter().next() {
            None => return None,
            Some(e) => e.clone(),
        };
        self.remove_queue.remove(&remove);

        // remove from loaded
        self.loaded.remove(&remove);
        self.reload_queue.remove(&remove);

        // shrink frontier
        self.frontier.insert(
            remove.clone(),
            FrontierElement {
                matches_query: false,
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
                    .get(&c)
                    .map(|e| !e.matches_query)
                    .unwrap_or(false)
            });
            if children_are_leaves && !self.cell_matches_query(&parent) {
                self.remove_queue.insert(parent);
            }
        }

        Some(remove)
    }

    pub fn is_dirty(&self) -> bool {
        !self.load_queue.is_empty()
            || !self.reload_queue.is_empty()
            || !self.remove_queue.is_empty()
    }
}

impl<Point, LasL, Sampl, SamplF> Reader<Point> for OctreeReader<Point, LasL, Sampl, SamplF>
where
    Point: PointType<Position = I32Position> + Clone,
    LasL: LasReadWrite<Point> + Clone,
    Sampl: Sampling<Point = Point>,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
{
    type NodeId = LeveledGridCell;
    type Node = OctreePage<Sampl, Point, LasL>;

    fn set_query<Q: Query + 'static + Send + Sync>(&mut self, query: Q) {
        OctreeReader::set_query(self, Box::new(query))
    }

    fn update(&mut self) {
        OctreeReader::update(self)
    }

    fn blocking_update(&mut self, queries: &mut Receiver<Box<dyn Query + Send + Sync>>) -> bool {
        // make sure we've go the most recent query
        if let Some(q) = queries.try_iter().last() {
            self.set_query(q);
        }

        // make sure we have the most recent updates from the writer
        self.update();

        loop {
            // if there are things to do (load_one, remove_one, update_one) return early.
            if self.is_dirty() {
                return true;
            }

            // if there is nothing to do:
            // wait for something to happen (either a new query, or an update to come in).
            match self.wait_update_or(&queries) {
                None => (),
                Some(Ok(query)) => {
                    if let Some(q) = queries.try_iter().last() {
                        self.set_query(q);
                    } else {
                        self.set_query(query);
                    }
                }
                Some(Err(_)) => return false,
            }
        }
    }

    fn load_one(&mut self) -> Option<(Self::NodeId, Self::Node)> {
        OctreeReader::load_one(self).map(|(n, d)| (n, OctreePage::from_page(d, &self.inner.loader)))
    }

    fn remove_one(&mut self) -> Option<Self::NodeId> {
        OctreeReader::remove_one(self)
    }

    fn update_one(&mut self) -> Option<Update<Self::NodeId, Self::Node>> {
        OctreeReader::reload_one(self)
            .map(|(n, d)| (n, vec![(n, OctreePage::from_page(d, &self.inner.loader))]))
    }
}

impl<Sampl, Point, LasL> OctreePage<Sampl, Point, LasL>
where
    Point: PointType<Position = I32Position> + Clone,
    Sampl: Sampling<Point = Point>,
    LasL: LasReadWrite<Point> + Clone,
{
    pub fn from_page(page: Arc<Page<Sampl, Point>>, loader: &LasL) -> Self {
        OctreePage {
            page,
            loader: loader.clone(),
        }
    }
}

impl<Sampl, Point, LasL> Node for OctreePage<Sampl, Point, LasL>
where
    Point: PointType<Position = I32Position> + Clone,
    Sampl: Sampling<Point = Point>,
    LasL: LasReadWrite<Point> + Clone,
{
    fn las_files(&self) -> Vec<Arc<Vec<u8>>> {
        vec![self.page.get_binary(&self.loader)]
    }
}

impl NodeId for LeveledGridCell {
    fn lod(&self) -> LodLevel {
        self.lod
    }
}
