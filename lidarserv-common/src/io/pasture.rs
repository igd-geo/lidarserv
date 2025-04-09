//! A data format that can store arbitrary pasture buffers.
//!
//!  # Format description
//!
//! ## Header
//!
//!  - field 1, str, 16 bytes: magic number. This is always the fixed string "lidarserv points" (utf-8)
//!  - field 2, u8, 1 byte: version number. Currently, there is only version 1.
//!  - field 3, u8, 1 byte: endianess used to store the point data.
//!     (All fields in the header are little endian, regardless of this
//!     field. It only affects the storage of the point attributes.)
//!     0 == Little Endian
//!     1 == Big Endian
//!  - field 4, u8, 1 byte: compression of the point data
//!     0 == uncompressed
//!     1 == lz4
//!  - field 5, u64 le, 8 bytes: number of points
//!  - field 6, u8, 1 byte: number of point attributes
//!  - field 7: Point Attribute headers. Each attribute has these fields:
//!     - field 7.1, u8, 1 byte: length of attribute name in bytes
//!     - field 7.2, str, variable length: attribute name (utf-8, of length specified in field 7.1)
//!     - field 7.3, u64 le, 8 bytes: size of attribute values
//!     - field 7.4, u8, 1 byte: attribute type
//!         0 == U8
//!         1 == I8
//!         2 == U16
//!         3 == I16
//!         4 == U32
//!         5 == I32
//!         6 == U64
//!         7 == I64
//!         8 == F32
//!         9 == F64
//!         10 == Vec3u8
//!         11 == Vec3u16
//!         12 == Vec3f32
//!         13 == Vec3i32
//!         14 == Vec3f64
//!         15 == Vec4u8
//!         16 == ByteArray
//!
//! ## Uncompressed Point data
//!
//! Each point is the concatenation of its attributes. No padding between the attributes ("packed layout").
//!
//! ## Lz4 compressed point data.
//!
//! Conceptually, each point also is the concatenation of its attribute, just like the packed layout representation
//! also used for the uncompressed point data.
//!
//! However, compressed point data is stored in an interleaved format.
//! Meaning, that first the first byte of each point is compressed,
//! then the second byte of each point (in a new compression context)
//! then the third byte of each point, and so on....
//!
//! This means, there will be a total of `point_size` compression contexts,
//! where `point_size` is the sum of all attribute sizes as defined in the header.
//!
//! for i in 0..point_size:
//!     - field 1, u64 le, 8 byte: compressed size
//!     - field 2, data: lz4-compressed data of specified size.
//!                      The uncompressed size of this chunk is `nr_points` bytes long.
//!                      It is the concatenation of the i'th byte of each point.

use super::{PointCodec, PointIoError};
use anyhow::Result;
use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use nalgebra::{SVector, Vector3, Vector4};
use pasture_core::{
    containers::{
        BorrowedBuffer, InterleavedBuffer, InterleavedBufferMut, OwningBuffer, VectorBuffer,
    },
    layout::{
        PointAttributeDataType, PointAttributeDefinition, PointLayout,
        conversion::BufferLayoutConverter,
    },
};
use std::borrow::Cow;

#[derive(Debug, Copy, Clone, Default, Eq, PartialEq)]
pub enum Endianess {
    #[default]
    LittleEndian,
    BigEndian,
}

impl Endianess {
    const NATIVE_ENDIANESS: Endianess = if cfg!(target_endian = "big") {
        Endianess::BigEndian
    } else {
        Endianess::LittleEndian
    };
}

#[derive(Debug, Copy, Clone, Default, Eq, PartialEq)]
pub enum Compression {
    #[default]
    None,
    Lz4,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct AttributeHeader {
    /// attribute name
    pub name: String,

    /// datatype
    pub datatype: PointAttributeDataType,
}

impl AttributeHeader {
    pub fn new(attr: &PointAttributeDefinition) -> Self {
        AttributeHeader {
            name: attr.name().to_string(),
            datatype: attr.datatype(),
        }
    }

