use std::{any::Any, fmt::Display};

use anyhow::anyhow;
use lidarserv_common::geometry::coordinate_system::CoordinateSystem;
use lidarserv_input_file::extractors::{
    AttributeExtractor, basic_flags::LasBasicFlagsExtractor,
    basic_flags_downgrade::LasBasicFlagsDowngradeExtractor,
    classification_flags::ClassificationFlagsExtractor, copy::CopyExtractor,
    edge_of_flight_line::EdgeOfFlightLineExtractor, extended_flags::LasExtendedFlagsExtractor,
    extended_flags_upgrade::LasExtendedFlagsUpgradeExtractor, init_zero::InitZeroExtractor,
    number_of_returns_3bit::NumberOfReturns3BitExtractor,
    number_of_returns_4bit::NumberOfReturns4BitExtractor, position::PositionExtractor,
    return_number_3bit::ReturnNumber3BitExtractor, return_number_4bit::ReturnNumber4BitExtractor,
    scan_angle::ScanAngleExtractor, scan_angle_rank::ScanAngleRankExtractor,
    scan_direction_flag::ScanDirectionFlagExtractor, scanner_channel::ScannerChannelExtractor,
};
use log::warn;
use pasture_core::{
    containers::{BorrowedBuffer, InterleavedBuffer, VectorBuffer},
    layout::PointLayout,
    meta::Metadata,
};
use pasture_io::base::{PointReader, SeekToPoint};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[allow(dead_code)] // some variants are never constructed. But they are useful for the future.
pub enum MissingAttributesStrategy {
    ZeroInitialize,
    ZeroInitializeAndWarn,
    Fail,
}

pub struct Converter {
    extractors: Vec<Box<dyn AttributeExtractor + Send>>,
}

impl Converter {
    pub fn new(
        src_layout: &PointLayout,
        src_coordinate_system: CoordinateSystem,
        dst_layout: &PointLayout,
        dst_coordinate_system: CoordinateSystem,
        missing_attributes: MissingAttributesStrategy,
    ) -> Result<Self, anyhow::Error> {
        let mut extractors: Vec<Box<dyn AttributeExtractor + Send>> = Vec::new();
        let dst_point_size = dst_layout.size_of_point_entry() as usize;

        for dst_attribute in dst_layout.attributes() {
            // position
            if let Some(extractor) = PositionExtractor::check(
                dst_coordinate_system,
                dst_attribute,
                dst_point_size,
                src_layout,
                Some(src_coordinate_system),
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
                LasBasicFlagsDowngradeExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }
            if let Some(extractor) =
                LasBasicFlagsExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // extended flags
            if let Some(extractor) =
                LasExtendedFlagsUpgradeExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }
            if let Some(extractor) =
                LasExtendedFlagsExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // classification flags
            if let Some(extractor) =
                ClassificationFlagsExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // edge of flightline flag
            if let Some(extractor) =
                EdgeOfFlightLineExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // number of returns
            if let Some(extractor) =
                NumberOfReturns3BitExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }
            if let Some(extractor) =
                NumberOfReturns4BitExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // return number
            if let Some(extractor) =
                ReturnNumber3BitExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }
            if let Some(extractor) =
                ReturnNumber4BitExtractor::check(dst_attribute, dst_point_size, src_layout)
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

            // scan angle
            if let Some(extractor) =
                ScanAngleExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // scan direction flag
            if let Some(extractor) =
                ScanDirectionFlagExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // scanner channel
            if let Some(extractor) =
                ScannerChannelExtractor::check(dst_attribute, dst_point_size, src_layout)
            {
                extractors.push(Box::new(extractor));
                continue;
            }

            // no extractor left to try
            if missing_attributes == MissingAttributesStrategy::ZeroInitializeAndWarn {
                warn!(
                    "Missing attribute: {} (will be filled with zeros)",
                    dst_attribute.attribute_definition()
                );
            }
            match missing_attributes {
                MissingAttributesStrategy::ZeroInitialize
                | MissingAttributesStrategy::ZeroInitializeAndWarn => {
                    extractors.push(Box::new(InitZeroExtractor::create(
                        dst_attribute,
                        dst_point_size,
                    )));
                    continue;
                }
                MissingAttributesStrategy::Fail => {
                    return Err(anyhow::anyhow!(
                        "Missing attribute: {}",
                        dst_attribute.attribute_definition()
                    ));
                }
            }
        }

        Ok(Converter { extractors })
    }

    pub fn convert(&self, src: &[u8], dst: &mut [u8]) {
        for extractor in &self.extractors {
            extractor.extract(src, dst);
        }
    }
}

