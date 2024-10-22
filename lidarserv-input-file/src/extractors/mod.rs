pub mod basic_flags;
pub mod basic_flags_downgrade;
pub mod classification_flags;
pub mod copy;
pub mod edge_of_flight_line;
pub mod extended_flags;
pub mod extended_flags_upgrade;
pub mod init_zero;
pub mod number_of_returns_3bit;
pub mod number_of_returns_4bit;
pub mod position;
pub mod return_number_3bit;
pub mod return_number_4bit;
pub mod scan_angle;
pub mod scan_angle_rank;
pub mod scan_direction_flag;
pub mod scanner_channel;

pub trait AttributeExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]);
}
