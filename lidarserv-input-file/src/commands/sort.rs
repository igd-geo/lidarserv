use crate::cli::SortOptions;
use anyhow::{anyhow, Result};
use log::info;
use pasture_core::{
    containers::{
        BorrowedBuffer, BorrowedBufferExt, InterleavedBuffer, InterleavedBufferMut,
        MakeBufferFromLayout, OwningBuffer, OwningBufferExt, VectorBuffer,
    },
    layout::{attributes::GPS_TIME, PointLayout},
    nalgebra::Point3,
};
use pasture_io::{
    base::PointReader,
    las::{
        LASReader, ATTRIBUTE_BASIC_FLAGS, ATTRIBUTE_EXTENDED_FLAGS, ATTRIBUTE_LOCAL_LAS_POSITION,
    },
    las_rs::{Header, Point, Vlr},
};
use std::{
    collections::VecDeque,
    fs::File,
    io::{BufReader, BufWriter, ErrorKind, Read, Seek, SeekFrom, Write},
    path::Path,
    slice,
};
use tempfile::NamedTempFile;

pub async fn sort(options: SortOptions) -> Result<()> {
    // output file must not exist.
    if options.output_file.exists() {
        return Err(anyhow!(
            "Output file {} does already exist.",
            options.output_file.display()
        ));
    }

    // open input files
    let readers = options
        .input_file
        .iter()
        .map(|path| LASReader::from_path(path, true))
        .collect::<Result<Vec<_>>>()?;

    let first = readers.first().expect("Input files may not be empty.");

    // layouts need to match
    let layout = first.get_default_point_layout().clone();
    for (i, reader) in readers.iter().enumerate() {
        if *reader.get_default_point_layout() != layout {
            let name = options.input_file[i].display();
            return Err(anyhow!(
                "All input files must have identical point layouts. (at: {name})"
            ));
        }
    }

    // layout must contain gps time
    if !layout.has_attribute(&GPS_TIME) {
        return Err(anyhow!("Input files must have attribute {GPS_TIME}."));
    }

    // coordinate systems need to match
    let header = first.header().clone();
    let transform = *header.transforms();
    for (i, reader) in readers.iter().enumerate() {
        if *reader.header().transforms() != transform {
            let name = options.input_file[i].display();
            return Err(anyhow!(
                "All input files must have identical offset/scale. (at: {name})"
            ));
        }
    }

    // type of gps time must match
    let gps_time_type = header.gps_time_type();
    for (i, reader) in readers.iter().enumerate() {
        if reader.header().gps_time_type() != gps_time_type {
            let name = options.input_file[i].display();
            return Err(anyhow!(
                "All input files must have identical gps time type. (at: {name})"
            ));
        }
    }

    // chunk size of 1 GiB (uncompressed)
    let chunk_size = 1024 * 1024 * 1024 / layout.size_of_point_entry() as usize;

    // create chunks
    info!("Chunking...");
    let mut chunker = Chunker {
        files: readers,
        layout: layout.clone(),
        chunk_size,
    };
    let mut merger = Merger::new(layout);
    while let Some(chunk) = chunker.read()? {
        let sorted = sort_points_incore(chunk);
        merger.add_input(sorted)?;
    }

    info!("Merging...");
    merger.merge_all()?;

    info!("Writing output file...");
    merger.write_output(&options.output_file, header)?;
    Ok(())
}

struct Chunker {
    files: Vec<LASReader<'static, BufReader<File>>>,
    layout: PointLayout,
    chunk_size: usize,
}

impl Chunker {
    fn read(&mut self) -> Result<Option<VectorBuffer>> {
        let mut chunk = VectorBuffer::with_capacity(self.chunk_size, self.layout.clone());
        while chunk.len() < self.chunk_size {
            let file = match self.files.last_mut() {
                Some(s) => s,
                None => break,
            };
            if file.remaining_points() == 0 {
                self.files.pop();
                continue;
            }
            let remaining_in_chunk = self.chunk_size - chunk.len();
            let remaining_in_file = file.remaining_points();
            let points_to_read = remaining_in_chunk.min(remaining_in_file);
            let points = file.read::<VectorBuffer>(points_to_read)?;
            chunk.append(&points);
        }
        if chunk.is_empty() {
            Ok(None)
        } else {
            Ok(Some(chunk))
        }
    }
}