    pub fn read(rd: &mut impl std::io::Read) -> Result<AttributeHeader, PointIoError> {
        // name
        let name_len = rd.read_u8()?;
        let mut name_buf = vec![0; name_len as usize];
        rd.read_exact(&mut name_buf)?;
        let name = String::from_utf8(name_buf).map_err(|_| {
            PointIoError::DataFormat("attribute name is not valid utf8.".to_string())
        })?;

        // length
        let len = rd.read_u64::<LittleEndian>()?;

        // type
        let datatype = match rd.read_u8()? {
            0 => PointAttributeDataType::U8,
            1 => PointAttributeDataType::I8,
            2 => PointAttributeDataType::U16,
            3 => PointAttributeDataType::I16,
            4 => PointAttributeDataType::U32,
            5 => PointAttributeDataType::I32,
            6 => PointAttributeDataType::U64,
            7 => PointAttributeDataType::I64,
            8 => PointAttributeDataType::F32,
            9 => PointAttributeDataType::F64,
            10 => PointAttributeDataType::Vec3u8,
            11 => PointAttributeDataType::Vec3u16,
            12 => PointAttributeDataType::Vec3f32,
            13 => PointAttributeDataType::Vec3i32,
            14 => PointAttributeDataType::Vec3f64,
            15 => PointAttributeDataType::Vec4u8,
            16 => PointAttributeDataType::ByteArray(len),
            _ => {
                return Err(PointIoError::DataFormat(
                    "Invalid point attribute datatype in header.".to_string(),
                ));
            }
        };
        if datatype.size() != len {
            return Err(PointIoError::DataFormat(
                "Invalid point attribute len in header.".to_string(),
            ));
        }

        Ok(AttributeHeader { name, datatype })
    }

