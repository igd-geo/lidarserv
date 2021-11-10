use crate::common::geometry::sampling::GridCenterSampling;
use crate::common::index::octree::writer::OctreeWriter;
use crate::common::index::Index;
use crate::index::point::LasPoint;
use lidarserv_common::geometry::grid::{I32Grid, I32GridHierarchy};
use lidarserv_common::geometry::position::{I32CoordinateSystem, I32Position};
use lidarserv_common::geometry::sampling::GridCenterSamplingFactory;
use lidarserv_common::index::octree::Octree;
use lidarserv_common::index::sensor_pos::writer::SensorPosWriter;
use lidarserv_common::index::sensor_pos::SensorPosIndex;
use lidarserv_common::index::Writer as CommonWriter;
use lidarserv_common::las::I32LasReadWrite;
use std::error::Error;
use thiserror::Error;

pub mod builder;
pub mod point;
pub mod settings;

#[derive(Debug, Error)]
#[error("Coordinate system mismatch.")]
pub struct CoordinateSystemMismatchError;

/// object safe wrapper for a point cloud index, otherwise very similar to [lidarserv_common::index::Index].
pub trait DynIndex: Send + Sync {
    fn index_info(&self) -> &I32CoordinateSystem; // maybe return more info in future...
    fn writer(&self) -> Box<dyn DynWriter>;
    fn flush(&mut self) -> Result<(), Box<dyn Error>>;
}

/// object safe wrapper for a point cloud writer, otherwise very similar to [lidarserv_common::index::Writer].
pub trait DynWriter: Send + Sync {
    fn insert_points(
        &mut self,
        points: Vec<LasPoint>,
        coordinate_system: &I32CoordinateSystem,
    ) -> Result<(), CoordinateSystemMismatchError>;
}

impl DynIndex
    for SensorPosIndex<
        I32GridHierarchy,
        GridCenterSamplingFactory<I32GridHierarchy, LasPoint, I32Position, i32>,
        i32,
        I32LasReadWrite,
        I32CoordinateSystem,
        LasPoint,
    >
{
    fn index_info(&self) -> &I32CoordinateSystem {
        self.coordinate_system()
    }

    fn writer(&self) -> Box<dyn DynWriter> {
        let wr = lidarserv_common::index::Index::writer(self);
        Box::new(wr)
    }

    fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

impl DynWriter for SensorPosWriter<LasPoint, I32CoordinateSystem> {
    fn insert_points(
        &mut self,
        points: Vec<LasPoint>,
        coordinate_system: &I32CoordinateSystem,
    ) -> Result<(), CoordinateSystemMismatchError> {
        if *self.coordinate_system() != *coordinate_system {
            return Err(CoordinateSystemMismatchError);
        }
        self.insert(points);
        Ok(())
    }
}

impl DynIndex
    for Octree<
        LasPoint,
        I32GridHierarchy,
        I32LasReadWrite,
        GridCenterSampling<I32Grid, LasPoint, I32Position, i32>,
        i32,
        I32CoordinateSystem,
        GridCenterSamplingFactory<I32GridHierarchy, LasPoint, I32Position, i32>,
    >
{
    fn index_info(&self) -> &I32CoordinateSystem {
        self.coordinate_system()
    }

    fn writer(&self) -> Box<dyn DynWriter> {
        Box::new((self.coordinate_system().clone(), Index::writer(self)))
    }

    fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        match Octree::flush(self) {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e)),
        }
    }
}

impl DynWriter
    for (
        I32CoordinateSystem,
        OctreeWriter<LasPoint, I32GridHierarchy>,
    )
{
    fn insert_points(
        &mut self,
        points: Vec<LasPoint>,
        coordinate_system: &I32CoordinateSystem,
    ) -> Result<(), CoordinateSystemMismatchError> {
        if self.0 != *coordinate_system {
            return Err(CoordinateSystemMismatchError);
        }
        self.1.insert(points);
        Ok(())
    }
}
