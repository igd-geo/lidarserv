use lidarserv_common::geometry::bounding_box::{BaseAABB, AABB};
use lidarserv_common::geometry::grid::LodLevel;
use lidarserv_common::index::octree::attribute_bounds::LasPointAttributeBounds;
use lidarserv_common::query::bounding_box::BoundingBoxQuery;
use lidarserv_common::query::view_frustum::ViewFrustumMatrixQuery;
use nalgebra::{Matrix4, Point3};

/// A query that "looks down at an overwiew of the full point cloud".
/// It mostly covers shallow lod levels.
#[rustfmt::skip]
#[allow(clippy::approx_constant)]
pub fn vf_preset_query_1() -> ViewFrustumMatrixQuery {
    ViewFrustumMatrixQuery::new_raw(
        Matrix4::new(
            2.414213562373095, 0.0000000000000001045301427691423, 0.00000000000000003295012305146171, 0.000000000000000032950057151281516,
            -0.00000000000000013016323879389544, 1.7071067811865472, std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2,
            -0.00000000000000001766470678702153, 1.7071067811865475, -std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2,
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
pub fn vf_preset_query_2() -> ViewFrustumMatrixQuery {
    ViewFrustumMatrixQuery::new_raw(
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
pub fn vf_preset_query_3() -> ViewFrustumMatrixQuery {
    ViewFrustumMatrixQuery::new_raw(
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
            Point3::new(-10000000, -10000000, -10000000),
            Point3::new(10000000, 10000000, 10000000),
        ),
        LodLevel::from_level(10),
    )
}

pub fn filter_apply_defaults(bounds: LasPointAttributeBounds) -> LasPointAttributeBounds {
    let mut attribute_bounds = LasPointAttributeBounds::new();
    attribute_bounds.intensity = Some((
        bounds.intensity.unwrap_or((0, u16::MAX)).0,
        bounds.intensity.unwrap_or((0, u16::MAX)).1,
    ));
    attribute_bounds.return_number = Some((
        bounds.return_number.unwrap_or((0, u8::MAX)).0,
        bounds.return_number.unwrap_or((0, u8::MAX)).1,
    ));
    attribute_bounds.number_of_returns = Some((
        bounds.number_of_returns.unwrap_or((0, u8::MAX)).0,
        bounds.number_of_returns.unwrap_or((0, u8::MAX)).1,
    ));
    attribute_bounds.scan_direction = Some((
        bounds.scan_direction.unwrap_or((false, true)).0,
        bounds.scan_direction.unwrap_or((false, true)).1,
    ));
    attribute_bounds.edge_of_flight_line = Some((
        bounds.edge_of_flight_line.unwrap_or((false, true)).0,
        bounds.edge_of_flight_line.unwrap_or((false, true)).1,
    ));
    attribute_bounds.classification = Some((
        bounds.classification.unwrap_or((0, u8::MAX)).0,
        bounds.classification.unwrap_or((0, u8::MAX)).1,
    ));
    attribute_bounds.scan_angle_rank = Some((
        bounds.scan_angle_rank.unwrap_or((-128, 127)).0,
        bounds.scan_angle_rank.unwrap_or((-128, 127)).1,
    ));
    attribute_bounds.user_data = Some((
        bounds.user_data.unwrap_or((0, u8::MAX)).0,
        bounds.user_data.unwrap_or((0, u8::MAX)).1,
    ));
    attribute_bounds.point_source_id = Some((
        bounds.point_source_id.unwrap_or((0, u16::MAX)).0,
        bounds.point_source_id.unwrap_or((0, u16::MAX)).1,
    ));
    attribute_bounds.gps_time = Some((
        bounds.gps_time.unwrap_or((f64::MIN, f64::MAX)).0,
        bounds.gps_time.unwrap_or((f64::MIN, f64::MAX)).1,
    ));
    attribute_bounds.color_r = Some((
        bounds.color_r.unwrap_or((0, u16::MAX)).0,
        bounds.color_r.unwrap_or((0, u16::MAX)).1,
    ));
    attribute_bounds.color_g = Some((
        bounds.color_g.unwrap_or((0, u16::MAX)).0,
        bounds.color_g.unwrap_or((0, u16::MAX)).1,
    ));
    attribute_bounds.color_b = Some((
        bounds.color_b.unwrap_or((0, u16::MAX)).0,
        bounds.color_b.unwrap_or((0, u16::MAX)).1,
    ));
    attribute_bounds
}

/// Classification Filter, that only accepts Ground Points (AHN4 Dataset)
pub fn ground_classification() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((2, 2)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

/// Classification Filter, that only accepts Points, NOT classified as cars (AHN4 dataset)
pub fn building_classification() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((6, 6)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

/// Intensity Filter, which only accepts Points with high intensity (AHN4 dataset)
pub fn high_intensity() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: Some((1268, 65535)),
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

/// Intensity Filter, which only accepts Points with low intensity (AHN4 dataset)
pub fn low_intensity() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: Some((0, 370)),
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

/// Number of Returns Filter, which only accepts Points with 1 or more returns (AHN4 dataset)
pub fn one_return() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: Some((2, 10)),
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

/// Time Filter which only accepts specific time range (around 20M points) (AHN4 dataset)
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
        gps_time: Some((270109229.0, 270109237.0)),
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

/// Normal Filter on UserData
/// (Modified AHN4 dataset)
/// Assumes, that NormalX is stored in UserData
/// Filters for upwards pointing x axis
pub fn normal_x_vertical() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: None,
        scan_angle_rank: None,
        user_data: Some((107, 147)),
        point_source_id: None,
        gps_time: None,
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

/// Mixed Filter, which only accepts points of a certain time range and a ground classification (Frankfurt dataset)
pub fn mixed_ground_and_time() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((2, 2)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: Some((270109229.0, 270109237.0)),
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

/// Mixed Filter, which only accepts ground points which have 1 or more returns (Frankfurt dataset)
pub fn mixed_ground_and_one_return() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: Some((2, 10)),
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((2, 2)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        color_r: None,
        color_g: None,
        color_b: None,
    }
}

/// Mixed Filter, which only accepts ground points, which have 1 or more returns and have a specific normal (Frankfurt dataset)
pub fn mixed_ground_normal_one_return() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: Some((2, 10)),
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((2, 2)),
        scan_angle_rank: None,
        user_data: Some((107, 147)),
        point_source_id: None,
        gps_time: None,
        color_r: None,
        color_g: None,
        color_b: None,
    }
}