    pub fn write(&self, wr: &mut impl std::io::Write) -> Result<(), PointIoError> {
        // name
        let name_buf = self.name.as_bytes();
        wr.write_u8(name_buf.len() as u8)?;
        wr.write_all(name_buf)?;

        // length
        wr.write_u64::<LittleEndian>(self.datatype.size())?;

        // type
        wr.write_u8(match self.datatype {
            PointAttributeDataType::U8 => 0,
            PointAttributeDataType::I8 => 1,
            PointAttributeDataType::U16 => 2,
            PointAttributeDataType::I16 => 3,
            PointAttributeDataType::U32 => 4,
            PointAttributeDataType::I32 => 5,
            PointAttributeDataType::U64 => 6,
            PointAttributeDataType::I64 => 7,
            PointAttributeDataType::F32 => 8,
            PointAttributeDataType::F64 => 9,
            PointAttributeDataType::Vec3u8 => 10,
            PointAttributeDataType::Vec3u16 => 11,
            PointAttributeDataType::Vec3f32 => 12,
            PointAttributeDataType::Vec3i32 => 13,
            PointAttributeDataType::Vec3f64 => 14,
            PointAttributeDataType::Vec4u8 => 15,
            PointAttributeDataType::ByteArray(_) => 16,
            PointAttributeDataType::Custom { .. } => {
                return Err(PointIoError::Unsupported(
                    "The point attribute data type `PointAttributeDataType::Custom` is not supported.".to_string(),
                ));
            }
        })?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
struct Header {
    pub version: u8,
    pub endianess: Endianess,
    pub compression: Compression,
    pub nr_points: u64,
    pub attributes: Vec<AttributeHeader>,
}

impl Header {
    const MAGIC_NUMBER: &'static str = "lidarserv points";

    pub fn read(rd: &mut impl std::io::Read) -> Result<Header, PointIoError> {
        // magic number
        let mut magic_buf = [0; 16];
        rd.read_exact(&mut magic_buf)?;
        if std::str::from_utf8(&magic_buf) != Ok(Self::MAGIC_NUMBER) {
            return Err(PointIoError::DataFormat(
                "This is not a lidarserv points file.".to_string(),
            ));
        }

        // version number
        let version = rd.read_u8()?;
        if version != 1 {
            return Err(PointIoError::DataFormat(format!(
                "Wrong version: {version} (expected version 1)"
            )));
        }

        // endianess
        let endianess = match rd.read_u8()? {
            0 => Endianess::LittleEndian,
            1 => Endianess::BigEndian,
            _ => {
                return Err(PointIoError::DataFormat(
                    "Invalid endianess in header.".to_string(),
                ));
            }
        };

        // compression
        let compression = match rd.read_u8()? {
            0 => Compression::None,
            1 => Compression::Lz4,
            _ => {
                return Err(PointIoError::DataFormat(
                    "Invalid compression in header.".to_string(),
                ));
            }
        };

        // point count
        let nr_points = rd.read_u64::<LittleEndian>()?;

        // layout
        let nr_attributes = rd.read_u8()?;
        let mut attributes = Vec::new();
        for _ in 0..nr_attributes {
            let attribute = AttributeHeader::read(rd)?;
            attributes.push(attribute);
        }

        Ok(Header {
            version,
            endianess,
            compression,
            nr_points,
            attributes,
        })
    }

    pub fn write(&self, wr: &mut impl std::io::Write) -> Result<(), PointIoError> {
        // check version
        // (currently version 1 is the only existing one)
        if self.version != 1 {
            return Err(PointIoError::Unsupported(format!(
                "Wrong version: {} (expected version 1)",
                self.version
            )));
        }
        if self.attributes.len() > 255 {
            return Err(PointIoError::Unsupported(
                "Too many attributes. (max 255).".to_string(),
            ));
        }

        // write magic number
        wr.write_all(Self::MAGIC_NUMBER.as_bytes())?;

        // version number
        wr.write_u8(self.version)?;

        // endianess
        wr.write_u8(match self.endianess {
            Endianess::LittleEndian => 0,
            Endianess::BigEndian => 1,
        })?;

        // compression
        wr.write_u8(match self.compression {
            Compression::None => 0,
            Compression::Lz4 => 1,
        })?;

        // nr points
        wr.write_u64::<LittleEndian>(self.nr_points)?;

        // layout
        let nr_attributes: u8 = self
            .attributes
            .len()
            .try_into()
            .expect("tested at the top, that nr_attributes <= 255");
        wr.write_u8(nr_attributes)?;
        for attr in &self.attributes {
            attr.write(wr)?;
        }

        Ok(())
    }
}

fn write_point_data_uncompressed(
    points: &impl BorrowedBuffer,
    header: &Header,
    write: &mut impl std::io::Write,
) -> Result<(), PointIoError> {
    // layout that should be used for writing to file:
    // Densely pack the attributes, no padding, interleaved
    let point_attrs = header
        .attributes
        .iter()
        .map(|a| PointAttributeDefinition::custom(Cow::Owned(a.name.clone()), a.datatype))
        .collect::<Vec<_>>();
    let packed_layout = PointLayout::from_attributes_packed(&point_attrs, 1);

    // convert layout
    let points_converted: VectorBuffer;
    let point_data = match points.as_interleaved() {
        // fast path if the point layout is already correct
        Some(interleaved) if *interleaved.point_layout() == packed_layout => {
            interleaved.get_point_range_ref(0..interleaved.len())
        }

        // slow path: convert to target layout
        _ => {
            let converter =
                BufferLayoutConverter::for_layouts(points.point_layout(), &packed_layout);
            points_converted = converter.convert(points);
            points_converted.get_point_range_ref(0..points_converted.len())
        }
    };

    // check size
    let point_size = header
        .attributes
        .iter()
        .map(|attr| attr.datatype.size() as usize)
        .sum::<usize>();
    let nr_points = header.nr_points as usize;
    assert!(point_data.len() == point_size * nr_points);

    write.write_all(point_data)?;
    Ok(())
}

trait Endian {
    fn convert_to_native_endianess<B: ByteOrder>(src: &[u8], dst: &mut [u8]);
}

macro_rules! impl_endian_single_byte {
    ($t:ty) => {
        impl Endian for $t {
            fn convert_to_native_endianess<B: ByteOrder>(src: &[u8], dst: &mut [u8]) {
                assert_eq!(src.len(), 1);
                assert_eq!(dst.len(), 1);
                dst.copy_from_slice(src);
            }
        }
    };
}
macro_rules! impl_endian_int {
    ($t:ty, $rd:ident) => {
        impl Endian for $t {
            fn convert_to_native_endianess<B: ByteOrder>(src: &[u8], dst: &mut [u8]) {
                assert_eq!(src.len(), std::mem::size_of::<$t>());
                assert_eq!(dst.len(), std::mem::size_of::<$t>());
                let int = B::$rd(&src);
                dst.copy_from_slice(&int.to_ne_bytes())
            }
        }
    };
}

impl_endian_single_byte!(u8);
impl_endian_single_byte!(i8);
impl_endian_int!(u16, read_u16);
impl_endian_int!(u32, read_u32);
impl_endian_int!(u64, read_u64);
impl_endian_int!(i16, read_i16);
impl_endian_int!(i32, read_i32);
impl_endian_int!(i64, read_i64);
impl_endian_int!(f32, read_f32);
impl_endian_int!(f64, read_f64);

impl<T, const D: usize> Endian for SVector<T, D>
where
    T: Endian,
{
    fn convert_to_native_endianess<B: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let s = std::mem::size_of::<T>();
        assert_eq!(src.len(), s * D);
        assert_eq!(dst.len(), s * D);
        for d in 0..D {
            let l = d * s;
            let r = d * s + s;
            T::convert_to_native_endianess::<B>(&src[l..r], &mut dst[l..r]);
        }
    }
}

fn byteorder_convert_fn(header: &Header, attr: &AttributeHeader) -> fn(&[u8], &mut [u8]) {
    fn convert_fn_for_type<E: ByteOrder>(datatype: PointAttributeDataType) -> fn(&[u8], &mut [u8]) {
        match datatype {
            PointAttributeDataType::U8 => u8::convert_to_native_endianess::<E>,
            PointAttributeDataType::I8 => i8::convert_to_native_endianess::<E>,
            PointAttributeDataType::U16 => u16::convert_to_native_endianess::<E>,
            PointAttributeDataType::I16 => i16::convert_to_native_endianess::<E>,
            PointAttributeDataType::U32 => u32::convert_to_native_endianess::<E>,
            PointAttributeDataType::I32 => i32::convert_to_native_endianess::<E>,
            PointAttributeDataType::U64 => u64::convert_to_native_endianess::<E>,
            PointAttributeDataType::I64 => i64::convert_to_native_endianess::<E>,
            PointAttributeDataType::F32 => f32::convert_to_native_endianess::<E>,
            PointAttributeDataType::F64 => f64::convert_to_native_endianess::<E>,
            PointAttributeDataType::Vec3u8 => Vector3::<u8>::convert_to_native_endianess::<E>,
            PointAttributeDataType::Vec3u16 => Vector3::<u16>::convert_to_native_endianess::<E>,
            PointAttributeDataType::Vec3f32 => Vector3::<f32>::convert_to_native_endianess::<E>,
            PointAttributeDataType::Vec3i32 => Vector3::<i32>::convert_to_native_endianess::<E>,
            PointAttributeDataType::Vec3f64 => Vector3::<f64>::convert_to_native_endianess::<E>,
            PointAttributeDataType::Vec4u8 => Vector4::<u8>::convert_to_native_endianess::<E>,
            PointAttributeDataType::ByteArray(_) => {
                |src: &[u8], dst: &mut [u8]| dst.copy_from_slice(src)
            }
            PointAttributeDataType::Custom { .. } => {
                |src: &[u8], dst: &mut [u8]| dst.copy_from_slice(src)
            }
        }
    }

    if header.endianess == Endianess::NATIVE_ENDIANESS {
        |slice_rd: &[u8], slice_wr: &mut [u8]| slice_wr.copy_from_slice(slice_rd)
    } else {
        match header.endianess {
            Endianess::LittleEndian => convert_fn_for_type::<LittleEndian>(attr.datatype),
            Endianess::BigEndian => convert_fn_for_type::<BigEndian>(attr.datatype),
        }
    }
}

fn read_point_data_uncompressed(
    header: &Header,
    layout: PointLayout,
    rd: &mut impl std::io::Read,
) -> Result<VectorBuffer, PointIoError> {
    // read
    let point_size = header
        .attributes
        .iter()
        .map(|attr| attr.datatype.size() as usize)
        .sum::<usize>();
    let nr_points = header.nr_points as usize;
    let read_size = point_size * nr_points;
    let mut point_data = vec![0; read_size];
    rd.read_exact(&mut point_data)?;
    let point_data = point_data;

    // to pasture
    let mut result = VectorBuffer::with_capacity(nr_points, layout);
    result.resize(nr_points);
    let mut conversions = Vec::new();
    let mut offset = 0;
    for attr in &header.attributes {
        let size = attr.datatype.size() as usize;
        let byte_range_in_data = offset..offset + size;
        let byte_range_in_layout = result
            .point_layout()
            .get_attribute(&PointAttributeDefinition::custom(
                Cow::Owned(attr.name.clone()),
                attr.datatype,
            ))
            .expect("layout missmatch") // expect: before calling this (private) function, we ensure that the layout matches.
            .byte_range_within_point();
        let conversion = byteorder_convert_fn(header, attr);
        conversions.push((byte_range_in_data, byte_range_in_layout, conversion));
        offset += size;
    }
    for point in 0..nr_points {
        let pasture_point_data = result.get_point_mut(point);
        let file_point_data = &point_data[point * point_size..point * point_size + point_size];
        for (byte_range_in_data, byte_range_in_layout, conversion) in &conversions {
            let file_attr_data = &file_point_data[byte_range_in_data.clone()];
            let pasture_attr_data = &mut pasture_point_data[byte_range_in_layout.clone()];
            conversion(file_attr_data, pasture_attr_data);
        }
    }

    Ok(result)
}

fn write_point_data_lz4(
    points: &impl BorrowedBuffer,
    header: &Header,
    wr: &mut impl std::io::Write,
) -> Result<(), PointIoError> {
    let point_size = header
        .attributes
        .iter()
        .map(|a| a.datatype.size())
        .sum::<u64>() as usize;
    let nr_points = header.nr_points as usize;

    let offsets: Vec<usize> = header
        .attributes
        .iter()
        .flat_map(|a| {
            points
                .point_layout()
                .get_attribute(&PointAttributeDefinition::custom(
                    Cow::Owned(a.name.clone()),
                    a.datatype,
                ))
                .expect("missing attribute")
                .byte_range_within_point()
        })
        .collect();
    let mut transposed = vec![Vec::new(); point_size];
    let mut point_buffer = vec![0; points.point_layout().size_of_point_entry() as usize];
    for point in 0..nr_points {
        points.get_point(point, &mut point_buffer);
        for byte in 0..point_size {
            let offset = offsets[byte];
            let value = point_buffer[offset];
            transposed[byte].push(value);
        }
    }

    for data in transposed {
        let compressed = lz4::block::compress(&data, None, false)?;
        let compressed_size = compressed.len() as u64;
        wr.write_u64::<LittleEndian>(compressed_size)?;
        wr.write_all(&compressed)?;
    }
    Ok(())
}

fn read_point_data_lz4(
    header: &Header,
    layout: PointLayout,
    rd: &mut impl std::io::Read,
) -> Result<VectorBuffer, PointIoError> {
    let point_size = header
        .attributes
        .iter()
        .map(|a| a.datatype.size())
        .sum::<u64>() as usize;
    let nr_points = header.nr_points as usize;

    let mut interleaved = Vec::new();
    for _ in 0..point_size {
        let compressed_size = rd.read_u64::<LittleEndian>()? as usize;
        let mut compressed = vec![0; compressed_size];
        rd.read_exact(&mut compressed)?;
        let uncompressed = lz4::block::decompress(&compressed, Some(nr_points as i32))?;
        interleaved.push(uncompressed);
    }

    let mut result = VectorBuffer::with_capacity(nr_points, layout);
    let mut point_buf = vec![0; point_size];
    result.resize(nr_points);
    let mut conversions = Vec::new();
    let mut offset = 0;
    for attr in &header.attributes {
        let size = attr.datatype.size() as usize;
        let byte_range_in_data = offset..offset + size;
        let byte_range_in_layout = result
            .point_layout()
            .get_attribute(&PointAttributeDefinition::custom(
                Cow::Owned(attr.name.clone()),
                attr.datatype,
            ))
            .expect("layout missmatch") // expect: before calling this function, we ensure that the layout matches.
            .byte_range_within_point();
        let conversion = byteorder_convert_fn(header, attr);
        conversions.push((byte_range_in_data, byte_range_in_layout, conversion));
        offset += size;
    }
    for point in 0..nr_points {
        for byte in 0..point_size {
            point_buf[byte] = interleaved[byte][point];
        }
        let pasture_point_data = result.get_point_mut(point);
        for (byte_range_in_data, byte_range_in_layout, conversion) in &conversions {
            let attr_data = &point_buf[byte_range_in_data.clone()];
            let pasture_attr_data = &mut pasture_point_data[byte_range_in_layout.clone()];
            conversion(attr_data, pasture_attr_data);
        }
    }
    Ok(result)
}

/// Writes the pasture buffer to the file.
pub fn write_points<B: BorrowedBuffer>(
    points: &B,
    compression: Compression,
    wr: &mut impl std::io::Write,
) -> Result<(), PointIoError> {
    let endianess = if cfg!(target_endian = "big") {
        Endianess::BigEndian
    } else {
        Endianess::LittleEndian
    };
    let attributes = points
        .point_layout()
        .attributes()
        .map(|a| AttributeHeader::new(a.attribute_definition()))
        .collect();

    let header = Header {
        version: 1,
        endianess,
        compression,
        nr_points: points.len() as u64,
        attributes,
    };
    header.write(wr)?;

    match compression {
        Compression::None => write_point_data_uncompressed(points, &header, wr)?,
        Compression::Lz4 => write_point_data_lz4(points, &header, wr)?,
    }
    Ok(())
}

/// Reads points from the file and returns them as a pasture buffer.
pub fn read_points(
    layout: &PointLayout,
    rd: &mut impl std::io::Read,
) -> Result<VectorBuffer, PointIoError> {
    // read header
    let header = Header::read(rd)?;

    // check attributes
    let attribute_error = PointIoError::PointLayoutMismatch {
        expected: layout
            .attributes()
            .map(|a| a.attribute_definition().clone())
            .collect(),
        actual: header
            .attributes
            .iter()
            .map(|attr| {
                PointAttributeDefinition::custom(Cow::Owned(attr.name.clone()), attr.datatype)
            })
            .collect(),
    };
    if header.attributes.len() != layout.attributes().count() {
        return Err(attribute_error);
    }
    for attr in &header.attributes {
        let attribute =
            PointAttributeDefinition::custom(Cow::Owned(attr.name.clone()), attr.datatype);
        if !layout.has_attribute(&attribute) {
            return Err(attribute_error);
        }
    }
    match header.compression {
        Compression::None => read_point_data_uncompressed(&header, layout.clone(), rd),
        Compression::Lz4 => read_point_data_lz4(&header, layout.clone(), rd),
    }
}

pub struct PastureIo {
    compression: Compression,
}

impl PastureIo {
    pub fn new(compression: Compression) -> Self {
        Self { compression }
    }
}

impl PointCodec for PastureIo {
    fn write_points(
        &self,
        points: &VectorBuffer,
        wr: &mut impl std::io::Write,
    ) -> std::result::Result<(), PointIoError> {
        write_points(points, self.compression, wr)
    }

