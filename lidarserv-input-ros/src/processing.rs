use crate::{
    cli::AppOptions,
    lidarserv::LidarservPointCloudInfo,
    ros::{Endianess, Field, PointCloudMessage, Transform},
    status::Status,
    transform_tree::{LookupError, TransformTree},
};
use anyhow::anyhow;
use conversions::{ConverterDyn, DstType};
use lidarserv_common::geometry::{
    coordinate_system::CoordinateSystem, position::WithComponentTypeOnce,
};
use log::{info, warn};
use nalgebra::Point3;
use pasture_core::{
    containers::{
        BorrowedBuffer, BorrowedBufferExt, BorrowedMutBufferExt, InterleavedBuffer,
        InterleavedBufferMut, OwningBuffer, VectorBuffer,
    },
    layout::{
        attributes::{COLOR_RGB, POSITION_3D},
        PointAttributeDefinition, PointAttributeMember, PointLayout,
    },
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::Ordering,
        mpsc::{self, RecvTimeoutError},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
const MAX_WAIT_TIME: Duration = Duration::from_secs(10);

enum MsgOrExit {
    Msg(PointCloudMessage),
    Exit,
}

pub fn processing_thread(
    args: AppOptions,
    info_rx: mpsc::Receiver<LidarservPointCloudInfo>,
    ros_rx: mpsc::Receiver<PointCloudMessage>,
    transforms_rx: mpsc::Receiver<Transform>,
    points_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    stop_process_rx: mpsc::Receiver<()>,
    status: Arc<Status>,
) -> Result<(), anyhow::Error> {
    let info = info_rx.recv()?;
    drop(info_rx);

    let (msg_or_exit_tx, msg_or_exit_rx) = mpsc::sync_channel(0);
    {
        let msg_or_exit_tx = msg_or_exit_tx.clone();
        thread::spawn(move || {
            for _ in stop_process_rx {
                msg_or_exit_tx.send(MsgOrExit::Exit).ok();
            }
        });
    }
    {
        let msg_or_exit_tx = msg_or_exit_tx.clone();
        thread::spawn(move || {
            for msg in ros_rx {
                msg_or_exit_tx.send(MsgOrExit::Msg(msg)).ok();
            }
            msg_or_exit_tx.send(MsgOrExit::Exit).ok();
        });
    }

    // layout for positions in global space (always Vec4F64)
    let global_attributes: Vec<_> = info
        .attributes
        .iter()
        .cloned()
        .map(|a| {
            if a.name() == POSITION_3D.name() {
                POSITION_3D
            } else {
                a
            }
        })
        .collect();
    let global_layout = PointLayout::from_attributes_packed(&global_attributes, 1);
    let lidarserv_layout = PointLayout::from_attributes_packed(&info.attributes, 1);

    // Number converters
    let mut init_fields = Vec::new();
    let mut init_endianess = Endianess::BigEndian;
    let mut converters = None;

    // transform tree for transforming coordinates into the world frame
    let mut transform_tree = TransformTree::new();

    loop {
        // keep maintaining the transform tree to avoid the
        // transforms_rx channel to fill up too much.
        let mut clean_before = None;
        for transform in transforms_rx.try_iter() {
            clean_before = Some(transform.time_stamp.saturating_sub(MAX_WAIT_TIME));
            transform_tree.add(transform);
        }
        if let Some(time_stamp) = clean_before {
            transform_tree.cleanup_before(time_stamp);
        }

        // receive next message
        let msg = match msg_or_exit_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(MsgOrExit::Msg(m)) => m,
            Ok(MsgOrExit::Exit) => break,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };

        // update status
        status.nr_process_in.fetch_add(1, Ordering::Relaxed);

        // init converters on first message
        let converters = converters.get_or_insert_with(|| {
            init_fields = msg.fields.clone();
            init_endianess = msg.endianess;
            initialize(&init_fields, init_endianess, &global_layout)
        });

        // ensure the fields and endianess are still identical to when
        // the converters where initialized. Otherwise, the converters will just
        // produce garbage.
        if msg.fields != init_fields || msg.endianess != init_endianess {
            return Err(anyhow!(
                "Fields and endianess of all ROS PointCloud2 messages must be identical."
            ));
        }

        // reserve buffer of correct size
        let nr_points = msg.width * msg.height;
        let mut buffer = VectorBuffer::with_capacity(nr_points, global_layout.clone());
        buffer.resize(nr_points);

        // convert and copy over point data
        for converter in converters {
            converter.convert(&msg, &mut buffer);
        }

        // get transform to world frame
        transform_tree.cleanup_before(msg.time_stamp);
        let wait_start = Instant::now();
        let transform = loop {
            for transform in transforms_rx.try_iter() {
                transform_tree.add(transform);
            }
            match transform_tree.transform(msg.time_stamp, &msg.frame, &args.world_frame) {
                Ok(t) => break t,
                Err(LookupError::Wait) => {
                    let now = Instant::now();
                    let already_waited = now - wait_start;
                    if already_waited < MAX_WAIT_TIME {
                        let remaining_time = MAX_WAIT_TIME - already_waited;
                        if let Ok(t) = transforms_rx.recv_timeout(remaining_time) {
                            transform_tree.add(t);
                            continue;
                        }
                    }
                }
                Err(LookupError::NotFound) => {}
            }
            return Err(anyhow!(
                "There is no viable transform from frame `{}` to frame `{}`.",
                msg.frame,
                args.world_frame
            ));
        };

        // transform to world frame
        let mut position_attr = buffer.view_attribute_mut::<Point3<f64>>(&POSITION_3D);
        for i in 0..nr_points {
            let local_pos = position_attr.at(i);
            let world_pos = transform.transform_point(&local_pos);
            position_attr.set_at(i, world_pos);
        }

        // flip axis
        if let Some(flip) = args.transform_flip {
            let flip_index = match flip {
                crate::cli::Axis::X => 0,
                crate::cli::Axis::Y => 1,
                crate::cli::Axis::Z => 2,
            };
            let mut positions = buffer.view_attribute_mut::<Point3<f64>>(&POSITION_3D);
            for pid in 0..nr_points {
                let mut pos = positions.at(pid);
                pos[flip_index] *= -1.0;
                positions.set_at(pid, pos);
            }
        }

        // transform to lidarserv coordinate system
        let mut lidarserv_buffer = VectorBuffer::with_capacity(nr_points, lidarserv_layout.clone());
        lidarserv_buffer.resize(nr_points);
        for dst_attr in lidarserv_layout.attributes() {
            if dst_attr.name() == POSITION_3D.name() {
                struct Wct<'a> {
                    src_buffer: &'a VectorBuffer,
                    dst_buffer: &'a mut VectorBuffer,
                    dst_attr: PointAttributeDefinition,
                    coordinate_system: CoordinateSystem,
                }

                impl WithComponentTypeOnce for Wct<'_> {
                    type Output = ();

                    fn run_once<C: lidarserv_common::geometry::position::Component>(
                        self,
                    ) -> Self::Output {
                        let src_positions =
                            self.src_buffer.view_attribute::<Point3<f64>>(&POSITION_3D);
                        let mut dst_positions = self
                            .dst_buffer
                            .view_attribute_mut::<C::PasturePrimitive>(&self.dst_attr);
                        let mut error_count = 0;
                        for pid in 0..self.src_buffer.len() {
                            let position_global = src_positions.at(pid);
                            let position_lidarserv = match self
                                .coordinate_system
                                .encode_position::<C>(position_global)
                            {
                                Ok(p) => p,
                                Err(_) => {
                                    error_count += 1;
                                    Point3::<C>::new(C::zero(), C::zero(), C::zero())
                                }
                            };
                            dst_positions.set_at(pid, C::position_to_pasture(position_lidarserv));
                        }
                        if error_count > 0 {
                            warn!("Received {error_count} point(s) outside the coordinate system. These points were set to zero.");
                        }
                    }
                }

                Wct {
                    src_buffer: &buffer,
                    dst_buffer: &mut lidarserv_buffer,
                    dst_attr: dst_attr.attribute_definition().clone(),
                    coordinate_system: info.coordinate_system,
                }
                .for_layout_once(&lidarserv_layout);
            } else {
                let src_attr = global_layout
                    .get_attribute(dst_attr.attribute_definition())
                    .expect("Missing attribute");
                let src_range = src_attr.byte_range_within_point();
                let dst_range = dst_attr.byte_range_within_point();
                for pid in 0..nr_points {
                    let src_point = buffer.get_point_ref(pid);
                    let dst_point = lidarserv_buffer.get_point_mut(pid);
                    let src = &src_point[src_range.clone()];
                    let dst = &mut dst_point[dst_range.clone()];
                    dst.copy_from_slice(src);
                }
            }
        }

        // encode bytes
        let mut data_buffer = Vec::new();
        info.codec
            .instance()
            .write_points(&lidarserv_buffer, &mut data_buffer)?;

        // send to lidarserv network thread
        status.nr_process_out.fetch_add(1, Ordering::Relaxed);
        points_tx.send(data_buffer)?;
    }

    Ok(())
}

