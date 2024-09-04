pub mod basic_flags;
pub mod copy;
pub mod extended_flags;
pub mod position;
pub mod scan_angle;

pub trait AttributeExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]);
}
