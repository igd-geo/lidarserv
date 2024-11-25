pub struct LasPointAttributeBounds {
    pub intensity: Option<(u16, u16)>,
    pub return_number: Option<(u8, u8)>,
    pub number_of_returns: Option<(u8, u8)>,
    pub scan_direction: Option<(bool, bool)>,
    pub edge_of_flight_line: Option<(bool, bool)>,
    pub classification: Option<(u8, u8)>,
    pub scan_angle_rank: Option<(i8, i8)>,
    pub user_data: Option<(u8, u8)>,
    pub point_source_id: Option<(u16, u16)>,
    pub gps_time: Option<(f64, f64)>,
    pub synthetic: Option<(bool, bool)>,
    pub key_point: Option<(bool, bool)>,
    pub withheld: Option<(bool, bool)>,
    pub overlap: Option<(bool, bool)>,
}

impl LasPointAttributeBounds {
    /// Creates a new attribute bounds
    pub fn new() -> Self {
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
            synthetic: None,
            key_point: None,
            withheld: None,
            overlap: None,
        }
    }
}