    fn read_points(
        &self,
        rd: &mut impl std::io::Read,
        point_layout: &PointLayout,
    ) -> std::result::Result<VectorBuffer, PointIoError> {
        read_points(point_layout, rd)
    }

    fn is_compatible_with(&self, _other: &Self) -> bool {
        // self.compression does not need to match,
        // because the compression parameter only affects the writer.
        // we can always read point data of any compression.
        true
    }
}

#[cfg(test)]
mod tests {

    use bytemuck::{Pod, Zeroable};
    use nalgebra::Vector3;
    use pasture_core::{
        containers::{BorrowedBuffer, VectorBuffer},
        layout::{
            PointAttributeDataType,
            attributes::{CLASSIFICATION, COLOR_RGB, INTENSITY, POSITION_3D},
        },
    };

    use super::{AttributeHeader, Compression, Endianess, Header, read_points, write_points};

    #[test]
    fn test_attribute_header_rw() {
        let test_values = [
            AttributeHeader::new(&POSITION_3D),
            AttributeHeader::new(&INTENSITY),
            AttributeHeader::new(&COLOR_RGB),
            AttributeHeader::new(&CLASSIFICATION),
            AttributeHeader {
                name: "extra_bytes".to_string(),
                datatype: PointAttributeDataType::ByteArray(17),
            },
        ];
        for test_attribute in test_values {
            // write to buffer
            let mut buf = Vec::new();
            test_attribute.write(&mut buf).unwrap();

            // read back from buffer
            let read_back = AttributeHeader::read(&mut buf.as_slice()).unwrap();
            assert_eq!(test_attribute, read_back);
        }
    }

