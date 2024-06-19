use crate::cli::Cli;
use lidarserv_server::{
    common::{
        geometry::{
            points::PointType,
            position::{F64Position, Position},
        },
        las::LasPointAttributes,
    },
    index::point::{GenericPoint, GlobalPoint},
};
use log::{debug, warn};
use rosrust_msg::sensor_msgs::{PointCloud2, PointField};
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Error)]
enum PointCloudReadError {
    #[error("The point cloud is missing the field '{field_name}'.")]
    MissingField { field_name: String },

    #[error(
        "The field '{field_name}' is expected to have a size of {expected_size} due to its type."
    )]
    SizeMismatch {
        field_name: String,
        expected_size: usize,
        actual_size: usize,
    },

    #[error(
        "The field '{field_name}' of type {field_type} can't be interpreted as {target_type_name}."
    )]
    TypeMismatch {
        field_name: String,
        field_type: u8,
        target_type_name: &'static str,
    },
}

pub fn ros_read_thread(options: &Cli, sender: mpsc::Sender<Vec<GlobalPoint>>) {
    rosrust::init("lidarserv");

    let _subscriber_tf =
        rosrust::subscribe("tf", 100, move |msg: rosrust_msg::tf2_msgs::TFMessage| {
            for tf in msg.transforms {
                debug!("tf: {} -> {}", &tf.header.frame_id, &tf.child_frame_id);
            }
        });

    // Create subscriber
    let _subscriber = rosrust::subscribe(
        &options.subscribe_topic_name,
        100,
        move |msg: PointCloud2| {
            // Callback for handling received messages
            debug!("points - tf={}", &msg.header.frame_id);
            let points = match parse_pointcloud2_message(msg) {
                Ok(p) => p,
                Err(e) => {
                    warn!("Ignoring received points: {e}");
                    return;
                }
            };
            sender.blocking_send(points).unwrap();
        },
    )
    .unwrap();

    // Block the thread until a shutdown signal is received
    rosrust::spin();
}

fn parse_pointcloud2_message(msg: PointCloud2) -> Result<Vec<GlobalPoint>, PointCloudReadError> {
    let nr_points = msg.width as usize * msg.height as usize;
    let mut points =
        vec![GenericPoint::new(F64Position::from_components(0.0, 0.0, 0.0)); nr_points];

    // required: x, y, z
    let field_x = find_field(&msg, "x")?;
    let field_y = find_field(&msg, "y")?;
    let field_z = find_field(&msg, "z")?;
    read_point_attribute_f64(
        &msg,
        &mut points,
        field_x,
        #[inline]
        |point, x| {
            point.position_mut().set_x(x);
        },
    )?;
    read_point_attribute_f64(
        &msg,
        &mut points,
        field_y,
        #[inline]
        |point, y| {
            point.position_mut().set_y(y);
        },
    )?;
    read_point_attribute_f64(
        &msg,
        &mut points,
        field_z,
        #[inline]
        |point, z| {
            point.position_mut().set_z(-z);
        },
    )?;

    // optional: GpsTime
    // use frame time, if attribute not present.
    if let Ok(field_gps_time) = find_field(&msg, "gps_time") {
        read_point_attribute_f64(
            &msg,
            &mut points,
            field_gps_time,
            #[inline]
            |point, value| point.attribute_mut::<LasPointAttributes>().gps_time = value,
        )?;
    } else {
        let time = msg.header.stamp.seconds();
        for point in &mut points {
            point.attribute_mut::<LasPointAttributes>().gps_time = time;
        }
    }

    // optional: intensity
    if let Ok(field) = find_field(&msg, "intensity") {
        read_point_attribute_u16(&msg, &mut points, field, |p, v| {
            p.attribute_mut::<LasPointAttributes>().intensity = v
        })?;
    }

    // optional: point_source_id
    if let Ok(field) = find_field(&msg, "point_source_id") {
        read_point_attribute_u16(&msg, &mut points, field, |p, v| {
            p.attribute_mut::<LasPointAttributes>().point_source_id = v
        })?;
    }

    // optional: r,g,b
    if let Ok(field) = find_field(&msg, "r") {
        read_point_attribute_u16(&msg, &mut points, field, |p, v| {
            p.attribute_mut::<LasPointAttributes>().color.0 = v
        })?;
    }
    if let Ok(field) = find_field(&msg, "g") {
        read_point_attribute_u16(&msg, &mut points, field, |p, v| {
            p.attribute_mut::<LasPointAttributes>().color.1 = v
        })?;
    }
    if let Ok(field) = find_field(&msg, "b") {
        read_point_attribute_u16(&msg, &mut points, field, |p, v| {
            p.attribute_mut::<LasPointAttributes>().color.2 = v
        })?;
    }

    // optional: return_number
    if let Ok(field) = find_field(&msg, "return_number") {
        read_point_attribute_u8(&msg, &mut points, field, |p, v| {
            p.attribute_mut::<LasPointAttributes>().return_number = v
        })?;
    }

    // optional: number_of_returns
    if let Ok(field) = find_field(&msg, "number_of_returns") {
        read_point_attribute_u8(&msg, &mut points, field, |p, v| {
            p.attribute_mut::<LasPointAttributes>().number_of_returns = v
        })?;
    }

    // optional: classification
    if let Ok(field) = find_field(&msg, "classification") {
        read_point_attribute_u8(&msg, &mut points, field, |p, v| {
            p.attribute_mut::<LasPointAttributes>().classification = v
        })?;
    }

    // optional: scan_angle_rank
    if let Ok(field) = find_field(&msg, "scan_angle_rank") {
        read_point_attribute_i8(&msg, &mut points, field, |p, v| {
            p.attribute_mut::<LasPointAttributes>().scan_angle_rank = v
        })?;
    }

    // done
    Ok(points)
}

