use lidarserv_common::geometry::points::{PointType, WithAttr};
use lidarserv_common::geometry::position::I32Position;
use lidarserv_common::index::sensor_pos::point::SensorPositionAttribute;
use lidarserv_common::las::{LasExtraBytes, LasPointAttributes};
use std::mem::size_of;

#[derive(Default, Clone, Debug)]
pub struct PointIdAttribute(pub usize);

static DEFAULT_LAS_POINT_ATTRS: LasPointAttributes = LasPointAttributes {
    intensity: 0,
    return_number: 0,
    number_of_returns: 0,
    scan_direction: false,
    edge_of_flight_line: false,
    classification: 0,
    scan_angle_rank: 0,
    user_data: 0,
    point_source_id: 0,
    color: (0, 0, 0),
};

#[derive(Default, Clone, Debug)]
pub struct Point {
    pub position: I32Position,
    pub sensor_position: SensorPositionAttribute,
    pub point_id: PointIdAttribute,
}

impl PointType for Point {
    type Position = I32Position;

    fn new(position: Self::Position) -> Self {
        Point {
            position,
            ..Default::default()
        }
    }

    fn position(&self) -> &Self::Position {
        &self.position
    }
}

impl WithAttr<SensorPositionAttribute> for Point {
    fn value(&self) -> &SensorPositionAttribute {
        &self.sensor_position
    }

    fn set_value(&mut self, new_value: SensorPositionAttribute) {
        self.sensor_position = new_value;
    }
}

impl WithAttr<LasPointAttributes> for Point {
    fn value(&self) -> &LasPointAttributes {
        // return a dummy value - we don't care about the las attributes in the evaluation code.
        // this point attribute is only here to make the las writer happy.
        // (and since ALL points are read into memory for the evaluation, points should not be to big)
        &DEFAULT_LAS_POINT_ATTRS
    }

    fn set_value(&mut self, _: LasPointAttributes) {
        // ignore - we don't care about the las attributes in the evaluation code.
        // this point attribute is only here to make the las writer happy.
    }
}

impl WithAttr<PointIdAttribute> for Point {
    fn value(&self) -> &PointIdAttribute {
        &self.point_id
    }

    fn set_value(&mut self, new_value: PointIdAttribute) {
        self.point_id = new_value;
    }
}

impl LasExtraBytes for Point {
    const NR_EXTRA_BYTES: usize = SensorPositionAttribute::NR_EXTRA_BYTES + size_of::<usize>();

    fn get_extra_bytes(&self) -> Vec<u8> {
        let mut extra = self.sensor_position.get_extra_bytes();
        extra.extend(self.point_id.0.to_le_bytes());
        extra
    }

    fn set_extra_bytes(&mut self, extra_bytes: &[u8]) {
        let mut point_id_bytes = [0; size_of::<usize>()];
        let (sensor_pos, rest) = extra_bytes.split_at(SensorPositionAttribute::NR_EXTRA_BYTES);
        self.sensor_position.set_extra_bytes(sensor_pos);
        point_id_bytes.copy_from_slice(rest);
        self.point_id.0 = usize::from_le_bytes(point_id_bytes);
    }
}
