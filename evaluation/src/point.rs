use lidarserv_common::geometry::points::{PointType, WithAttr};
use lidarserv_common::geometry::position::I32Position;
use lidarserv_common::las::LasPointAttributes;

#[derive(Default, Clone, Debug)]
pub struct PointIdAttribute(pub usize);

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