pub struct ConvertingPointReader<Inner> {
    inner: Inner,
    converter: Converter,
    dst_layout: PointLayout,
}

impl<Inner> ConvertingPointReader<Inner>
where
    Inner: PointReader,
{
    pub fn new(
        src_reader: Inner,
        src_coordinate_system: CoordinateSystem,
        dst_layout: PointLayout,
        dst_coordinate_system: CoordinateSystem,
        missing_attributes: MissingAttributesStrategy,
    ) -> Result<Self, anyhow::Error> {
        let converter = Converter::new(
            src_reader.get_default_point_layout(),
            src_coordinate_system,
            &dst_layout,
            dst_coordinate_system,
            missing_attributes,
        )?;
        Ok(ConvertingPointReader {
            inner: src_reader,
            converter,
            dst_layout,
        })
    }
}

impl<Inner> SeekToPoint for ConvertingPointReader<Inner>
where
    Inner: SeekToPoint,
{
    fn seek_point(&mut self, position: std::io::SeekFrom) -> anyhow::Result<usize> {
        self.inner.seek_point(position)
    }

    fn point_index(&mut self) -> anyhow::Result<usize> {
        self.inner.point_count()
    }

    fn point_count(&mut self) -> anyhow::Result<usize> {
        self.inner.point_count()
    }
}

impl<Inner> PointReader for ConvertingPointReader<Inner>
where
    Inner: PointReader,
{
    fn read_into<B: pasture_core::containers::BorrowedMutBuffer>(
        &mut self,
        point_buffer: &mut B,
        count: usize,
    ) -> anyhow::Result<usize> {
        let intermediate_buf: VectorBuffer = self.inner.read(count)?;
        let nr_points = intermediate_buf.len();
        let src = intermediate_buf.get_point_range_ref(0..nr_points);

        let Some(out_buf) = point_buffer.as_interleaved_mut() else {
            return Err(anyhow!("Not implemented. Buffer must be interleaved."));
        };
        let dst = out_buf.get_point_range_mut(0..nr_points);

        self.converter.convert(src, dst);

        Ok(nr_points)
    }

    fn get_metadata(&self) -> &dyn Metadata {
        self
    }

    fn get_default_point_layout(&self) -> &PointLayout {
        &self.dst_layout
    }
}

impl<Inner> Metadata for ConvertingPointReader<Inner>
where
    Inner: PointReader,
{
    fn bounds(&self) -> Option<pasture_core::math::AABB<f64>> {
        None
    }

    fn number_of_points(&self) -> Option<usize> {
        self.inner.get_metadata().number_of_points()
    }

    fn get_named_field(&self, _field_name: &str) -> Option<Box<dyn Any>> {
        None
    }

    fn clone_into_box(&self) -> Box<dyn Metadata> {
        Box::new(ClonedMeta {
            inner: self.inner.get_metadata().clone_into_box(),
        })
    }
}

impl<Inner> Display for ConvertingPointReader<Inner> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "()")
    }
}

struct ClonedMeta {
    inner: Box<dyn Metadata>,
}

impl Display for ClonedMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "()")
    }
}

impl Metadata for ClonedMeta {
    fn bounds(&self) -> Option<pasture_core::math::AABB<f64>> {
        None
    }

    fn number_of_points(&self) -> Option<usize> {
        self.inner.number_of_points()
    }

    fn get_named_field(&self, _field_name: &str) -> Option<Box<dyn std::any::Any>> {
        None
    }

    fn clone_into_box(&self) -> Box<dyn Metadata> {
        Box::new(ClonedMeta {
            inner: self.inner.clone_into_box(),
        })
    }
}
