use pasture_core::containers::{BorrowedBuffer, VectorBuffer};

use super::{PointSplitter, PointSplitterChunk};

/// A point splitter that assigns points to frames based on a fixed
/// point rate in points per second.
pub struct FixedPointRateSplitter {
    nr_points: u64,
    nr_points_chunk: u64,
    points_per_second: u32,
    frames_per_second: u32,
}

pub struct FixedPointRateChunkSplitter<'a> {
    state: &'a FixedPointRateSplitter,
    pos_in_chunk: u64,
}

impl FixedPointRateSplitter {
    pub fn init(points_per_second: u32, frames_per_second: u32) -> Self {
        FixedPointRateSplitter {
            nr_points: 0,
            nr_points_chunk: 0,
            frames_per_second,
            points_per_second,
        }
    }
}

impl PointSplitter for FixedPointRateSplitter {
    type ChunkSplitter<'a, 'b> = FixedPointRateChunkSplitter<'a>where Self: 'a;

    fn next_chunk<'a, 'b>(&'a mut self, points: &'b VectorBuffer) -> Self::ChunkSplitter<'a, 'b> {
        self.nr_points += self.nr_points_chunk;
        self.nr_points_chunk = points.len() as u64;
        FixedPointRateChunkSplitter {
            state: &*self,
            pos_in_chunk: 0,
        }
    }
}

impl<'a> PointSplitterChunk for FixedPointRateChunkSplitter<'a> {
    fn next_point(&mut self) -> u64 {
        self.pos_in_chunk += 1;
        let point = self.state.nr_points + self.pos_in_chunk;
        let time = point as f64 / self.state.points_per_second as f64;
        let frame = time * self.state.frames_per_second as f64;
        frame.floor() as u64
    }
}