/// Builds the list of conversion functions based on the
/// source and destination layout and logs some info for the user.
fn initialize(
    src_fields: &[Field],
    src_endianess: Endianess,
    dst_layout: &PointLayout,
) -> Vec<Box<dyn ConverterDyn>> {
    // print layout info
    info!("Point attributes from ROS PointCloud2 messages:");
    for field in src_fields {
        let mut type_name = format!("{:?}", field.typ);
        if field.count > 1 {
            type_name = format!("[{}; {}]", type_name, field.count);
        }
        info!(" - {} {}", type_name, field.name);
    }
    info!("Point attributes from LidarServ:");
    for field in dst_layout.attributes() {
        info!(" - {} {}", field.datatype(), field.name());
    }

    // find matching fields in messages
    let field_map = make_field_map(src_fields, dst_layout);

    // print mapping info
    info!("Attribute mapping:");
    for mapping in &field_map {
        let mut ros_fields_str: String;
        let mut fields_iter = mapping.src_fields.iter().cloned();
        if let Some(first) = fields_iter.next() {
            ros_fields_str = first.name.clone();
        } else {
            continue;
        }
        for field in fields_iter {
            ros_fields_str += ", ";
            ros_fields_str += &field.name;
        }
        info!(" - {} <= {}", mapping.dst_attribute.name(), ros_fields_str);
    }

    // warn about attributes that are not mapped or that use lossy conversions
    let lossy: HashSet<_> = conversions::LOSSY_CONVERSIONS.iter().copied().collect();
    for mapping in &field_map {
        if mapping.src_fields.is_empty() {
            warn!("Attribute {} has no corresponding field(s) in the ROS PointCloud2 message and will be filled with zeros.", mapping.dst_attribute);
        }
        for ros_field in &mapping.src_fields {
            if lossy.contains(&(ros_field.typ, mapping.dst_type)) {
                warn!("Attribute {} uses a lossy conversion ({:?} to {:?}) when reading values from the field {} in the ROS PointCloud2 message.", mapping.dst_attribute, ros_field.typ, mapping.dst_type, ros_field.name);
            }
        }
    }

    // get converters for this field mapping.
    make_converters(&field_map, src_endianess)
}

