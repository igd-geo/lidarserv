use crate::cli::ReplayOptions;
use anyhow::{Result, anyhow};
use lidarserv_common::geometry::coordinate_system::CoordinateSystem;
use lidarserv_common::tracy_client::{plot, span};
use lidarserv_input_file::extractors::AttributeExtractor;
use lidarserv_input_file::extractors::basic_flags::LasBasicFlagsExtractor;
use lidarserv_input_file::extractors::copy::CopyExtractor;
use lidarserv_input_file::extractors::extended_flags::LasExtendedFlagsExtractor;
use lidarserv_input_file::extractors::position::PositionExtractor;
use lidarserv_input_file::extractors::scan_angle_rank::ScanAngleRankExtractor;
use lidarserv_input_file::splitters::fixed::FixedPointRateSplitter;
use lidarserv_input_file::splitters::gpstime::GpsTimeSplitter;
use lidarserv_input_file::splitters::{PointSplitter, PointSplitterChunk};
use lidarserv_server::net::client::capture_device::CaptureDeviceClient;
use log::error;
use pasture_core::containers::{
    InterleavedBuffer, InterleavedBufferMut, MakeBufferFromLayout, OwningBuffer, SliceBuffer,
};
use pasture_core::layout::{PointAttributeDefinition, PointLayout};
use pasture_core::{
    containers::{BorrowedBuffer, VectorBuffer},
    layout::attributes::GPS_TIME,
};
use pasture_io::{base::PointReader, las::LASReader};
use std::convert::Infallible;
use std::mem;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::{thread, time::Duration};
use tokio::time::Instant;
use tokio::{sync::broadcast, sync::mpsc, time::sleep};

pub async fn replay(options: ReplayOptions) -> Result<()> {
    // connect to server
    let (_shutdown_tx, mut shutdown_rx) = broadcast::channel(1);
    let client =
        CaptureDeviceClient::connect((options.host, options.port), &mut shutdown_rx).await?;

    let read = LASReader::from_path(&options.file, false)?;
    let src_layout = read.get_default_point_layout().clone();
    let buffer_size = 10 * options.fps as usize; // buffer over 10 seconds of point data
    let progress_state = Arc::new(ReplayState {
        buffer_size: buffer_size as u64,
        points_full: read.remaining_points() as u64,
        ..Default::default()
    });

    // report progress
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();
    let report_thread_handle = {
        let progress_state = Arc::clone(&progress_state);
        thread::spawn(move || report_thread(shutdown_rx, progress_state))
    };

    // read file
    let (points_tx, points_rx) = std::sync::mpsc::sync_channel(10);
    let read_thread_handle = thread::spawn(move || read_thread(points_tx, read));

    let (frames_tx, frames_rx) = tokio::sync::mpsc::channel(buffer_size);
    let convert_thread_handle = {
        let attributes = client.attributes().to_owned();
        let coordinate_system = client.coordinate_system();
        let progress_state = Arc::clone(&progress_state);
        if let Some(pps) = options.points_per_second {
            let splitter = FixedPointRateSplitter::init(pps, options.fps);
            thread::spawn(move || {
                convert_thread(
                    frames_tx,
                    coordinate_system,
                    points_rx,
                    splitter,
                    attributes,
                    progress_state,
                    src_layout,
                )
            })
        } else {
            let has_gps_time = src_layout.has_attribute(&GPS_TIME);
            if !has_gps_time {
                return Err(anyhow!(
                    "Missing {GPS_TIME} point attribute. (Note: You could specify the '--points-per-second' option to replay the file at a fixed point rate.)"
                ));
            }
            let splitter = GpsTimeSplitter::init(options.fps, options.autoskip, options.accelerate);
            thread::spawn(move || {
                convert_thread(
                    frames_tx,
                    coordinate_system,
                    points_rx,
                    splitter,
                    attributes,
                    progress_state,
                    src_layout,
                )
            })
        }
    };
    sleep(Duration::from_secs(1)).await; // 1 second head start for the read thread so that the buffers can fill up

    send_thread(frames_rx, options.fps, client, progress_state).await;

    // shutdown
    convert_thread_handle.join().unwrap();
    read_thread_handle.join().unwrap();
    drop(shutdown_tx);
    report_thread_handle.join().unwrap();

    Ok(())
}

fn read_thread(points_tx: std::sync::mpsc::SyncSender<VectorBuffer>, mut read: impl PointReader) {
    loop {
        let _s1 = span!("read_thread read");
        let chunk = match read.read::<VectorBuffer>(50_000) {
            Ok(o) => o,
            Err(e) => {
                error!("Error while reading points: {e}");
                break;
            }
        };
        if chunk.is_empty() {
            // reached EOF
            break;
        }
        drop(_s1);

        let _s2 = span!("read_thread send");
        if points_tx.send(chunk).is_err() {
            // receiver disconnected
            break;
        }
        drop(_s2);
    }
}