fn read_point_attribute<A>(
    msg: &PointCloud2,
    points: &mut [GlobalPoint],
    field: &PointField,
    read_attr: impl Fn(&[u8]) -> A,
    set: impl Fn(&mut GlobalPoint, A),
) {
    let height = msg.height as usize;
    let width = msg.width as usize;
    let row_step = msg.row_step as usize;
    let point_step = msg.point_step as usize;
    let field_offset = field.offset as usize;
    let field_size = ros_field_size(field.datatype);

    let nr_points = width * height;
    assert_eq!(points.len(), nr_points);

    let min_data_size =
        row_step * (height - 1) + point_step * (width - 1) + field_offset + field_size;
    assert!(msg.data.len() >= min_data_size);

    for row in 0..height {
        let row_start = row * row_step;
        for col in 0..width {
            // data range to read from
            let point_start = row_start + col * point_step;
            let field_start = point_start + field_offset;
            let field_end = field_start + field_size;
            let data = &msg.data[field_start..field_end];

            // point to write to
            let point_index = row * width + col;
            let point = &mut points[point_index];

            // set
            let value = read_attr(data);
            set(point, value);
        }
    }
}

macro_rules! fn_parse_le {
    ($field:ident: $t:ty as $t2:ty) => {{
        let actual_size = ros_field_size($field.datatype);
        let expected_size = std::mem::size_of::<$t>();
        if actual_size == expected_size {
            #[inline]
            fn parse(data: &[u8]) -> $t2 {
                <$t>::from_le_bytes(<[u8; std::mem::size_of::<$t>()]>::try_from(data).unwrap())
                    as $t2
            }
            Ok(parse)
        } else {
            Err(PointCloudReadError::SizeMismatch {
                field_name: $field.name.clone(),
                expected_size,
                actual_size,
            })
        }
    }};
}

macro_rules! fn_parse_be {
    ($field:ident: $t:ty as $t2:ty) => {{
        let actual_size = ros_field_size($field.datatype);
        let expected_size = std::mem::size_of::<$t>();
        if actual_size == expected_size {
            #[inline]
            fn parse(data: &[u8]) -> $t2 {
                <$t>::from_be_bytes(<[u8; std::mem::size_of::<$t>()]>::try_from(data).unwrap())
                    as $t2
            }
            Ok(parse)
        } else {
            Err(PointCloudReadError::SizeMismatch {
                field_name: $field.name.clone(),
                expected_size,
                actual_size,
            })
        }
    }};
}

fn find_field<'a>(
    msg: &'a PointCloud2,
    attribute_name: &str,
) -> Result<&'a PointField, PointCloudReadError> {
    let maybe_field = msg
        .fields
        .iter()
        .find(|f| f.name == attribute_name && f.count > 0);
    let Some(field) = maybe_field else {
        return Err(PointCloudReadError::MissingField {
            field_name: attribute_name.to_string(),
        });
    };
    Ok(field)
}

