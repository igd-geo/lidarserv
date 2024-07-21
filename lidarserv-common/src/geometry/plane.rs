use nalgebra::{Point3, Vector3};

pub struct Plane {
    pub normal: Vector3<f64>,
    pub b: f64,
}

impl Plane {
    pub fn from_triangle(p1: Point3<f64>, p2: Point3<f64>, p3: Point3<f64>) -> Self {
        let normal = (p2 - p1).cross(&(p3 - p1)).normalize();
        let b = normal.dot(&p1.coords);
        Plane { normal, b }
    }

    pub fn signed_distance(&self, p: Point3<f64>) -> f64 {
        self.normal.dot(&p.coords) - self.b
    }

    pub fn is_on_positive_side(&self, p: Point3<f64>) -> bool {
        self.signed_distance(p) >= 0.0
    }

    pub fn is_on_negative_side(&self, p: Point3<f64>) -> bool {
        !self.is_on_positive_side(p)
    }

    pub fn project_onto_plane(&self, p: Point3<f64>) -> Point3<f64> {
        p - self.normal * self.signed_distance(p)
    }
}

#[cfg(test)]
mod tests {
    use nalgebra::{Point3, Vector3};

    use crate::geometry::plane::Plane;

    #[test]
    fn test_plane_from_triangle() {
        let p = Plane::from_triangle(
            Point3::new(1.0, 0.0, 0.5),
            Point3::new(2.0, 0.0, 0.5),
            Point3::new(1.0, 3.0, 0.5),
        );
        assert_eq!(Vector3::new(0.0, 0.0, 1.0), p.normal);
        assert_eq!(0.5, p.b);
        assert!(p.is_on_positive_side(Point3::new(0.0, 0.0, 1.0)));
        assert!(p.is_on_negative_side(Point3::new(0.0, 0.0, 0.0)));
    }

    #[test]
    fn test_project_point_on_plane() {
        let p = Plane::from_triangle(
            Point3::new(1.0, 0.0, 0.5),
            Point3::new(2.0, 0.0, 0.5),
            Point3::new(1.0, 3.0, 0.5),
        );
        let on_plane = p.project_onto_plane(Point3::new(1.0, 2.0, 3.0));
        assert_eq!(on_plane, Point3::new(1.0, 2.0, 0.5));
    }
}
