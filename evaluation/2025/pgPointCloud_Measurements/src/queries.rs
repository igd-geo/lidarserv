use crate::attribute_bounds::LasPointAttributeBounds;

pub fn filter_apply_defaults(bounds : LasPointAttributeBounds) -> LasPointAttributeBounds {
    let mut attribute_bounds = LasPointAttributeBounds::new();
    attribute_bounds.intensity = Some((bounds.intensity.unwrap_or((0, u16::MAX)).0, bounds.intensity.unwrap_or((0, u16::MAX)).1));
    attribute_bounds.return_number = Some((bounds.return_number.unwrap_or((0, u8::MAX)).0, bounds.return_number.unwrap_or((0, u8::MAX)).1));
    attribute_bounds.number_of_returns = Some((bounds.number_of_returns.unwrap_or((0, u8::MAX)).0, bounds.number_of_returns.unwrap_or((0, u8::MAX)).1));
    attribute_bounds.scan_direction = Some((bounds.scan_direction.unwrap_or((false, true)).0, bounds.scan_direction.unwrap_or((false, true)).1));
    attribute_bounds.edge_of_flight_line = Some((bounds.edge_of_flight_line.unwrap_or((false, true)).0, bounds.edge_of_flight_line.unwrap_or((false, true)).1));
    attribute_bounds.classification = Some((bounds.classification.unwrap_or((0, u8::MAX)).0, bounds.classification.unwrap_or((0, u8::MAX)).1));
    attribute_bounds.scan_angle_rank = Some((bounds.scan_angle_rank.unwrap_or((-128, 127)).0, bounds.scan_angle_rank.unwrap_or((-128, 127)).1));
    attribute_bounds.user_data = Some((bounds.user_data.unwrap_or((0, u8::MAX)).0, bounds.user_data.unwrap_or((0, u8::MAX)).1));
    attribute_bounds.point_source_id = Some((bounds.point_source_id.unwrap_or((0, u16::MAX)).0, bounds.point_source_id.unwrap_or((0, u16::MAX)).1));
    attribute_bounds.gps_time = Some((bounds.gps_time.unwrap_or((f64::MIN, f64::MAX)).0, bounds.gps_time.unwrap_or((f64::MIN, f64::MAX)).1));
    attribute_bounds.synthetic = Some((bounds.synthetic.unwrap_or((false, true)).0, bounds.synthetic.unwrap_or((false, true)).1));
    attribute_bounds.key_point = Some((bounds.key_point.unwrap_or((false, true)).0, bounds.key_point.unwrap_or((false, true)).1));
    attribute_bounds.withheld = Some((bounds.withheld.unwrap_or((false, true)).0, bounds.withheld.unwrap_or((false, true)).1));
    attribute_bounds.overlap = Some((bounds.overlap.unwrap_or((false, true)).0, bounds.overlap.unwrap_or((false, true)).1));
    attribute_bounds
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
        gps_time: Some((270109229.0, 270109237.0)),// shifted by 270109201.59 in CloudCompare --> (27,41; 35,41)
        synthetic: None,
        key_point: None,
        withheld: None,
        overlap: None,
    }
}
pub fn time_range_patchwise() -> &'static str {
    return "pc_patchmin(pa, 'GpsTime') <= 270109237 AND pc_patchmax(pa, 'GpsTime') >= 270109229"
}
pub fn time_range_pointwise() -> &'static str {
    return "PC_FilterBetween(pa, 'GpsTime', 270109229, 270109237)"
}

/// Classification Filter, that only accepts Ground Points (AHN4 Dataset)
pub fn ground_classification() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((2,2)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        synthetic: None,
        key_point: None,
        withheld: None,
        overlap: None,
    }
}
pub fn ground_classification_pointwise() -> &'static str {
    return "PC_FilterEquals(pa, 'Classification', 2)"
}
pub fn ground_classification_patchwise() -> &'static str {
    return "pc_patchmin(pa, 'Classification') <= 2 AND pc_patchmax(pa, 'Classification') >= 2"
}

pub fn bridge_classification() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((26,26)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        synthetic: None,
        key_point: None,
        withheld: None,
        overlap: None,
    }
}
pub fn bridge_classification_pointwise() -> &'static str {
    return "PC_FilterEquals(pa, 'Classification', 26)"
}
pub fn bridge_classification_patchwise() -> &'static str {
    return "pc_patchmin(pa, 'Classification') <= 26 AND pc_patchmax(pa, 'Classification') >= 26"
}

