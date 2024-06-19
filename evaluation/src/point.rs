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

    fn position_mut(&mut self) -> &mut Self::Position {
        &mut self.position
    }
}

impl WithAttr<LasPointAttributes> for Point {
    fn value(&self) -> &LasPointAttributes {
        &self.las_attributes
    }

    fn value_mut(&mut self) -> &mut LasPointAttributes {
        &mut self.las_attributes
    }
}

impl WithAttr<PointIdAttribute> for Point {
    fn value(&self) -> &PointIdAttribute {
        &self.point_id
    }

    fn value_mut(&mut self) -> &mut PointIdAttribute {
        &mut self.point_id
    }
}