fn convert_thread(
    frames_tx: mpsc::Sender<(u64, VectorBuffer)>,
    coordinate_system: CoordinateSystem,
    points_rx: std::sync::mpsc::Receiver<VectorBuffer>,
    mut splitter: impl PointSplitter,
    attributes: Vec<PointAttributeDefinition>,
    state: Arc<ReplayState>,
    src_layout: PointLayout,
) {
    let mut frame_manager = match FrameManager::new(
        &src_layout,
        &attributes,
        &coordinate_system,
        frames_tx,
        state,
    ) {
        Ok(o) => o,
        Err(e) => {
            error!("{e}");
            return;
        }
    };
    loop {
        let _s1 = span!("convert_thread receive chunk");
        let chunk = match points_rx.recv() {
            Ok(o) => o,
            Err(_) => break,
        };
        drop(_s1);

        struct CurrentFrame {
            frame_number: u64,
            start_position: usize,
        }
        let nr_points = chunk.len();
        let mut splitter = splitter.next_chunk(&chunk);
        let mut current_frame = None;
        let _s2 = span!("convert_thread process chunk");
        for point in 0..nr_points {
            let frame_number = splitter.next_point();

            match &mut current_frame {
                None => {
                    current_frame = Some(CurrentFrame {
                        frame_number,
                        start_position: point,
                    })
                }
                Some(cf) => {
                    if cf.frame_number != frame_number {
                        let points = chunk.slice(cf.start_position..point);
                        match frame_manager.add_points(cf.frame_number, &points) {
                            Ok(_) => (),
                            Err(FrameManagerError::SendError(_)) => return,
                        }
                        cf.frame_number = frame_number;
                        cf.start_position = point;
                    }
                }
            }
        }
        if let Some(cf) = current_frame {
            let points = chunk.slice(cf.start_position..chunk.len());
            match frame_manager.add_points(cf.frame_number, &points) {
                Ok(_) => (),
                Err(FrameManagerError::SendError(_)) => return,
            }
        }
        drop(_s2);
    }
}

struct FrameManager {
    current_frame_number: u64,
    current_frame_points: VectorBuffer,
    frames_tx: mpsc::Sender<(u64, VectorBuffer)>,
    extractors: Vec<Box<dyn AttributeExtractor>>,
    state: Arc<ReplayState>,
}

impl FrameManager {
    fn new(
        src_layout: &PointLayout,
        attributes: &[PointAttributeDefinition],
        dst_coordinate_system: &CoordinateSystem,
        frames_tx: mpsc::Sender<(u64, VectorBuffer)>,
        state: Arc<ReplayState>,
    ) -> Result<Self> {
        let dst_layout = PointLayout::from_attributes(attributes);
        let dst_point_size = dst_layout.size_of_point_entry() as usize;
        let mut extractors: Vec<Box<dyn AttributeExtractor>> = Vec::new();
        for dst_attribute in dst_layout.attributes() {
            // position
            if let Some(extractor) = PositionExtractor::check(
                *dst_coordinate_system,
                dst_attribute,
                dst_point_size,
                src_layout,
                None,
            ) {
                extractors.push(Box::new(extractor));
                continue;
            }

            // most normal attributes: bitwise copy
            if let Some(extractor) = CopyExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // basic flags
            if let Some(extractor) =
                LasBasicFlagsExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // extended flags
            if let Some(extractor) =
                LasExtendedFlagsExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // scan angle rank
            if let Some(extractor) =
                ScanAngleRankExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // no extractor left to try for this attribute
            return Err(anyhow!(
                "Missing attribute: {}",
                dst_attribute.attribute_definition()
            ));
        }

        Ok(FrameManager {
            current_frame_number: 0,
            current_frame_points: VectorBuffer::new_from_layout(dst_layout),
            frames_tx,
            extractors,
            state,
        })
    }

    fn flush(&mut self) -> Result<(), FrameManagerError> {
        if !self.current_frame_points.is_empty() {
            let layout = self.current_frame_points.point_layout().clone();
            let capacity = self.current_frame_points.len() * 2;
            let next_points = VectorBuffer::with_capacity(capacity, layout);
            let points = mem::replace(&mut self.current_frame_points, next_points);
            let frame_number = self.current_frame_number;
            self.state.frames_read.fetch_add(1, Ordering::Relaxed);
            self.frames_tx.blocking_send((frame_number, points))?;
        }
        Ok(())
    }