    #[test]
    fn test_header_rw() {
        let test_values = [
            Header {
                version: 1,
                endianess: Endianess::LittleEndian,
                compression: Compression::None,
                nr_points: 1234,
                attributes: vec![],
            },
            Header {
                version: 1,
                endianess: Endianess::BigEndian,
                compression: Compression::None,
                nr_points: 0,
                attributes: vec![AttributeHeader {
                    name: "position_3d".into(),
                    datatype: PointAttributeDataType::Vec3f64,
                }],
            },
            Header {
                version: 1,
                endianess: Endianess::LittleEndian,
                compression: Compression::Lz4,
                nr_points: 50_000,
                attributes: vec![
                    AttributeHeader {
                        name: "position_3d".into(),
                        datatype: PointAttributeDataType::Vec3i32,
                    },
                    AttributeHeader {
                        name: "intensity".into(),
                        datatype: PointAttributeDataType::F32,
                    },
                    AttributeHeader {
                        name: "classification".into(),
                        datatype: PointAttributeDataType::U8,
                    },
                ],
            },
        ];

        for header in test_values {
            // write to buffer
            let mut buf = Vec::new();
            header.write(&mut buf).unwrap();

            // read back
            let read_back = Header::read(&mut buf.as_slice()).unwrap();
            assert_eq!(header, read_back);
        }
    }

