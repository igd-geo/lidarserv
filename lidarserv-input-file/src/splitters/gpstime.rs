use super::{PointSplitter, PointSplitterChunk};
use pasture_core::{
    containers::{AttributeView, BorrowedBufferExt, VectorBuffer},
    layout::attributes::GPS_TIME,
};
use std::mem;

/// Splits a stream of points into frames based on the gps time attribute.
///
/// The points must have the [GPS_TIME] attribute. Otherwise the splitter will panick.
pub struct GpsTimeSplitter {
    fps: f64,
    autoskip: bool,
    accelerate: f64,
    last_point_time: Option<f64>,
    base_frame: u64,
    base_time: f64,
}

pub struct GpsTimeChunkSplitter<'a, 'b> {
    state: &'a mut GpsTimeSplitter,
    pos: usize,
    gps_time_attribute: AttributeView<'b, VectorBuffer, f64>,
}

impl GpsTimeSplitter {
    pub fn init(fps: u32, autoskip: bool, accelerate: f64) -> Self {
        GpsTimeSplitter {
            fps: fps as f64,
            autoskip,
            accelerate,
            last_point_time: None,
            base_frame: 0,
            base_time: 0.0,
        }
    }
}

impl PointSplitter for GpsTimeSplitter {
    type ChunkSplitter<'a, 'b>
    = GpsTimeChunkSplitter<'a, 'b>
    where
        Self: 'a;

    fn next_chunk<'a, 'b>(&'a mut self, points: &'b VectorBuffer) -> Self::ChunkSplitter<'a, 'b> {
        let gps_time_attribute = points.view_attribute::<f64>(&GPS_TIME);
        GpsTimeChunkSplitter {
            state: self,
            pos: 0,
            gps_time_attribute,
        }
    }
}

impl<'a, 'b> PointSplitterChunk for GpsTimeChunkSplitter<'a, 'b> {
    fn next_point(&mut self) -> u64 {
        // read gps time of next point
        let mut gps_time = self.gps_time_attribute.at(self.pos);
        self.pos += 1;

        // apply speed factor
        gps_time /= self.state.accelerate;

        // get last
        let last = mem::replace(&mut self.state.last_point_time, Some(gps_time));
        let Some(last_gps_time) = last else {
            self.state.base_frame = 0;
            self.state.base_time = gps_time;
            return 0;
        };
        let last_frame = ((last_gps_time - self.state.base_time) * self.state.fps).floor() as u64
            + self.state.base_frame;

        // handle jumps backwards in time
        if gps_time < last_gps_time {
            let frame_time = 1.0 / self.state.fps;
            self.state.base_frame = last_frame + 1;
            self.state.base_time = gps_time - 0.5 * frame_time;
            return self.state.base_frame;
        }

        // handle jumps forward in time
        if self.state.autoskip && gps_time > last_gps_time + 1.0 {
            self.state.base_frame = last_frame + 1;
            self.state.base_time = gps_time;
            return self.state.base_frame;
        }

        // calculate the frame number
        ((gps_time - self.state.base_time) * self.state.fps).floor() as u64 + self.state.base_frame
    }
}