    fn add_points<T: InterleavedBuffer>(
        &mut self,
        frame_number: u64,
        points: &T,
    ) -> Result<(), FrameManagerError> {
        let _s1 = span!("FrameManager::add_points");
        assert!(frame_number >= self.current_frame_number);
        if self.current_frame_number != frame_number {
            let _s2 = span!("FrameManager::add_points flush");
            self.flush()?;
            self.current_frame_number = frame_number;
            drop(_s2);
        }

        let offset = self.current_frame_points.len();
        let new_len = offset + points.len();
        self.current_frame_points.resize(new_len);
        let dst = self
            .current_frame_points
            .get_point_range_mut(offset..new_len);
        let src = points.get_point_range_ref(0..points.len());
        let _s3 = span!("FrameManager::add_points extract");
        for extractor in &self.extractors {
            extractor.extract(src, dst);
        }
        drop(_s3);
        Ok(())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
enum FrameManagerError {
    #[error("MPSC Channel is closed.")]
    SendError(#[from] mpsc::error::SendError<(u64, VectorBuffer)>),
}

async fn send_thread(
    mut frames_rx: mpsc::Receiver<(u64, VectorBuffer)>,
    fps: u32,
    mut client: CaptureDeviceClient,
    state: Arc<ReplayState>,
) {
    let time_start = Instant::now();

    while let Some((frame_number, points)) = frames_rx.recv().await {
        let time_frame = time_start + Duration::from_secs_f64(frame_number as f64 / fps as f64);
        let time_now = Instant::now();

        // wait for frame timing
        let _s1 = span!("sleeping");
        if time_now < time_frame {
            let wait_for = time_frame - time_now;
            state.behind_by.store(0, Ordering::Relaxed);
            sleep(wait_for).await
        } else {
            let behind_by = ((time_now - time_frame).as_secs_f64() * 10.0).round() as u64;
            state.behind_by.store(behind_by, Ordering::Relaxed);
        }
        drop(_s1);

        // send
        match client.insert_points_local_coordinates(&points).await {
            Ok(_) => (),
            Err(e) => {
                error!("{e}");
                return;
            }
        }

        // update progress state
        state.frames_sent.fetch_add(1, Ordering::Relaxed);
        state
            .points_sent
            .fetch_add(points.len() as u64, Ordering::Relaxed);
    }
}

#[derive(Debug, Default)]
struct ReplayState {
    points_sent: AtomicU64,
    frames_sent: AtomicU64,
    frames_read: AtomicU64,
    behind_by: AtomicU64,
    points_full: u64,
    buffer_size: u64,
}

fn report_thread(shutdown_rx: std::sync::mpsc::Receiver<Infallible>, state: Arc<ReplayState>) {
    let mut last_points_sent = 0;
    let mut last_frames_sent = 0;

    loop {
        // sleep
        match shutdown_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(_) => break,
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => (),
        }

        let points_sent = state.points_sent.load(Ordering::Relaxed);
        let frames_sent = state.frames_sent.load(Ordering::Relaxed);
        let frames_read = state.frames_read.load(Ordering::Relaxed);
        let points_total = state.points_full;

        let points_percentage = (points_sent as f64 / points_total as f64).clamp(0.0, 1.0);
        let progress_bar = make_progress_bar(points_percentage, 10);

        let points_per_second = points_sent - last_points_sent;
        let frames_per_second = frames_sent - last_frames_sent;
        last_points_sent = points_sent;
        last_frames_sent = frames_sent;

        let buffered_frames = frames_read - frames_sent;
        let buffer_percentage = (buffered_frames as f64 / state.buffer_size as f64 * 100.0)
            .clamp(0.0, 100.0)
            .round() as u8;

        let behind_by = state.behind_by.load(Ordering::Relaxed);
        let behind_str = if behind_by < 10 {
            "".to_string()
        } else {
            format!(
                "[ !!! Too slow !!! {:.1}s behind !!! ]",
                behind_by as f64 * 0.1
            )
        };

        println!(
            "[fps: {frames_per_second:2} pps: {points_per_second:7}][buffer: {buffer_percentage:3}%][{progress_bar}| {points_sent}/{points_total} points sent]{behind_str}"
        );
        plot!("fps", frames_per_second as f64);
        plot!("pps", points_per_second as f64);
        plot!("buffered frames", buffered_frames as f64);
        plot!("behind by", behind_by as f64 * 0.1);
    }
}

fn make_progress_bar(progress: f64, width: usize) -> String {
    let blocks = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
    (0..width)
        .map(|i| {
            let left = i as f64 / width as f64;
            let right = (i + 1) as f64 / width as f64;
            if progress <= left {
                blocks[0]
            } else if progress >= right {
                blocks[8]
            } else {
                let filling = (progress - left) / (right - left);
                let subblocks = (filling * 8.0).round().clamp(0.0, 8.0) as usize;
                blocks[subblocks]
            }
        })
        .collect()
}