fn read_point_attribute_f64(
    msg: &PointCloud2,
    points: &mut [GlobalPoint],
    field: &PointField,
    set: impl Fn(&mut GlobalPoint, f64),
) -> Result<(), PointCloudReadError> {
    // read (different read function based on datatype)
    // (accept datatypes f32 or f64)
    let parse = match field.datatype {
        ros_field_types::FLOAT32 => {
            if msg.is_bigendian {
                fn_parse_be!(field: f32 as f64)?
            } else {
                fn_parse_le!(field: f32 as f64)?
            }
        }
        ros_field_types::FLOAT64 => {
            if msg.is_bigendian {
                fn_parse_be!(field: f64 as f64)?
            } else {
                fn_parse_le!(field: f64 as f64)?
            }
        }
        _ => {
            return Err(PointCloudReadError::TypeMismatch {
                field_name: field.name.clone(),
                field_type: field.datatype,
                target_type_name: "float64",
            })
        }
    };
    read_point_attribute(msg, points, field, parse, set);

    Ok(())
}

fn read_point_attribute_u16(
    msg: &PointCloud2,
    points: &mut [GlobalPoint],
    field: &PointField,
    set: impl Fn(&mut GlobalPoint, u16),
) -> Result<(), PointCloudReadError> {
    // read (different read function based on datatype)
    // (accept datatypes u8 or u16)
    let parse = match field.datatype {
        ros_field_types::UINT8 => {
            if msg.is_bigendian {
                fn_parse_be!(field: u8 as u16)?
            } else {
                fn_parse_le!(field: u8 as u16)?
            }
        }
        ros_field_types::UINT16 => {
            if msg.is_bigendian {
                fn_parse_be!(field: u16 as u16)?
            } else {
                fn_parse_le!(field: u16 as u16)?
            }
        }
        ros_field_types::FLOAT32 => {
            if msg.is_bigendian {
                fn_parse_be!(field: f32 as u16)?
            } else {
                fn_parse_le!(field: f32 as u16)?
            }
        }
        ros_field_types::FLOAT64 => {
            if msg.is_bigendian {
                fn_parse_be!(field: f64 as u16)?
            } else {
                fn_parse_le!(field: f64 as u16)?
            }
        }
        _ => {
            return Err(PointCloudReadError::TypeMismatch {
                field_name: field.name.clone(),
                field_type: field.datatype,
                target_type_name: "uint16",
            })
        }
    };
    read_point_attribute(msg, points, field, parse, set);

    Ok(())
}

fn read_point_attribute_u8(
    msg: &PointCloud2,
    points: &mut [GlobalPoint],
    field: &PointField,
    set: impl Fn(&mut GlobalPoint, u8),
) -> Result<(), PointCloudReadError> {
    // read (different read function based on datatype)
    // (accept datatype u8)
    let parse = match field.datatype {
        ros_field_types::UINT8 => {
            if msg.is_bigendian {
                fn_parse_be!(field: u8 as u8)?
            } else {
                fn_parse_le!(field: u8 as u8)?
            }
        }
        _ => {
            return Err(PointCloudReadError::TypeMismatch {
                field_name: field.name.clone(),
                field_type: field.datatype,
                target_type_name: "uint8",
            })
        }
    };
    read_point_attribute(msg, points, field, parse, set);

    Ok(())
}

fn read_point_attribute_i8(
    msg: &PointCloud2,
    points: &mut [GlobalPoint],
    field: &PointField,
    set: impl Fn(&mut GlobalPoint, i8),
) -> Result<(), PointCloudReadError> {
    // read (different read function based on datatype)
    // (accept datatype i8)
    let parse = match field.datatype {
        ros_field_types::INT8 => {
            if msg.is_bigendian {
                fn_parse_be!(field: i8 as i8)?
            } else {
                fn_parse_le!(field: i8 as i8)?
            }
        }
        _ => {
            return Err(PointCloudReadError::TypeMismatch {
                field_name: field.name.clone(),
                field_type: field.datatype,
                target_type_name: "int8",
            })
        }
    };
    read_point_attribute(msg, points, field, parse, set);

    Ok(())
}

#[allow(unused)] // some of the constants are not used (yet), but I want them to be complete.
mod ros_field_types {
    pub const INT8: u8 = 1;
    pub const UINT8: u8 = 2;
    pub const INT16: u8 = 3;
    pub const UINT16: u8 = 4;
    pub const INT32: u8 = 5;
    pub const UINT32: u8 = 6;
    pub const FLOAT32: u8 = 7;
    pub const FLOAT64: u8 = 8;
}

pub fn ros_field_size(typ: u8) -> usize {
    use ros_field_types::*;
    match typ {
        INT8 => 1,
        UINT8 => 1,
        INT16 => 2,
        UINT16 => 2,
        INT32 => 4,
        UINT32 => 4,
        FLOAT32 => 4,
        FLOAT64 => 8,
        _ => unreachable!(),
    }
}
