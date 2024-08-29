use std::sync::Arc;

use pasture_core::{
    containers::VectorBuffer,
    layout::{PointAttributeDefinition, PointLayout},
};
use thiserror::Error;

pub mod pasture;

#[derive(Debug, Error, Clone)]
pub enum PointIoError {
    #[error("I/O Error")]
    Io(#[from] IoError),

    #[error("Invalid points file: {0}")]
    DataFormat(String),

    #[error(
        "Point layout mismatch.\n\n  Expected attributes: {expected:#?}\n\n  Actual attributes: {actual:#?}"
    )]
    PointLayoutMismatch {
        expected: Vec<PointAttributeDefinition>,
        actual: Vec<PointAttributeDefinition>,
    },

    /// Some aspect of the point buffer exceeds what the file format can store.
    /// (For example, this could occur if the point buffer contains an attribute that is not supported by the file format)
    #[error("Invalid point buffer: {0}")]
    Unsupported(String),
}

/// Wrapper around std::io::Error,
/// that allows it to be "cloned" by putting it inside of an Arc.
#[derive(Debug, Clone)]
pub struct IoError(pub Arc<std::io::Error>);

impl std::error::Error for IoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl std::fmt::Display for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<std::io::Error> for IoError {
    fn from(value: std::io::Error) -> Self {
        Self(Arc::new(value))
    }
}

impl From<std::io::Error> for PointIoError {
    fn from(value: std::io::Error) -> Self {
        PointIoError::Io(value.into())
    }
}

pub trait PointCodec {
    fn write_points(
        &self,
        points: &VectorBuffer,
        wr: &mut impl std::io::Write,
    ) -> Result<(), PointIoError>;

    fn read_points(
        &self,
        rd: &mut impl std::io::Read,
        point_layout: &PointLayout,
    ) -> Result<VectorBuffer, PointIoError>;

    fn is_compatible_with(&self, other: &Self) -> bool;
}

pub trait InMemoryPointCodec {
    fn write_points(&self, points: &VectorBuffer, wr: &mut Vec<u8>) -> Result<(), PointIoError>;

    fn read_points<'a>(
        &self,
        rd: &'a [u8],
        point_layout: &PointLayout,
    ) -> Result<(VectorBuffer, &'a [u8]), PointIoError>;

    fn is_compatible_with(&self, other: &Self) -> bool
    where
        Self: Sized;
}

impl<T> InMemoryPointCodec for T
where
    T: PointCodec,
{
    fn write_points(&self, points: &VectorBuffer, wr: &mut Vec<u8>) -> Result<(), PointIoError> {
        PointCodec::write_points(self, points, wr)
    }

    fn read_points<'a>(
        &self,
        rd: &'a [u8],
        point_layout: &PointLayout,
    ) -> Result<(VectorBuffer, &'a [u8]), PointIoError> {
        let mut rd = rd;
        let result = PointCodec::read_points(self, &mut rd, point_layout);
        result.map(|points| (points, rd))
    }

    fn is_compatible_with(&self, other: &Self) -> bool {
        PointCodec::is_compatible_with(self, other)
    }
}
