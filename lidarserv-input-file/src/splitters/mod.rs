use pasture_core::containers::VectorBuffer;

pub mod fixed;
pub mod gpstime;

/// Splits a stream of points into frames for the lidarserv server.
/// The points are processed chunk-wise.
pub trait PointSplitter {
    type ChunkSplitter<'a, 'b>: PointSplitterChunk
    where
        Self: 'a;

    /// Start processing the next chunk of points.
    fn next_chunk<'a, 'b>(&'a mut self, points: &'b VectorBuffer) -> Self::ChunkSplitter<'a, 'b>;
}

pub trait PointSplitterChunk {
    /// "Consumes" one point from the chunk and returns its frame number.
    ///
    /// This must not be called more times as there are points in the chunk.
    /// Doing so would be undefined behaviour.
    /// (Implementations are e.g. allowed to panick in this case.)
    fn next_point(&mut self) -> u64;
}
