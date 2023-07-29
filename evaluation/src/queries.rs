use lidarserv_common::geometry::bounding_box::{BaseAABB, AABB};
use lidarserv_common::geometry::grid::LodLevel;
use lidarserv_common::query::bounding_box::BoundingBoxQuery;
use lidarserv_common::query::view_frustum::ViewFrustumQuery;
use nalgebra::{Matrix4, Point3};
use lidarserv_common::index::octree::attribute_bounds::LasPointAttributeBounds;

/// A query that "looks down at an overwiew of the full point cloud".
/// It mostly covers shallow lod levels.
#[rustfmt::skip]
#[allow(clippy::approx_constant)]
pub fn vf__preset_query_1() -> ViewFrustumQuery {
    ViewFrustumQuery::new_raw(
        Matrix4::new(
            2.414213562373095, 0.0000000000000001045301427691423, 0.00000000000000003295012305146171, 0.000000000000000032950057151281516,
            -0.00000000000000013016323879389544, 1.7071067811865472, 0.707108195401524, 0.7071067811865476,
            -0.00000000000000001766470678702153, 1.7071067811865475, -0.7071081954015239, -0.7071067811865475,
            -163.02489952732154, 117.88506141368407, 471.4895251003546, 480.11426701202856,
        ).transpose(),
        Matrix4::new(
            0.41421356237309503, -0.000000000000000022332481132216892, -0.000000000000000003030784533964774, 0.0,
            0.000000000000000017934537145592993, 0.2928932188134524, 0.2928932188134525, 0.0,
            -7.8285945055347765, 43.36108329674386, -35.35530370398832, -0.11593259118318977,
            7.8286101627394435, -42.654063237810625, 34.64826763347989, 0.11593282304860399,
        ).transpose(),
        0.02,
        8.184,
    )
}

/// A query, that "looks down the street".
/// As a result, it covers both detailed lod levels close to the camera and shallow lod levels far away.
#[rustfmt::skip]
pub fn vf_preset_query_2() -> ViewFrustumQuery {
    ViewFrustumQuery::new_raw(
        Matrix4::new(
            -2.011705014272859, 0.2674860964718512, 0.5416431284229128, 0.5416420451377393,
            -1.3347172210980225, -0.40315889614282446, -0.8163723222949177, -0.8163706895519058,
            -0.0000000000000000670078870827233, 2.3652359749930536, -0.20040696802018315, -0.20040656720664793,
            60.636580583746756, -2.9903839056593315, 15.401307939568177, 15.839166713354256
        ).transpose(),
        Matrix4::new(
            -0.34515401346130115, -0.22900127127456288, -0.000000000000000017245103777198202, 0.0,
            0.04589335866209349, -0.06917113099537653, 0.4058103368833067, 0.0,
            -28.5163542592145, -60.767795058196825, -10.020318340004037, -2.2836761913565216,
            29.058053337117787, 59.951545904356564, 9.81993181345411, 2.2836807587134715
        ).transpose(),
        0.02,
        8.184,
    )
}

/// A query, that "looks at a detail on the ground".
/// As a result, it covers very detailed lod levels.
#[rustfmt::skip]
pub fn vf_preset_query_3() -> ViewFrustumQuery {
    ViewFrustumQuery::new_raw(
        Matrix4::new(
            -2.33458662155896, 0.6149247361946918, 0.0, 0.0,
            -0.6149247361946918, -2.33458662155896, -0.0000000000000001147389927325049, -0.0000000000000001147387632547489,
            0.00000000000000007055570372573166, 0.0000000000000002678675816687576, -1.000002000002, -1.0,
            72.51569552480794, -11.153318240539475, 3.793036395069941, 3.8704374812204745
        ).transpose(),
        Matrix4::new(
            -0.4005517391899489, -0.1055044050536138, 0.000000000000000012105444953779729, 0.0,
            0.1055044050536138, -0.4005517391899489, 0.00000000000000004595881117419348, 0.0,
            -390.4336666075007, -41.12249747745215, -49.99995, -12.918423367539681,
            390.4344474756148, 41.12257972252935, 49.000049999999995, 12.918449204412253
        ).transpose(),
        0.02,
        8.184,
    )
}

/// AABB query, that covers the whole point cloud with a very high lod level.
pub fn aabb_full() -> BoundingBoxQuery {
    BoundingBoxQuery::new(
        AABB::new(
            Point3::new(-10000, -10000, -10000),
            Point3::new(10000, 70000, 20000),
        ),
        LodLevel::from_level(20),
    )
}

pub fn ground_classification() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((11,11)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

pub fn no_cars_classification() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((0,20)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

pub fn high_intensity() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: Some((64000, 65535)),
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: None,
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

pub fn low_intensity() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: Some((0, 1000)),
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: None,
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

pub fn one_return() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: Some((1, 1)),
        scan_direction: None,
        edge_of_flight_line: None,
        classification: None,
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

pub fn time_range() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: None,
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: Some((-710108.82202797, 191462.59930070)),
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

pub fn full_red_part() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: None,
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        color_r: Some((255, 255)),
        color_g: None,
        color_b: None,
    }
}

pub fn mixed_ground_and_time() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((11,11)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: Some((-710108.82202797, 191462.59930070)),
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

pub fn mixed_ground_and_one_return() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: Some((1, 1)),
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((11,11)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        color_r: None,
        color_g: None,
        color_b: None,
    }
}