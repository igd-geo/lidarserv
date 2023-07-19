use crate::geometry::grid::{LeveledGridCell, LodLevel};
use crate::geometry::points::PointType;
use crate::geometry::points::WithAttr;
use crate::geometry::position::{I32CoordinateSystem, I32Position};
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index::octree::page_manager::Page;
use crate::index::octree::Inner;
use crate::index::{Node, NodeId, Reader, Update};
use crate::las::I32LasReadWrite;
use crate::las::LasPointAttributes;
use crate::lru_cache::pager::PageDirectory;
use crate::query::{Query, QueryExt};
use crossbeam_channel::Receiver;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use log::debug;
use crate::index::octree::attribute_bounds::LasPointAttributeBounds;

pub struct OctreeReader<Point, Sampl, SamplF> {
    pub(super) inner: Arc<Inner<Point, Sampl, SamplF>>,
    update_cnt: u64,
    query: Box<dyn Query + Send + Sync>,
    filter: Option<LasPointAttributeBounds>,
    enable_attribute_acceleration: bool,
    enable_histogram_acceleration: bool,
    enable_point_filtering: bool,
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

pub struct OctreePage<Sampl, Point> {
    page: Arc<Page<Sampl, Point>>,
    loader: I32LasReadWrite,
}

impl<Point, Sampl, SamplF> OctreeReader<Point, Sampl, SamplF>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + Clone,
    Sampl: Sampling<Point = Point>,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
{
    /// Creates a new reader for the given octree and query.
    /// All root nodes of the octree are added to the reader.
    pub(super) fn new<Q>(query: Q, inner: Arc<Inner<Point, Sampl, SamplF>>) -> Self
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
            filter: None,
            enable_attribute_acceleration: false,
            enable_histogram_acceleration: false,
            enable_point_filtering: false,
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

    /// Adds a new root node to the readers frontier and list of known root nodes.
    /// If the node matches the query, it is also added to the load queue.
    fn add_root(&mut self, cell: LeveledGridCell) {
        let matches_query = self.cell_matches_query(&cell);
        self.frontier.insert(
            cell,
            FrontierElement {
                matches_query,
                exists: true,
            },
        );
        if matches_query {
            self.load_queue.insert(cell);
        }
        self.known_root_nodes.insert(cell);
    }

    fn cell_matches_query(&self, cell: &LeveledGridCell) -> bool {
        Self::cell_matches_query_impl(cell, self.query.as_ref(), &self.filter, self.inner.as_ref(), self.enable_attribute_acceleration, self.enable_histogram_acceleration)
    }

    /// Checks, if the given cell matches the query and filter.
    fn cell_matches_query_impl(
        cell: &LeveledGridCell,
        query: &(dyn Query + Send + Sync),
        filter: &Option<LasPointAttributeBounds>,
        inner: &Inner<Point, Sampl, SamplF>,
        enable_attribute_acceleration: bool,
        enable_histogram_acceleration: bool,
    ) -> bool {
        let bounds = inner.node_hierarchy.get_leveled_cell_bounds(cell);
        let lod = cell.lod;

        // check spatial query for cell
        if !query.matches_node(&bounds, &inner.coordinate_system, &lod) {
            debug!("Cell {:?} does not match query", cell);
            return false;
        }

        // check attributes for cell
        if enable_attribute_acceleration {
            if let Some(filter) = filter {
                let attribute_index = inner.attribute_index.as_ref().unwrap();
                if !attribute_index.cell_overlaps_with_bounds(lod, &cell.pos, filter, enable_histogram_acceleration) {
                    debug!("Cell {:?} does not match attribute filter {:?}", cell, filter);
                    return false;
                }
            }
        }
        debug!("Cell {:?} matches query and filter", cell);
        true
    }

    /// Filters out all points of the given Vector, that do not match the query or filter
    /// returns vector of points that match the query and filter.
    fn filter_points(&self, points: &Vec<Point>) -> Vec<Point> {
        let mut filtered_points = Vec::new();
        for point in points {
            if self.query.matches_point(point, &self.inner.coordinate_system) {
                if let Some(filter) = &self.filter {
                    if !filter.is_attributes_in_bounds(&point.attribute()) {
                        continue;
                    }
                }
                filtered_points.push(point.clone());
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
                    self.load_queue.insert(*change);
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

    /// Sets the query
    pub fn set_query(&mut self, q: Box<dyn Query + Send + Sync + 'static>) {
        debug!("Setting new query");
        self.query = q;
        self.update_new_query();
    }

    /// Sets the filter
    pub fn set_filter(&mut self, f: (Option<LasPointAttributeBounds>, bool, bool, bool)) {
        debug!("Setting new filter");
        self.filter = f.0;
        self.enable_attribute_acceleration = f.1;
        self.enable_histogram_acceleration = f.2;
        self.enable_point_filtering = f.3;
        self.update_new_query();
    }

    /// Updates the frontier, load and remove queue after new query or filter.
    fn update_new_query(&mut self) {
        // update frontier
        debug!("Updating frontier after new query");
        {
            let Self {
                frontier, query, ..
            } = self;
            for (cell, elem) in frontier {
                elem.matches_query =
                    Self::cell_matches_query_impl(cell, query.as_ref(), &self.filter, self.inner.as_ref(), self.enable_attribute_acceleration, self.enable_histogram_acceleration);
            }
        }

        // update load queue from frontier
        debug!("Updating load queue after new query");
        self.load_queue = self
            .frontier
            .iter()
            .filter_map(|(cell, elem)| {
                if elem.exists && elem.matches_query {
                    Some(*cell)
                } else {
                    None
                }
            })
            .collect();

        // search for removable nodes
        debug!("Searching for removable nodes after new query");
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

        // remove nodes that are not needed anymore --> add to remove queue
        debug!("Updating remove queue after new query");
        self.remove_queue = removable_cnt
            .into_iter()
            .filter_map(|(cell, cnt)| if cnt == 8 { Some(cell) } else { None })
            .filter(|cell| !self.cell_matches_query(cell))
            .collect();
    }

    /// Reloads a node from the reload queue and removes it from the queue.
    /// Returns LeveledGridCell of old node and new node-page.
    /// Returns None if the update queue is empty.
    pub fn reload_one(&mut self) -> Option<(LeveledGridCell, Vec<Point>, I32CoordinateSystem)> {
        let reload = match self.reload_queue.iter().min_by_key(|&(_, v)| *v) {
            None => return None,
            Some((k, _)) => *k,
        };
        self.reload_queue.remove(&reload);
        debug!("Reloading node {:?} from reload queue", reload);
        let node = self.inner.page_cache.load_or_default(&reload).unwrap();
        let loader = &self.inner.loader;
        let mut points = node.get_points(loader).unwrap();
        if self.enable_point_filtering {
            points = self.filter_points(&points);
        }
        Some((reload, points, self.inner.coordinate_system.clone()))
    }

    /// Loads a node from the load queue.
    /// Adds children of the loaded node to the frontier and schedules them for their initial load.
    /// Returns LeveledGridCell of the loaded node and the node-page.
    /// Returns None if the load queue is empty.
    pub fn load_one(&mut self) -> Option<(LeveledGridCell, Vec<Point>, I32CoordinateSystem)> {
        // get a node to load
        let load = match self.load_queue.iter().next() {
            None => return None,
            Some(e) => *e,
        };
        self.load_queue.remove(&load);
        debug!("Loading node {:?} from load queue", load);

        // update the set of loaded nodes
        self.loaded.insert(load);

        // update the frontier (remove this node, but add childresn)
        // and schedule the children that can be loaded immediately for their initial loading
        self.frontier.remove(&load);
        for child in load.children() {
            let exists = self.inner.page_cache.directory().exists(&child);
            let matches_query = self.cell_matches_query(&child);
            if exists && matches_query {
                self.load_queue.insert(child);
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
        let loader = &self.inner.loader;
        let mut points = node.get_points(loader).unwrap();
        if self.enable_point_filtering {
            points = self.filter_points(&points);
        }
        Some((load, points, self.inner.coordinate_system.clone()))
    }

    /// Removes a node from the remove queue.
    /// Updates the frontier.
    /// Adds children of the removed node to the remove queue.
    /// Returns LeveledGridCell of the removed node.
    /// Returns None if the remove queue is empty.
    pub fn remove_one(&mut self) -> Option<LeveledGridCell> {
        // get a node to remove
        let remove = match self.remove_queue.iter().next() {
            None => return None,
            Some(e) => *e,
        };
        self.remove_queue.remove(&remove);
        debug!("Removing node {:?} from remove queue", remove);

        // remove from loaded
        self.loaded.remove(&remove);
        self.reload_queue.remove(&remove);

        // shrink frontier
        self.frontier.insert(
            remove,
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
                    .get(c)
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

impl<Point, Sampl, SamplF> Reader<Point> for OctreeReader<Point, Sampl, SamplF>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + Clone,
    Sampl: Sampling<Point = Point>,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl>,
{
    type NodeId = LeveledGridCell;
    type Node = OctreePage<Sampl, Point>;

    fn set_query<Q: Query + 'static + Send + Sync>(&mut self, query: Q) {
        OctreeReader::set_query(self, Box::new(query))
    }

    fn set_filter(&mut self, filter: (Option<LasPointAttributeBounds>, bool, bool, bool)) {
        OctreeReader::set_filter(self, filter)
    }

    fn fetch_query_filter(
        &mut self,
        queries: &mut Receiver<Box<dyn Query + Send + Sync>>,
        filters: &mut Receiver<(Option<LasPointAttributeBounds>, bool, bool, bool)>
    ) {
        if let Some(q) = queries.try_iter().last() {
            self.set_query(q);
            debug!("Updating query");
        }
        if let Some(f) = filters.try_iter().last() {
            self.set_filter(f);
            debug!("Updating filter");
        }
    }

    fn updates_available(
        &mut self,
        queries: &mut Receiver<Box<dyn Query + Send + Sync>>,
        filters: &mut Receiver<(Option<LasPointAttributeBounds>, bool, bool, bool)>
    ) -> bool {
        self.fetch_query_filter(queries, filters);
        self.update();
        self.is_dirty()
    }

    fn update(&mut self) {
        OctreeReader::update(self)
    }

    fn blocking_update(
        &mut self,
        queries: &mut Receiver<Box<dyn Query + Send + Sync>>,
        filters: &mut Receiver<(Option<LasPointAttributeBounds>, bool, bool, bool)>
    ) -> bool {
        // make sure we've go the most recent query
        self.fetch_query_filter(queries, filters);

        // make sure we have the most recent updates from the writer
        self.update();

        loop {
            // if there are things to do (load_one, remove_one, update_one) return early.
            if self.is_dirty() {
                return true;
            }

            // if there is nothing to do:
            // wait for something to happen (either a new query, or an update to come in).
            match self.wait_update_or(queries) {
                None => (),
                Some(Ok(query)) => {
                    self.set_query(query);
                    self.fetch_query_filter(queries, filters);
                }
                Some(Err(_)) => return false,
            }
        }
    }


    fn load_one(&mut self) -> Option<(Self::NodeId, Vec<Point>, I32CoordinateSystem)> {
        OctreeReader::load_one(self)
    }

    fn remove_one(&mut self) -> Option<Self::NodeId> {
        OctreeReader::remove_one(self)
    }

    fn update_one(&mut self) -> Option<Update<Self::NodeId, I32CoordinateSystem, Vec<Point>>> {
        OctreeReader::reload_one(self)
            .map(|(n, d, c)| (n, c, vec![(n, d)]))
    }
}

impl<Sampl, Point> OctreePage<Sampl, Point>
where
    Point: PointType<Position = I32Position> + Clone,
    Sampl: Sampling<Point = Point>,
{
    pub fn from_page(page: Arc<Page<Sampl, Point>>, loader: &I32LasReadWrite) -> Self {
        OctreePage {
            page,
            loader: loader.clone(),
        }
    }
}

impl<Sampl, Point> Node<Point> for OctreePage<Sampl, Point>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + Clone,
    Sampl: Sampling<Point = Point>,
{
    fn las_files(&self) -> Vec<Arc<Vec<u8>>> {
        vec![self.page.get_binary(&self.loader)]
    }

    fn points(&self) -> Vec<Point> {
        self.page.get_points(&self.loader).unwrap_or(vec![])
    }
}

impl NodeId for LeveledGridCell {
    fn lod(&self) -> LodLevel {
        self.lod
    }
}
