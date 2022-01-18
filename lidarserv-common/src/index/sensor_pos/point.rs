use crate::geometry::position::{I32Position, Position};
use crate::las::LasExtraBytes;

#[derive(Clone, Debug, Default)]
pub struct SensorPositionAttribute(pub I32Position);

impl LasExtraBytes for SensorPositionAttribute {
    const NR_EXTRA_BYTES: usize = 12;

    fn get_extra_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0; 12];
        let bytes_x = self.0.x().to_le_bytes();
        let bytes_y = self.0.y().to_le_bytes();
        let bytes_z = self.0.z().to_le_bytes();
        bytes[0..4].copy_from_slice(&bytes_x[..]);
        bytes[4..8].copy_from_slice(&bytes_y[..]);
        bytes[8..12].copy_from_slice(&bytes_z[..]);
        bytes
    }

    fn set_extra_bytes(&mut self, extra_bytes: &[u8]) {
        let mut bytes_x = [0; 4];
        let mut bytes_y = [0; 4];
        let mut bytes_z = [0; 4];
        bytes_x.copy_from_slice(&extra_bytes[0..4]);
        bytes_y.copy_from_slice(&extra_bytes[4..8]);
        bytes_z.copy_from_slice(&extra_bytes[8..12]);
        self.0 = I32Position::from_components(
            i32::from_le_bytes(bytes_x),
            i32::from_le_bytes(bytes_y),
            i32::from_le_bytes(bytes_z),
        );
    }
}
