use anyhow::Result;
use anyhow::{anyhow, Context};
use std::io::BufRead;
use std::str::FromStr;

pub struct TrajectoryReader<R> {
    line: usize,
    inner: R,
}

pub struct PointReader<R> {
    line: usize,
    inner: R,
}

#[derive(Debug, Clone)]
pub struct TrajectoryCsvRecord {
    pub time_stamp: i32,
    pub distance: f64,
    pub easting: f64,
    pub northing: f64,
    pub altitude1: f64,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude2: f64,
    pub roll: f64,
    pub pitch: f64,
    pub heading: f64,
    pub velocity_easting: f64,
    pub velocity_northing: f64,
    pub velocity_down: f64,
}

#[derive(Debug, Clone)]
pub struct PointCsvRecord {
    pub time_stamp: f64,
    pub point_3d_x: f64,
    pub point_3d_y: f64,
    pub point_3d_z: f64,
    pub intensity: f64,
    pub polar_angle: f64,
}

impl<R> PointReader<R>
where
    R: BufRead,
{
    pub fn new(inner: R) -> Result<Self> {
        Ok(PointReader { line: 0, inner })
    }
    pub fn read_one(&mut self) -> Result<Option<PointCsvRecord>> {
        // read line
        self.line += 1;
        let mut line = "".to_string();
        let read = self
            .inner
            .read_line(&mut line)
            .with_context(|| format!("Points file, line {}: I/O Error", self.line))?;
        if read == 0 {
            // EOF
            return Ok(None);
        }

        // split columns
        let fields = line
            .trim_end_matches('\n')
            .trim()
            .split(' ')
            .collect::<Vec<_>>();
        if fields.len() != 6 {
            return Err(anyhow!(
                "Points file, line {}: Expecting 6 columns, got {}",
                self.line,
                fields.len()
            ));
        }

        // parse individual columns
        Ok(Some(PointCsvRecord {
            time_stamp: f64::from_str(fields[0]).with_context(|| {
                format!(
                    "Points file, line {}: Unable to parse field 'time_stamp'",
                    self.line
                )
            })?,
            point_3d_x: f64::from_str(fields[1]).with_context(|| {
                format!(
                    "Points file, line {}: Unable to parse field 'point_3d_x'",
                    self.line
                )
            })?,
            point_3d_y: f64::from_str(fields[2]).with_context(|| {
                format!(
                    "Points file, line {}: Unable to parse field 'point_3d_y'",
                    self.line
                )
            })?,
            point_3d_z: f64::from_str(fields[3]).with_context(|| {
                format!(
                    "Points file, line {}: Unable to parse field 'point_3d_z'",
                    self.line
                )
            })?,
            intensity: f64::from_str(fields[4]).with_context(|| {
                format!(
                    "Points file, line {}: Unable to parse field 'intensity'",
                    self.line
                )
            })?,
            polar_angle: f64::from_str(fields[5]).with_context(|| {
                format!(
                    "Points file, line {}: Unable to parse field 'polar_angle'",
                    self.line
                )
            })?,
        }))
    }
}

impl<R> TrajectoryReader<R>
where
    R: BufRead,
{
    pub fn new(inner: R) -> Result<Self> {
        Ok(TrajectoryReader { line: 0, inner })
    }

    pub fn read_one(&mut self) -> Result<Option<TrajectoryCsvRecord>> {
        // read line
        self.line += 1;
        let mut line = "".to_string();
        let read = self
            .inner
            .read_line(&mut line)
            .with_context(|| format!("Trajectory file, line {}: I/O Error", self.line))?;
        if read == 0 {
            // EOF
            return Ok(None);
        }

        // split columns
        let fields = line
            .trim_end_matches('\n')
            .trim()
            .split(' ')
            .collect::<Vec<_>>();
        if fields.len() != 14 {
            return Err(anyhow!(
                "Trajectory file, line {}: Expecting 14 columns, got {}",
                self.line,
                fields.len()
            ));
        }

        // parse individual columns
        Ok(Some(TrajectoryCsvRecord {
            time_stamp: i32::from_str(fields[0]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'time_stamp'",
                    self.line
                )
            })?,
            distance: f64::from_str(fields[1]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'distance'",
                    self.line
                )
            })?,
            easting: f64::from_str(fields[2]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'easting'",
                    self.line
                )
            })?,
            northing: f64::from_str(fields[3]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'northing'",
                    self.line
                )
            })?,
            altitude1: f64::from_str(fields[4]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'altitude1'",
                    self.line
                )
            })?,
            latitude: f64::from_str(fields[5]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'latitude'",
                    self.line
                )
            })?,
            longitude: f64::from_str(fields[6]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'longitude'",
                    self.line
                )
            })?,
            altitude2: f64::from_str(fields[7]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'altitude2'",
                    self.line
                )
            })?,
            roll: f64::from_str(fields[8]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'roll'",
                    self.line
                )
            })?,
            pitch: f64::from_str(fields[9]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'pitch'",
                    self.line
                )
            })?,
            heading: f64::from_str(fields[10]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'heading'",
                    self.line
                )
            })?,
            velocity_easting: f64::from_str(fields[11]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'velocity_easting'",
                    self.line
                )
            })?,
            velocity_northing: f64::from_str(fields[12]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'velocity_northing'",
                    self.line
                )
            })?,
            velocity_down: f64::from_str(fields[13]).with_context(|| {
                format!(
                    "Trajectory file, line {}: Unable to parse field 'velocity_down'",
                    self.line
                )
            })?,
        }))
    }
}

impl<R: BufRead> Iterator for TrajectoryReader<R> {
    type Item = Result<TrajectoryCsvRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_one() {
            Ok(Some(item)) => Some(Ok(item)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

impl<R: BufRead> Iterator for PointReader<R> {
    type Item = Result<PointCsvRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_one() {
            Ok(Some(item)) => Some(Ok(item)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
