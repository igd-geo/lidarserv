use crate::{F64Position, PointType};
use anyhow::anyhow;
use anyhow::Result;
use lidarserv_server::index::point::GlobalPoint;
use std::ffi::OsStr;
use std::io::SeekFrom::{Current, Start};
use std::io::{BufRead, BufReader, ErrorKind, Read, Seek};
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PclReadError {
    #[error("Unexpected end of file")]
    UnexpectedEOF,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(not(feature = "file-lock"))]
pub fn read_pcd_file(filename: &OsStr) -> Result<Vec<GlobalPoint>, PclReadError> {
    let mut file = File::open(filename).map_err(|e| anyhow!(e))?;
    let mut file = BufReader::new(file);
    let header = PclHeader::read(&mut file)?;
    let body = header.read_data(&mut file)?;
    Ok(body)
}

#[cfg(feature = "file-lock")]
pub fn read_pcd_file(
    filename: &OsStr,
    origin: (f64, f64, f64),
) -> Result<Vec<GlobalPoint>, PclReadError> {
    use file_locker::FileLock;
    let mut lock = FileLock::new(filename)
        .blocking(true)
        .lock()
        .map_err(|e| anyhow!(e))?;
    let mut file = BufReader::new(&mut lock.file);
    let header = PclHeader::read(&mut file)?;
    let body = header.read_data(&mut file, origin)?;
    lock.unlock().map_err(|e| anyhow!(e))?;
    Ok(body)
}

#[allow(dead_code)]
// suppress warnings 'field is never read' - we are indeed not
// interested in all fields, but these are the fields as defined by the PCL header.
#[derive(Debug)]
pub struct PclHeader {
    version: String,
    fields: Vec<PclField>,
    width: u32,
    height: u32,
    viewpoint: String,
    points: u32,
    data: PclEncoding,
    data_start_pos: u64,
}

#[derive(Debug)]
pub struct PclField {
    field: String,
    size: u32,
    typ: PclType,
    count: u32,
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum PclType {
    Integer,
    Unsigned,
    Float,
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum PclEncoding {
    Ascii,
    Binary,
    BinaryCompressed,
}

pub struct TypeInfo<'a> {
    offset: usize,
    stride: usize,
    index: usize,
    field: &'a PclField,
}

impl PclHeader {
    fn read_pcl_header_entry(mut f: impl BufRead) -> Result<(String, String), PclReadError> {
        loop {
            // read a line
            let mut line = String::new();
            let chars = f.read_line(&mut line).map_err(|e| anyhow!(e))?;
            if chars == 0 || !line.ends_with('\n') {
                return Err(PclReadError::UnexpectedEOF);
            }

            // remove trailing comments
            let statement = if let Some((stmnt, _comment)) = line.split_once('#') {
                stmnt
            } else {
                &line
            }
            .trim();

            // skip blank lines
            if statement.is_empty() {
                continue;
            }

            // split key / value
            let (key, val) = match statement.split_once(' ') {
                None => {
                    return Err(PclReadError::Other(anyhow!(
                        "Invalid header field - missing seperator between key and value."
                    )))
                }
                Some(x) => x,
            };
            return Ok((key.to_string(), val.to_string()));
        }
    }

    fn expect_entry(f: impl BufRead, entry: &'static str) -> Result<String, PclReadError> {
        let (k, v) = Self::read_pcl_header_entry(f)?;
        if k != entry {
            Err(anyhow!("Expected header entry {} - but found {}", entry, k).into())
        } else {
            Ok(v)
        }
    }

    pub fn read(mut f: impl BufRead + Seek) -> Result<Self, PclReadError> {
        // read all fields
        let version = Self::expect_entry(&mut f, "VERSION")?;
        if version != "0.7" {
            return Err(anyhow!(
                "Version {} is not supported. Currently, only pcd version 0.7 is supported.",
                version
            )
            .into());
        }

        let fields = Self::expect_entry(&mut f, "FIELDS")?;
        let size = Self::expect_entry(&mut f, "SIZE")?;
        let typ = Self::expect_entry(&mut f, "TYPE")?;
        let count = Self::expect_entry(&mut f, "COUNT")?;
        let width = Self::expect_entry(&mut f, "WIDTH")?;
        let height = Self::expect_entry(&mut f, "HEIGHT")?;
        let viewpoint = Self::expect_entry(&mut f, "VIEWPOINT")?;
        let points = Self::expect_entry(&mut f, "POINTS")?;
        let data = Self::expect_entry(&mut f, "DATA")?;
        let data_start_pos = f.seek(Current(0)).map_err(|e| anyhow!(e))?;

        // parse & zip field data
        let fields = Self::parse_space_seperated(&fields, Self::parse_string)?;
        let size = Self::parse_space_seperated(&size, Self::parse_u32)?;
        let typ = Self::parse_space_seperated(&typ, Self::parse_type)?;
        let count = Self::parse_space_seperated(&count, Self::parse_u32)?;
        if fields.len() != size.len() || fields.len() != typ.len() || fields.len() != count.len() {
            return Err(anyhow!("The number of fields in the header entries FIELDS, SIZE, TYPE and COUNT did not match.").into());
        }
        let zipped_fields: Vec<_> = fields
            .into_iter()
            .zip(size)
            .zip(typ)
            .zip(count)
            .map(|(((field, size), typ), count)| PclField {
                field,
                size,
                typ,
                count,
            })
            .collect();

        let header = PclHeader {
            version,
            fields: zipped_fields,
            width: Self::parse_u32(&width)?,
            height: Self::parse_u32(&height)?,
            viewpoint,
            points: Self::parse_u32(&points)?,
            data: Self::parse_encoding(&data)?,
            data_start_pos,
        };

        Ok(header)
    }

