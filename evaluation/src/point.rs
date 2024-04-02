use lidarserv_common::geometry::points::{PointType, WithAttr};
use lidarserv_common::geometry::position::I32Position;
use lidarserv_common::las::LasPointAttributes;

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
    pub las_attributes: LasPointAttributes,
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
        &self.las_attributes
    }

    fn set_value(&mut self, new_value: LasPointAttributes) {
        self.las_attributes = new_value
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