fn sort_points_incore(points: VectorBuffer) -> VectorBuffer {
    // read gps times
    let mut gps_times: Vec<_> = points
        .view_attribute::<f64>(&GPS_TIME)
        .into_iter()
        .enumerate()
        .collect();

    // sort
    gps_times.sort_unstable_by_key(|(_, gps_time)| FloatOrd(*gps_time));

    // reorder points
    let mut result = VectorBuffer::with_capacity(points.len(), points.point_layout().clone());
    result.resize(points.len());
    for (to_index, (from_index, _)) in gps_times.into_iter().enumerate() {
        result
            .get_point_mut(to_index)
            .copy_from_slice(points.get_point_ref(from_index));
    }
    result
}

/// Wrapper for a float that implements Ord based on total_cmp
#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
struct FloatOrd(f64);

impl PartialOrd for FloatOrd {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FloatOrd {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl Eq for FloatOrd {}

impl PartialEq for FloatOrd {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

struct Merger {
    input: VecDeque<tempfile::NamedTempFile>,
    layout: PointLayout,
}

impl Merger {
    pub fn new(layout: PointLayout) -> Self {
        Merger {
            input: VecDeque::new(),
            layout,
        }
    }

    pub fn add_input(&mut self, points: VectorBuffer) -> Result<()> {
        assert!(*points.point_layout() == self.layout);
        let mut file = NamedTempFile::new()?;
        info!("Create chunk {}", file.path().display());
        let data = points.get_point_range_ref(0..points.len());
        file.write_all(data)?;
        self.input.push_back(file);
        Ok(())
    }