    #[repr(C)]
    #[derive(Debug, Copy, Clone, PartialEq, pasture_derive::PointType, Pod, Zeroable)]
    struct TestPoint {
        #[pasture(BUILTIN_POSITION_3D)]
        position: Vector3<i32>,
        #[pasture(BUILTIN_INTENSITY)]
        intensity: f32,
    }

    #[test]
    fn test_points_rw() {
        let points: VectorBuffer = [
            TestPoint {
                position: Vector3::new(1, 2, 3),
                intensity: 1.0,
            },
            TestPoint {
                position: Vector3::new(4, 5, 6),
                intensity: 2.0,
            },
            TestPoint {
                position: Vector3::new(7, 8, 9),
                intensity: 3.0,
            },
        ]
        .into_iter()
        .collect();

        // uncompressed
        {
            // write
            let mut buffer = Vec::new();
            write_points(&points, Compression::None, &mut buffer).unwrap();

            // read back
            let read_back = read_points(points.point_layout(), &mut buffer.as_slice()).unwrap();
            assert_eq!(points, read_back);
        }

        // compressed
        {
            // write
            let mut buffer = Vec::new();
            write_points(&points, Compression::Lz4, &mut buffer).unwrap();

            // read back
            let read_back = read_points(points.point_layout(), &mut buffer.as_slice()).unwrap();
            assert_eq!(points, read_back);
        }
    }
}