    fn parse_space_seperated<T>(val: &str, inner: impl Fn(&str) -> Result<T>) -> Result<Vec<T>> {
        val.split(' ').map(inner).collect()
    }

    fn parse_u32(val: &str) -> Result<u32> {
        Ok(u32::from_str(val)?)
    }

    fn parse_f64(val: &str) -> Result<f64> {
        Ok(f64::from_str(val)?)
    }

    fn parse_string(val: &str) -> Result<String> {
        Ok(val.to_string())
    }

    fn parse_type(val: &str) -> Result<PclType> {
        let typ = match val {
            "I" => PclType::Integer,
            "U" => PclType::Unsigned,
            "F" => PclType::Float,
            &_ => return Err(anyhow!("Expected one of 'I', 'U' or 'F' as type.")),
        };
        Ok(typ)
    }

    fn parse_encoding(val: &str) -> Result<PclEncoding> {
        let typ = match val {
            "ascii" => PclEncoding::Ascii,
            "binary" => PclEncoding::Binary,
            "binary_compressed" => PclEncoding::BinaryCompressed,
            &_ => {
                return Err(anyhow!(
                    "Expected one of 'ascii' or 'binary' or 'binary_compressed' as encoding."
                ))
            }
        };
        Ok(typ)
    }

    fn point_size(&self) -> usize {
        self.fields.iter().map(|f| f.size * f.count).sum::<u32>() as usize
    }

    fn read_data(
        &self,
        f: impl BufRead + Seek,
        origin: (f64, f64, f64),
    ) -> Result<Vec<GlobalPoint>, PclReadError> {
        match self.data {
            PclEncoding::Ascii => self.read_data_ascii(f, origin),
            PclEncoding::Binary => self.read_data_binary(f, origin),
            PclEncoding::BinaryCompressed => {
                Err(anyhow!("Binary Compressed format is not supported.").into())
            }
        }
    }

    fn read_data_ascii(
        &self,
        mut f: impl BufRead + Seek,
        origin: (f64, f64, f64),
    ) -> Result<Vec<GlobalPoint>, PclReadError> {
        let nr_points = (self.width * self.height) as usize;
        let mut points = Vec::with_capacity(nr_points);

        // location of x, y, z fields
        let field_x_info = self.get_type_info("x")?;
        let field_y_info = self.get_type_info("y")?;
        let field_z_info = self.get_type_info("z")?;
        let nr_fields = self.fields.iter().map(|f| f.count).sum::<u32>() as usize;

        // read file (line by line)
        f.seek(Start(self.data_start_pos)).map_err(|e| anyhow!(e))?;
        for i in 0..nr_points {
            // read line
            let mut line = String::new();
            let len = f.read_line(&mut line).map_err(|e| anyhow!(e))?;
            if len == 0 || !line.ends_with('\n') {
                return Err(PclReadError::UnexpectedEOF);
            }

            // split into fields
            let fields = line.trim_end().split(' ').collect::<Vec<_>>();
            if fields.len() != nr_fields {
                return Err(PclReadError::Other(anyhow!(
                    "Point {}: Expected {} fields, but got {}",
                    i,
                    nr_fields,
                    fields.len()
                )));
            }

            // extract x, y, z
            let x = Self::parse_f64(fields[field_x_info.index])? - origin.0;
            let y = Self::parse_f64(fields[field_y_info.index])? - origin.1;
            let z = Self::parse_f64(fields[field_z_info.index])? - origin.2;
            let point = GlobalPoint::new(F64Position::new(x, y, z));
            points.push(point);
        }
        Ok(points)
    }