/// Classification Filter, that only accepts Points, NOT classified as cars (AHN4 dataset)
pub fn building_classification() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((6,6)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        synthetic: None,
        key_point: None,
        withheld: None,
        overlap: None,
    }
}
pub fn building_classification_pointwise() -> &'static str {
    return "PC_FilterEquals(pa, 'Classification', 6)"
}
pub fn building_classification_patchwise() -> &'static str {
    return "pc_patchmin(pa, 'Classification') <= 6 AND pc_patchmax(pa, 'Classification') >= 6"
}

/// Classification Filter, that only accepts Points, NOT classified as cars (AHN4 dataset)
pub fn vegetation_classification() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((1,1)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: None,
        synthetic: None,
        key_point: None,
        withheld: None,
        overlap: None,
    }
}
pub fn vegetation_classification_pointwise() -> &'static str {
    return "PC_FilterEquals(pa, 'Classification', 1)"
}
pub fn vegetation_classification_patchwise() -> &'static str {
    return "pc_patchmin(pa, 'Classification') <= 1 AND pc_patchmax(pa, 'Classification') >= 1"
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
        synthetic: None,
        key_point: None,
        withheld: None,
        overlap: None,
    }
}
pub fn high_intensity_pointwise() -> &'static str {
    return "PC_FilterBetween(pa, 'Intensity', 1268, 65535)"
}
pub fn high_intensity_patchwise() -> &'static str {
    return "pc_patchmin(pa, 'Intensity') <= 65535 AND pc_patchmax(pa, 'Intensity') >= 1268"
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
        synthetic: None,
        key_point: None,
        withheld: None,
        overlap: None,
    }
}
pub fn low_intensity_pointwise() -> &'static str {
    return "PC_FilterBetween(pa, 'Intensity', 0, 370)"
}
pub fn low_intensity_patchwise() -> &'static str {
    return "pc_patchmin(pa, 'Intensity') <= 370 AND pc_patchmax(pa, 'Intensity') >= 0"
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
        synthetic: None,
        key_point: None,
        withheld: None,
        overlap: None,
    }
}
pub fn normal_x_vertical_pointwise() -> &'static str {
    return "PC_FilterBetween(pa, 'UserData', 107, 147)"
}
pub fn normal_x_vertical_patchwise() -> &'static str {
    return "pc_patchmin(pa, 'UserData') <= 147 AND pc_patchmax(pa, 'UserData') >= 107"
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
        synthetic: None,
        key_point: None,
        withheld: None,
        overlap: None,
    }
}
pub fn one_return_pointwise() -> &'static str {
    return "PC_FilterBetween(pa, 'NumberOfReturns', 2, 10)"
}
pub fn one_return_patchwise() -> &'static str {
    return "pc_patchmin(pa, 'NumberOfReturns') <= 10 AND pc_patchmax(pa, 'NumberOfReturns') >= 2"
}

/// Mixed Filter, which only accepts points of a certain time range and a ground classification (Frankfurt dataset)
pub fn mixed_ground_and_time() -> LasPointAttributeBounds {
    LasPointAttributeBounds {
        intensity: None,
        return_number: None,
        number_of_returns: None,
        scan_direction: None,
        edge_of_flight_line: None,
        classification: Some((2,2)),
        scan_angle_rank: None,
        user_data: None,
        point_source_id: None,
        gps_time: Some((270109229.0, 270109237.0)),
        synthetic: None,
        key_point: None,
        withheld: None,
        overlap: None,
    }
}
pub fn mixed_ground_and_time_pointwise() -> &'static str {
    return "PC_FilterEquals(PC_FilterBetween(pa, 'GpsTime', 270109229, 270109237), 'Classification', 2)"
}
pub fn mixed_ground_and_time_patchwise() -> &'static str {
    return "pc_patchmin(pa, 'GpsTime') <= 270109237 AND pc_patchmax(pa, 'GpsTime') >= 270109229 AND pc_patchmin(pa, 'Classification') <= 2 AND pc_patchmax(pa, 'Classification') >= 2"
}