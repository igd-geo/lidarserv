use lidarserv_common::geometry::points::{PointType, WithAttr};
use lidarserv_common::geometry::position::{
    CoordinateSystemError, F64CoordinateSystem, F64Position, I32CoordinateSystem, I32Position,
    Position,
};
use lidarserv_common::index::sensor_pos::point::SensorPositionAttribute;
use lidarserv_common::las::{LasExtraBytes, LasPointAttributes};

/// Point type for the lidar server.
#[derive(Debug, Clone)]
pub struct GenericPoint<Position> {
    position: Position,
    sensor_pos: SensorPositionAttribute<Position>,
    las_attributes: Box<LasPointAttributes>,
}

/// Point type for the lidar server, with the positions being stored the same way as
/// they are in LAS, as integer coordinates. However, the positions are only really
/// meaningful in the context of some coordinate system
/// ([lidarserv_common::geometry::position::I32CoordinateSystem]), that will apply some scale
/// and offset transformation.
pub type LasPoint = GenericPoint<I32Position>;

/// Point format with f64 coordinates in global space.
pub type GlobalPoint = GenericPoint<F64Position>;

impl<Pos> PointType for GenericPoint<Pos>
where
    Pos: Position + Default,
{
    type Position = Pos;

    fn new(position: Self::Position) -> Self {
        GenericPoint {
            position,
            sensor_pos: Default::default(),
            las_attributes: Default::default(),
        }
    }

    fn position(&self) -> &Self::Position {
        &self.position
    }
}

impl<Pos> WithAttr<SensorPositionAttribute<Pos>> for GenericPoint<Pos>
where
    Pos: Position,
{
    fn value(&self) -> &SensorPositionAttribute<Pos> {
        &self.sensor_pos
    }

    fn set_value(&mut self, new_value: SensorPositionAttribute<Pos>) {
        self.sensor_pos = new_value;
    }
}

impl<Pos> WithAttr<LasPointAttributes> for GenericPoint<Pos> {
    fn value(&self) -> &LasPointAttributes {
        self.las_attributes.as_ref()
    }

    fn set_value(&mut self, new_value: LasPointAttributes) {
        *self.las_attributes = new_value
    }
}

impl LasExtraBytes for LasPoint {
    const NR_EXTRA_BYTES: usize = SensorPositionAttribute::<I32Position>::NR_EXTRA_BYTES;

    fn get_extra_bytes(&self) -> Vec<u8> {
        self.sensor_pos.get_extra_bytes()
    }

    fn set_extra_bytes(&mut self, extra_bytes: &[u8]) {
        self.sensor_pos.set_extra_bytes(extra_bytes)
    }
}

impl GlobalPoint {
    /// Converts this point into a [LasPoint] with the given coordinate system.
    pub fn into_las_point(
        self,
        coordinate_system: &I32CoordinateSystem,
    ) -> Result<LasPoint, CoordinateSystemError> {
        let GlobalPoint {
            position,
            sensor_pos: SensorPositionAttribute(sensor_position),
            las_attributes,
        } = self;
        let global = F64CoordinateSystem::new();
        let las_position = position.transcode(&global, coordinate_system)?;
        let las_sensor_position = sensor_position.transcode(&global, coordinate_system)?;
        Ok(LasPoint {
            position: las_position,
            sensor_pos: SensorPositionAttribute(las_sensor_position),
            las_attributes,
        })
    }

    pub fn from_las_point(
        las_point: LasPoint,
        coordinate_system: &I32CoordinateSystem,
    ) -> GlobalPoint {
        let LasPoint {
            position,
            sensor_pos: SensorPositionAttribute(sensor_position),
            las_attributes,
        } = las_point;
        let global = F64CoordinateSystem::new();
        // unwrap: transcoding to F64CoordinateSystem never fails
        let position = position.transcode(coordinate_system, &global).unwrap();
        let sensor_position = sensor_position
            .transcode(coordinate_system, &global)
            .unwrap();
        GlobalPoint {
            position,
            sensor_pos: SensorPositionAttribute(sensor_position),
            las_attributes,
        }
    }
}