    fn read_data_binary(
        &self,
        mut f: impl Read + Seek,
        origin: (f64, f64, f64),
    ) -> Result<Vec<GlobalPoint>, PclReadError> {
        // calculate how many points - and subsequently how many bytes - to read.
        let points_to_read = (self.width * self.height) as usize;
        let point_size = self.point_size();
        let bytes_to_read = points_to_read * point_size;

        // read data
        f.seek(Start(self.data_start_pos)).map_err(|e| anyhow!(e))?;
        let mut data = vec![0; bytes_to_read];
        f.read_exact(&mut data).map_err(|e| {
            if e.kind() == ErrorKind::UnexpectedEof {
                PclReadError::UnexpectedEOF
            } else {
                anyhow!(e).into()
            }
        })?;

        // read positions
        let mut positions = vec![F64Position::default(); points_to_read];
        self.collect_field_as_f64(&data, "x", |i, x| positions[i].set_x(x - origin.0))?;
        self.collect_field_as_f64(&data, "y", |i, y| positions[i].set_y(y - origin.1))?;
        self.collect_field_as_f64(&data, "z", |i, z| positions[i].set_z(z - origin.2))?;

        // create points
        let points = positions
            .into_iter()
            .map(GlobalPoint::new)
            .collect::<Vec<_>>();
        Ok(points)
    }

    fn get_type_info(&self, field_name: &str) -> Result<TypeInfo> {
        let mut offset = 0;
        let mut index = 0;
        let mut attr_i = self.fields.len();
        for (i, field) in self.fields.iter().enumerate() {
            if field.field == field_name {
                attr_i = i;
                break;
            } else {
                offset += (field.size * field.count) as usize;
                index += field.count as usize;
            }
        }
        if attr_i == self.fields.len() {
            return Err(anyhow!(
                "Field '{}' is not present in the point cloud.",
                field_name
            ));
        }
        let stride = self.point_size();
        Ok(TypeInfo {
            offset,
            stride,
            index,
            field: &self.fields[attr_i],
        })
    }

    fn collect_field_as_f64(
        &self,
        data: &[u8],
        name: &str,
        collect: impl FnMut(usize, f64),
    ) -> Result<()> {
        let TypeInfo {
            offset,
            stride,
            field,
            ..
        } = self.get_type_info(name)?;
        use read_primitives::as_f64::*;
        match (field.typ, field.size) {
            (PclType::Unsigned, 1) => Self::collect_field(data, stride, offset, rd_u8, collect),
            (PclType::Unsigned, 2) => Self::collect_field(data, stride, offset, rd_u16, collect),
            (PclType::Unsigned, 4) => Self::collect_field(data, stride, offset, rd_u32, collect),
            (PclType::Unsigned, 8) => Self::collect_field(data, stride, offset, rd_u64, collect),
            (PclType::Integer, 1) => Self::collect_field(data, stride, offset, rd_i8, collect),
            (PclType::Integer, 2) => Self::collect_field(data, stride, offset, rd_i16, collect),
            (PclType::Integer, 4) => Self::collect_field(data, stride, offset, rd_i32, collect),
            (PclType::Integer, 8) => Self::collect_field(data, stride, offset, rd_i64, collect),
            (PclType::Float, 4) => Self::collect_field(data, stride, offset, rd_f32, collect),
            (PclType::Float, 8) => Self::collect_field(data, stride, offset, rd_f64, collect),
            _ => {
                return Err(anyhow!(
                    "Field '{}' has unsupported type ({:?} of size {})",
                    name,
                    field.typ,
                    field.size
                ))
            }
        }
        Ok(())
    }

    fn collect_field<const LEN: usize, T>(
        data: &[u8],
        stride: usize,
        offset: usize,
        read_primitive: impl Fn([u8; LEN]) -> T,
        mut collect: impl FnMut(usize, T),
    ) {
        let mut pos = offset;
        let mut i = 0;
        while pos + LEN <= data.len() {
            let bytes = <[u8; LEN]>::try_from(&data[pos..pos + LEN]).unwrap();
            let primitive = read_primitive(bytes);
            collect(i, primitive);
            pos += stride;
            i += 1;
        }
    }
}

mod read_primitives {

    macro_rules! read_primitive {
        ($fn:ident, $len:literal, $ty:ty, $as:ty) => {
            #[inline]
            pub fn $fn(data: [u8; $len]) -> $as {
                <$ty>::from_le_bytes(data) as $as // todo for now assuming that pcl uses little endian for its numbers...
            }
        };
    }

    pub mod as_f64 {
        read_primitive!(rd_i8, 1, i8, f64);
        read_primitive!(rd_i16, 2, i16, f64);
        read_primitive!(rd_i32, 4, i32, f64);
        read_primitive!(rd_i64, 8, i64, f64);
        read_primitive!(rd_u8, 1, u8, f64);
        read_primitive!(rd_u16, 2, u16, f64);
        read_primitive!(rd_u32, 4, u32, f64);
        read_primitive!(rd_u64, 8, u64, f64);
        read_primitive!(rd_f32, 4, f32, f64);
        read_primitive!(rd_f64, 8, f64, f64);
    }
}