    pub fn merge_step(&mut self) -> Result<()> {
        let Some(mut f1_raw) = self.input.pop_front() else {
            return Ok(());
        };
        let Some(mut f2_raw) = self.input.pop_front() else {
            self.input.push_front(f1_raw);
            return Ok(());
        };
        f1_raw.seek(SeekFrom::Start(0))?;
        f2_raw.seek(SeekFrom::Start(0))?;
        let mut merged_raw = NamedTempFile::new()?;
        info!(
            "Merging {} and {} into {}",
            f1_raw.path().display(),
            f2_raw.path().display(),
            merged_raw.path().display()
        );

        let point_size = self.layout.size_of_point_entry() as usize;
        let mut merged = BufWriter::new(merged_raw.as_file_mut());
        let mut f1 = PeekOnePoint::new(point_size, f1_raw);
        let mut f2 = PeekOnePoint::new(point_size, f2_raw);

        let gps_time_attr = self
            .layout
            .get_attribute(&GPS_TIME)
            .expect("Missing gps time attribute")
            .clone();
        let read_gps_time = move |point: &[u8]| -> f64 {
            let mut gps_time = 0.0_f64;
            bytemuck::cast_slice_mut::<f64, u8>(slice::from_mut(&mut gps_time))
                .copy_from_slice(&point[gps_time_attr.byte_range_within_point()]);
            gps_time
        };

        loop {
            let p1 = f1.peek()?;
            let p2 = f2.peek()?;
            let take_which_one = match (p1, p2) {
                (Some(p1), Some(p2)) => {
                    let t1 = read_gps_time(p1);
                    let t2 = read_gps_time(p2);
                    t1 < t2
                }
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            if take_which_one {
                merged.write_all(p1.unwrap())?;
                f1.consume();
            } else {
                merged.write_all(p2.unwrap())?;
                f2.consume();
            }
        }

        merged.flush()?;
        drop(merged);
        self.input.push_back(merged_raw);
        Ok(())
    }

    pub fn merge_all(&mut self) -> Result<()> {
        while self.input.len() > 1 {
            self.merge_step()?;
        }
        Ok(())
    }

    pub fn write_output(self, file_name: &Path, mut header: Header) -> Result<()> {
        let mut wr = File::create_new(file_name)?;
        header.clear();
        header.clone().into_raw()?.write_to(&mut wr)?;
        wr.write_all(header.padding())?;
        for vlr in header.vlrs() {
            // fix: the reader does not correctly determine the end of the strings
            let mut vlr = vlr.clone();
            fix_vlr(&mut vlr);
            let raw = vlr.clone().into_raw(false)?;
            raw.write_to(&mut wr)?;
        }
        wr.write_all(header.vlr_padding())?;

        // assert that we are at the brginning of the point data now
        assert_eq!(
            header.clone().into_raw()?.offset_to_point_data as u64,
            wr.stream_position()?
        );

        let buf_size_points = 1024 * 1024 / self.layout.size_of_point_entry() as usize;
        let buf_size_bytes = buf_size_points * self.layout.size_of_point_entry() as usize;
        let mut buf = vec![0; buf_size_bytes];
        let mut points = VectorBuffer::new_from_layout(self.layout.clone());

        for mut file in self.input {
            let mut remaining_bytes = file.seek(SeekFrom::End(0))? as usize;
            file.seek(SeekFrom::Start(0))?;

            while remaining_bytes > 0 {
                let read_bytes = remaining_bytes.min(buf_size_bytes);
                let buf_slice = &mut buf[..read_bytes];
                file.read_exact(buf_slice)?;
                wr.write_all(buf_slice)?;
                remaining_bytes -= read_bytes;
                points.clear();
                unsafe { points.push_points(buf_slice) };
                let positions = points.view_attribute::<Point3<i32>>(&ATTRIBUTE_LOCAL_LAS_POSITION);
                let basic_flags = if points.point_layout().has_attribute(&ATTRIBUTE_BASIC_FLAGS) {
                    Some(points.view_attribute::<u8>(&ATTRIBUTE_BASIC_FLAGS))
                } else {
                    None
                };
                let extended_flags = if points
                    .point_layout()
                    .has_attribute(&ATTRIBUTE_EXTENDED_FLAGS)
                {
                    Some(points.view_attribute::<u16>(&ATTRIBUTE_EXTENDED_FLAGS))
                } else {
                    None
                };
                for i in 0..points.len() {
                    let pos = positions.at(i);
                    let mut return_number = 0;
                    if let Some(basic_flags) = &basic_flags {
                        return_number = basic_flags.at(i) & 0x07;
                    }
                    if let Some(extended_flags) = &extended_flags {
                        let value = u16::from_le(extended_flags.at(i));
                        return_number = (value & 0x000F) as u8;
                    }
                    header.add_point(&Point {
                        x: header.transforms().x.direct(pos.x),
                        y: header.transforms().y.direct(pos.y),
                        z: header.transforms().z.direct(pos.z),
                        return_number,
                        ..Default::default()
                    })
                }
            }
        }

        wr.write_all(header.point_padding())?;
        for evlr in header.evlrs() {
            let mut evlr = evlr.clone();
            fix_vlr(&mut evlr);

            evlr.clone().into_raw(true)?.write_to(&mut wr)?;
        }

        wr.seek(SeekFrom::Start(0))?;
        header.into_raw()?.write_to(&mut wr)?;

        wr.flush()?;
        Ok(())
    }
}

struct PeekOnePoint {
    reader: BufReader<NamedTempFile>,
    init: bool,
    data: Vec<u8>,
}

impl PeekOnePoint {
    pub fn new(point_size: usize, reader: NamedTempFile) -> Self {
        PeekOnePoint {
            reader: BufReader::new(reader),
            init: false,
            data: vec![0; point_size],
        }
    }

    pub fn consume(&mut self) {
        self.init = false;
    }

    pub fn peek(&mut self) -> Result<Option<&[u8]>> {
        if !self.init {
            match self.reader.read_exact(&mut self.data) {
                Ok(_) => (),
                Err(e) => {
                    if e.kind() == ErrorKind::UnexpectedEof {
                        return Ok(None);
                    } else {
                        return Err(e)?;
                    }
                }
            };
            self.init = true;
        }
        Ok(Some(&self.data))
    }
}

fn fix_vlr(vlr: &mut Vlr) {
    fix_str(&mut vlr.description, 32);
    fix_str(&mut vlr.user_id, 16);

    fn fix_str(str: &mut String, max_len: usize) {
        *str = str
            .split_once('\0')
            .map(|(s, _)| s.to_string())
            .unwrap_or(str.clone());
        if str.len() > max_len {
            *str = str[..max_len].to_string();
        }
    }
}