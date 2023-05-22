use lidarserv_common::geometry::points::{PointType, WithAttr};
use lidarserv_common::geometry::position::I32Position;
use lidarserv_common::las::{LasPointAttributes};

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
    gps_time: 0.0,
};

#[derive(Default, Clone, Debug)]
pub struct Point {
    pub position: I32Position,
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