fn normalize_field_name(name: &str) -> String {
    let special_characters = ['_', '-', '.', ' ', '[', ']'];
    name.chars()
        .filter(|char| !special_characters.contains(char))
        .flat_map(|char| char.to_lowercase())
        .collect()
}

struct FieldMapping<'a, 'b> {
    dst_attribute: &'a PointAttributeMember,
    dst_type: DstType,
    dst_count: usize,
    src_fields: Vec<&'b Field>,
}

/// Finds corresponding fields in the source ROS PointCloud2 message
/// for the attributes in the destination pasture buffer.
fn make_field_map<'a, 'b>(src: &'b [Field], dst: &'a PointLayout) -> Vec<FieldMapping<'a, 'b>> {
    // init empty mapping for the destination point attributes.
    let mut map: Vec<FieldMapping> = dst
        .attributes()
        .map(|dst_attribute| {
            let (dst_type, dst_count) = DstType::from_pasture(dst_attribute.datatype());
            FieldMapping {
                dst_attribute,
                dst_type,
                dst_count,
                src_fields: Vec::new(),
            }
        })
        .collect();

    // lookup src fields by their (normalized) name.
    let fields_by_name = src
        .iter()
        .map(|field| (normalize_field_name(&field.name), field))
        .collect::<HashMap<_, _>>();

    for mapping in &mut map {
        // build a list of possible field names and value counts that would match this attribute.
        let mut candidate_names = Vec::new();

        // special rules for position3d
        if *mapping.dst_attribute.attribute_definition() == POSITION_3D {
            candidate_names.push(vec![
                ("x".to_string(), 1),
                ("y".to_string(), 1),
                ("z".to_string(), 1),
            ]);
        }

        // special rules for color
        if *mapping.dst_attribute.attribute_definition() == COLOR_RGB {
            candidate_names.push(vec![
                ("r".to_string(), 1),
                ("g".to_string(), 1),
                ("b".to_string(), 1),
            ]);
        }

        // point attributes are always matched by fields with the same (normalized) name and matching value count.
        let normalized_name = normalize_field_name(mapping.dst_attribute.name());
        candidate_names.push(vec![(normalized_name.clone(), mapping.dst_count)]);

        // point attributes consisting of exactly 3 values match the attribute name
        // suffixed with 'x', 'y', 'z',
        // so that e.g.: point attribute "normal" (pasture type Vec3F64) is matched by
        // the field names "normal_x", "normal_y", "normal_z".
        if mapping.dst_count == 3 {
            candidate_names.push(vec![
                (format!("{normalized_name}x"), 1),
                (format!("{normalized_name}y"), 1),
                (format!("{normalized_name}z"), 1),
            ]);
        }

        // point attributes consisting of more than one value match the attribute name
        // suffixed with the indices,
        // so that e.g.: point attribute "user_data" (pasture type ByteArray(5)) is matched by
        // the field names "user_data[0]", "user_data[1]", "user_data[2]", "user_data[3]", "user_data[4]".
        if mapping.dst_count > 1 {
            // start counting at 0
            candidate_names.push(
                (0..mapping.dst_count)
                    .map(|i| (format!("{normalized_name}{i}"), 1))
                    .collect(),
            );
            // start counting at 1
            candidate_names.push(
                (1..=mapping.dst_count)
                    .map(|i| (format!("{normalized_name}{i}"), 1))
                    .collect(),
            );
        }

        // look for fields these candidate names.
        for candidate in candidate_names {
            mapping.src_fields.clear();
            for &(ref name, cnt) in &candidate {
                if let Some(f) = fields_by_name.get(name) {
                    if f.count == cnt {
                        mapping.src_fields.push(*f);
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            if mapping.src_fields.len() == candidate.len() {
                break;
            }
        }
    }

    map
}

/// Sets up a list of conversion functions that need to be applied in order to
/// transform the ROS PointCloud2 messages to pasture buffers.
fn make_converters(field_map: &[FieldMapping], endianess: Endianess) -> Vec<Box<dyn ConverterDyn>> {
    let mut converters = Vec::new();
    for mapping in field_map {
        let mut remaining_attribute_range = mapping.dst_attribute.byte_range_within_point();
        for ros_field in &mapping.src_fields {
            for i in 0..ros_field.count {
                let src_offset = ros_field.offset + i * ros_field.typ.len();
                let src_end = src_offset + ros_field.typ.len();
                let dst_offset = remaining_attribute_range.start;
                let dst_end = dst_offset + mapping.dst_type.len();
                remaining_attribute_range.start = dst_end;
                let conv = conversions::make_converter_for(
                    src_offset..src_end,
                    ros_field.typ,
                    endianess,
                    dst_offset..dst_end,
                    mapping.dst_type,
                );
                converters.push(conv);
            }
        }
    }

    converters
}

mod conversions {

    use crate::ros::{Endianess, PointCloudMessage, Type};
    use byteorder::ByteOrder;
    use pasture_core::{
        containers::{InterleavedBufferMut, VectorBuffer},
        layout::PointAttributeDataType,
    };
    use std::{ops::Range, slice};

    /// Number types that we can convert from.
    pub type SrcType = Type;

    /// Number types that we can convert to.
    #[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
    pub enum DstType {
        I8,
        U8,
        I16,
        U16,
        I32,
        U32,
        I64,
        U64,
        F32,
        F64,
    }

    impl DstType {
        /// The size of the type in bytes
        pub fn len(&self) -> usize {
            match self {
                DstType::I8 => 1,
                DstType::U8 => 1,
                DstType::I16 => 2,
                DstType::U16 => 2,
                DstType::I32 => 4,
                DstType::U32 => 4,
                DstType::I64 => 8,
                DstType::U64 => 8,
                DstType::F32 => 4,
                DstType::F64 => 8,
            }
        }

        /// Returns the type and count of values that are needed to represent
        /// the given [PointAttributeDataType] from pasture.
        pub fn from_pasture(datatype: PointAttributeDataType) -> (Self, usize) {
            match datatype {
                PointAttributeDataType::U8 => (DstType::U8, 1),
                PointAttributeDataType::I8 => (DstType::I8, 1),
                PointAttributeDataType::U16 => (DstType::U16, 1),
                PointAttributeDataType::I16 => (DstType::I16, 1),
                PointAttributeDataType::U32 => (DstType::U32, 1),
                PointAttributeDataType::I32 => (DstType::I32, 1),
                PointAttributeDataType::U64 => (DstType::U64, 1),
                PointAttributeDataType::I64 => (DstType::I64, 1),
                PointAttributeDataType::F32 => (DstType::F32, 1),
                PointAttributeDataType::F64 => (DstType::F64, 1),
                PointAttributeDataType::Vec3u8 => (DstType::U8, 3),
                PointAttributeDataType::Vec3u16 => (DstType::U16, 3),
                PointAttributeDataType::Vec3f32 => (DstType::F32, 3),
                PointAttributeDataType::Vec3i32 => (DstType::I32, 3),
                PointAttributeDataType::Vec3f64 => (DstType::F64, 3),
                PointAttributeDataType::Vec4u8 => (DstType::U8, 4),
                PointAttributeDataType::ByteArray(len) => (DstType::U8, len as usize),
                PointAttributeDataType::Custom { size, .. } => (DstType::U8, size as usize),
            }
        }
    }

    /// Can copy and convert a single value from
    /// a PointCloud2 message into a pasture buffer.
    struct Converter<T> {
        src_start: usize,
        src_end: usize,
        dst_start: usize,
        dst_end: usize,
        fun: T,
    }

    /// Can copy and convert a single value from
    /// a PointCloud2 message into a pasture buffer.
    ///
    /// Trait for [Converter<T>] that erases the type of the actual
    /// conversion function. This allows to store all converters
    /// in a single array of type `Vec<Box<dyn ConverterDyn>>`.
    pub trait ConverterDyn {
        /// Invokes the conversion function for each point.
        ///
        /// (Like this, we only have one virtual function call per converter, not one per point.
        /// This allows the actual per-point call to the conversion function to be inlined. )
        fn convert(&self, src: &PointCloudMessage, dst: &mut VectorBuffer);
    }

    impl<T> ConverterDyn for Converter<T>
    where
        T: Fn(&[u8], &mut [u8]),
    {
        fn convert(&self, src: &PointCloudMessage, dst: &mut VectorBuffer) {
            let mut i = 0;
            for row in 0..src.height {
                let row_offset = row * src.row_step;
                let row_end = row_offset + src.row_step;
                let src_row = &src.data[row_offset..row_end];
                for col in 0..src.width {
                    let point_offset = col * src.point_step;
                    let point_end = point_offset + src.point_step;
                    let src_point = &src_row[point_offset..point_end];
                    let src_field = &src_point[self.src_start..self.src_end];
                    let dst_point = dst.get_point_mut(i);
                    let dst_field = &mut dst_point[self.dst_start..self.dst_end];
                    (self.fun)(src_field, dst_field);
                    i += 1;
                }
            }
        }
    }

    /// Chooses a conversion function based on the
    /// src_type, src_endianess and dst_type and
    /// builds a converter object using this function.
    pub fn make_converter_for(
        src_range: Range<usize>,
        src_typ: SrcType,
        src_endian: Endianess,
        dst_range: Range<usize>,
        dst_typ: DstType,
    ) -> Box<dyn ConverterDyn> {
        macro_rules! case {
            ($fun:expr) => {
                Box::new(Converter {
                    src_start: src_range.start,
                    src_end: src_range.end,
                    dst_start: dst_range.start,
                    dst_end: dst_range.end,
                    fun: $fun,
                })
            };
        }

        macro_rules! case_endian {
            ($fun:ident) => {
                match src_endian {
                    Endianess::BigEndian => case!($fun::<byteorder::BigEndian>),
                    Endianess::LittleEndian => case!($fun::<byteorder::LittleEndian>),
                }
            };
        }

        match (src_typ, dst_typ) {
            (SrcType::I8, DstType::I8) => case!(i8_to_i8),
            (SrcType::I8, DstType::U8) => case!(i8_to_u8),
            (SrcType::I8, DstType::I16) => case!(i8_to_i16),
            (SrcType::I8, DstType::U16) => case!(i8_to_u16),
            (SrcType::I8, DstType::I32) => case!(i8_to_i32),
            (SrcType::I8, DstType::U32) => case!(i8_to_u32),
            (SrcType::I8, DstType::F32) => case!(i8_to_f32),
            (SrcType::I8, DstType::F64) => case!(i8_to_f64),
            (SrcType::U8, DstType::I8) => case!(u8_to_i8),
            (SrcType::U8, DstType::U8) => case!(u8_to_u8),
            (SrcType::U8, DstType::I16) => case!(u8_to_i16),
            (SrcType::U8, DstType::U16) => case!(u8_to_u16),
            (SrcType::U8, DstType::I32) => case!(u8_to_i32),
            (SrcType::U8, DstType::U32) => case!(u8_to_u32),
            (SrcType::U8, DstType::F32) => case!(u8_to_f32),
            (SrcType::U8, DstType::F64) => case!(u8_to_f64),
            (SrcType::I16, DstType::I8) => case_endian!(i16_to_i8),
            (SrcType::I16, DstType::U8) => case_endian!(i16_to_u8),
            (SrcType::I16, DstType::I16) => case_endian!(i16_to_i16),
            (SrcType::I16, DstType::U16) => case_endian!(i16_to_u16),
            (SrcType::I16, DstType::I32) => case_endian!(i16_to_i32),
            (SrcType::I16, DstType::U32) => case_endian!(i16_to_u32),
            (SrcType::I16, DstType::F32) => case_endian!(i16_to_f32),
            (SrcType::I16, DstType::F64) => case_endian!(i16_to_f64),
            (SrcType::U16, DstType::I8) => case_endian!(u16_to_i8),
            (SrcType::U16, DstType::U8) => case_endian!(u16_to_u8),
            (SrcType::U16, DstType::I16) => case_endian!(u16_to_i16),
            (SrcType::U16, DstType::U16) => case_endian!(u16_to_u16),
            (SrcType::U16, DstType::I32) => case_endian!(u16_to_i32),
            (SrcType::U16, DstType::U32) => case_endian!(u16_to_u32),
            (SrcType::U16, DstType::F32) => case_endian!(u16_to_f32),
            (SrcType::U16, DstType::F64) => case_endian!(u16_to_f64),
            (SrcType::I32, DstType::I8) => case_endian!(i32_to_i8),
            (SrcType::I32, DstType::U8) => case_endian!(i32_to_u8),
            (SrcType::I32, DstType::I16) => case_endian!(i32_to_i16),
            (SrcType::I32, DstType::U16) => case_endian!(i32_to_u16),
            (SrcType::I32, DstType::I32) => case_endian!(i32_to_i32),
            (SrcType::I32, DstType::U32) => case_endian!(i32_to_u32),
            (SrcType::I32, DstType::F32) => case_endian!(i32_to_f32),
            (SrcType::I32, DstType::F64) => case_endian!(i32_to_f64),
            (SrcType::U32, DstType::I8) => case_endian!(u32_to_i8),
            (SrcType::U32, DstType::U8) => case_endian!(u32_to_u8),
            (SrcType::U32, DstType::I16) => case_endian!(u32_to_i16),
            (SrcType::U32, DstType::U16) => case_endian!(u32_to_u16),
            (SrcType::U32, DstType::I32) => case_endian!(u32_to_i32),
            (SrcType::U32, DstType::U32) => case_endian!(u32_to_u32),
            (SrcType::U32, DstType::F32) => case_endian!(u32_to_f32),
            (SrcType::U32, DstType::F64) => case_endian!(u32_to_f64),
            (SrcType::F32, DstType::I8) => case_endian!(f32_to_i8),
            (SrcType::F32, DstType::U8) => case_endian!(f32_to_u8),
            (SrcType::F32, DstType::I16) => case_endian!(f32_to_i16),
            (SrcType::F32, DstType::U16) => case_endian!(f32_to_u16),
            (SrcType::F32, DstType::I32) => case_endian!(f32_to_i32),
            (SrcType::F32, DstType::U32) => case_endian!(f32_to_u32),
            (SrcType::F32, DstType::F32) => case_endian!(f32_to_f32),
            (SrcType::F32, DstType::F64) => case_endian!(f32_to_f64),
            (SrcType::F64, DstType::I8) => case_endian!(f64_to_i8),
            (SrcType::F64, DstType::U8) => case_endian!(f64_to_u8),
            (SrcType::F64, DstType::I16) => case_endian!(f64_to_i16),
            (SrcType::F64, DstType::U16) => case_endian!(f64_to_u16),
            (SrcType::F64, DstType::I32) => case_endian!(f64_to_i32),
            (SrcType::F64, DstType::U32) => case_endian!(f64_to_u32),
            (SrcType::F64, DstType::F32) => case_endian!(f64_to_f32),
            (SrcType::F64, DstType::F64) => case_endian!(f64_to_f64),
            (SrcType::I8, DstType::I64) => case!(i8_to_i64),
            (SrcType::I8, DstType::U64) => case!(i8_to_u64),
            (SrcType::U8, DstType::I64) => case!(u8_to_i64),
            (SrcType::U8, DstType::U64) => case!(u8_to_u64),
            (SrcType::I16, DstType::I64) => case_endian!(i16_to_i64),
            (SrcType::I16, DstType::U64) => case_endian!(i16_to_u64),
            (SrcType::U16, DstType::I64) => case_endian!(u16_to_i64),
            (SrcType::U16, DstType::U64) => case_endian!(u16_to_u64),
            (SrcType::I32, DstType::I64) => case_endian!(i32_to_i64),
            (SrcType::I32, DstType::U64) => case_endian!(i32_to_u64),
            (SrcType::U32, DstType::I64) => case_endian!(u32_to_i64),
            (SrcType::U32, DstType::U64) => case_endian!(u32_to_u64),
            (SrcType::F32, DstType::I64) => case_endian!(f32_to_i64),
            (SrcType::F32, DstType::U64) => case_endian!(f32_to_u64),
            (SrcType::F64, DstType::I64) => case_endian!(f64_to_i64),
            (SrcType::F64, DstType::U64) => case_endian!(f64_to_u64),
        }
    }

    /// List of conversions that are considered "lossy", because they loose information.
    ///
    /// A conversion can be lossy for one of two reasons:
    ///  - The value range of the destination type is smaller than the
    ///    source type. In this case, the conversion function will clamp
    ///    the values to the valid bounds of the target type.
    ///    An example would be the conversion I32 to U8, where the i32
    ///    values need to be clamped to the interval [0, 255].
    ///  - The conversion looses precision. This is for example the
    ///    case for the conversion from F64 to F32.
    pub const LOSSY_CONVERSIONS: &[(SrcType, DstType)] = &[
        (SrcType::I8, DstType::U8),
        (SrcType::I8, DstType::U16),
        (SrcType::I8, DstType::U32),
        (SrcType::U8, DstType::I8),
        (SrcType::I16, DstType::I8),
        (SrcType::I16, DstType::U8),
        (SrcType::I16, DstType::U16),
        (SrcType::I16, DstType::U32),
        (SrcType::U16, DstType::I8),
        (SrcType::U16, DstType::U8),
        (SrcType::U16, DstType::I16),
        (SrcType::I32, DstType::I8),
        (SrcType::I32, DstType::U8),
        (SrcType::I32, DstType::I16),
        (SrcType::I32, DstType::U16),
        (SrcType::I32, DstType::U32),
        (SrcType::U32, DstType::I8),
        (SrcType::U32, DstType::U8),
        (SrcType::U32, DstType::I16),
        (SrcType::U32, DstType::U16),
        (SrcType::U32, DstType::I32),
        (SrcType::F32, DstType::I8),
        (SrcType::F32, DstType::U8),
        (SrcType::F32, DstType::I16),
        (SrcType::F32, DstType::U16),
        (SrcType::F32, DstType::I32),
        (SrcType::F32, DstType::U32),
        (SrcType::F64, DstType::I8),
        (SrcType::F64, DstType::U8),
        (SrcType::F64, DstType::I16),
        (SrcType::F64, DstType::U16),
        (SrcType::F64, DstType::I32),
        (SrcType::F64, DstType::U32),
        (SrcType::F64, DstType::F32),
        (SrcType::I8, DstType::U64),
        (SrcType::I16, DstType::U64),
        (SrcType::I32, DstType::U64),
        (SrcType::F32, DstType::I64),
        (SrcType::F32, DstType::U64),
        (SrcType::F64, DstType::I64),
        (SrcType::F64, DstType::U64),
    ];

    #[inline]
    fn u8_to_u8(src: &[u8], dst: &mut [u8]) {
        dst[0] = src[0]
    }

    #[inline]
    fn u8_to_u16(src: &[u8], dst: &mut [u8]) {
        let value = src[0] as u16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u8_to_u32(src: &[u8], dst: &mut [u8]) {
        let value = src[0] as u32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u8_to_u64(src: &[u8], dst: &mut [u8]) {
        let value = src[0] as u64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u8_to_i8(src: &[u8], dst: &mut [u8]) {
        let value = src[0].min(i8::MAX as u8) as i8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u8_to_i16(src: &[u8], dst: &mut [u8]) {
        let value = src[0] as i16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u8_to_i32(src: &[u8], dst: &mut [u8]) {
        let value = src[0] as i32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u8_to_i64(src: &[u8], dst: &mut [u8]) {
        let value = src[0] as i64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u8_to_f32(src: &[u8], dst: &mut [u8]) {
        let value = src[0] as f32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u8_to_f64(src: &[u8], dst: &mut [u8]) {
        let value = src[0] as f64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u16_to_u8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u16(src).min(u8::MAX as u16) as u8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u16_to_u16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u16(src);
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u16_to_u32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u16(src) as u32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u16_to_u64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u16(src) as u64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u16_to_i8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u16(src).min(i8::MAX as u16) as i8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u16_to_i16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u16(src).min(i16::MAX as u16) as i16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u16_to_i32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u16(src) as i32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u16_to_i64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u16(src) as i64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u16_to_f32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u16(src) as f32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u16_to_f64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u16(src) as f64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u32_to_u8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u32(src).min(u8::MAX as u32) as u8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u32_to_u16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u32(src).min(u16::MAX as u32) as u16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u32_to_u32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u32(src);
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u32_to_u64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u32(src) as u64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u32_to_i8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u32(src).min(i8::MAX as u32) as i8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u32_to_i16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u32(src).min(i16::MAX as u32) as i16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u32_to_i32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u32(src).min(i32::MAX as u32) as i32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u32_to_i64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u32(src) as i64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u32_to_f32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u32(src) as f32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn u32_to_f64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_u32(src) as f64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i8_to_u8(src: &[u8], dst: &mut [u8]) {
        let value = (src[0] as i8).max(0) as u8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i8_to_u16(src: &[u8], dst: &mut [u8]) {
        let value = (src[0] as i8).max(0) as u16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i8_to_u32(src: &[u8], dst: &mut [u8]) {
        let value = (src[0] as i8).max(0) as u32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i8_to_u64(src: &[u8], dst: &mut [u8]) {
        let value = (src[0] as i8).max(0) as u64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i8_to_i8(src: &[u8], dst: &mut [u8]) {
        let value = src[0] as i8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i8_to_i16(src: &[u8], dst: &mut [u8]) {
        let value = (src[0] as i8) as i16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i8_to_i32(src: &[u8], dst: &mut [u8]) {
        let value = (src[0] as i8) as i32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i8_to_i64(src: &[u8], dst: &mut [u8]) {
        let value = (src[0] as i8) as i64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i8_to_f32(src: &[u8], dst: &mut [u8]) {
        let value = (src[0] as i8) as f32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i8_to_f64(src: &[u8], dst: &mut [u8]) {
        let value = (src[0] as i8) as f64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i16_to_u8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i16(src).clamp(0, u8::MAX as i16) as u8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i16_to_u16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i16(src).max(0) as u16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i16_to_u32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i16(src).max(0) as u32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i16_to_u64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i16(src).max(0) as u64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i16_to_i8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i16(src).clamp(i8::MIN as i16, i8::MAX as i16) as i8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i16_to_i16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i16(src);
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i16_to_i32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i16(src) as i32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i16_to_i64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i16(src) as i64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i16_to_f32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i16(src) as f32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i16_to_f64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i16(src) as f64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i32_to_u8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i32(src).clamp(0, u8::MAX as i32) as u8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i32_to_u16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i32(src).clamp(0, u16::MAX as i32) as u16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i32_to_u32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i32(src).clamp(0, u32::MAX as i32) as u32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i32_to_u64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i32(src).max(0) as u64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i32_to_i8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i32(src).clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i32_to_i16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i32(src).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i32_to_i32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i32(src);
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i32_to_i64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i32(src) as i64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i32_to_f32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i32(src) as f32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn i32_to_f64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_i32(src) as f64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f32_to_u8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f32(src) as u8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f32_to_u16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f32(src) as u16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f32_to_u32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f32(src) as u32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f32_to_u64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f32(src) as u64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f32_to_i8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f32(src) as i8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f32_to_i16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f32(src) as i16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f32_to_i32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f32(src) as i32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f32_to_i64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f32(src) as i64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f32_to_f32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f32(src);
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f32_to_f64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f32(src) as f64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f64_to_u8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f64(src) as u8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f64_to_u16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f64(src) as u16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f64_to_u32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f64(src) as u32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f64_to_u64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f64(src) as u64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f64_to_i8<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f64(src) as i8;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f64_to_i16<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f64(src) as i16;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f64_to_i32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f64(src) as i32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f64_to_i64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f64(src) as i64;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f64_to_f32<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f64(src) as f32;
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }

    #[inline]
    fn f64_to_f64<E: ByteOrder>(src: &[u8], dst: &mut [u8]) {
        let value = E::read_f64(src);
        dst.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&value)));
    }
}

#[cfg(test)]
mod test {}
