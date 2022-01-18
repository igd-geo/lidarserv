use crate::velodyne_csv_reader::{PointReader, TrajectoryCsvRecord, TrajectoryReader};
use anyhow::Result;
use itertools::Itertools;
use lidarserv_server::common::geometry::points::PointType;
use lidarserv_server::common::geometry::position::F64Position;
use lidarserv_server::common::index::sensor_pos::point::SensorPositionAttribute;
use lidarserv_server::common::las::LasPointAttributes;
use lidarserv_server::common::nalgebra::{Matrix4, Vector3, Vector4};
use lidarserv_server::index::point::{GlobalPoint, GlobalSensorPositionAttribute};
use log::error;
use std::f64::consts::PI;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub fn iter_points(
    trajectory_file: &Path,
    points_file: &Path,
    offset: Vector3<f64>,
) -> Result<impl Iterator<Item = (f64, GlobalPoint)> + 'static> {
    let trajectory_file = BufReader::new(File::open(trajectory_file)?);
    let mut trajectory_iter = TrajectoryReader::new(trajectory_file)?
        .flat_map(|r| {
            if let Err(e) = &r {
                error!("Skip trajectory record: {}", e);
            }
            r
        })
        .group_into_vec_by(|it| it.time_stamp)
        .flat_map(|(time_stamp, items)| {
            let time_base = time_stamp as f64;
            let time_step = 1.0 / items.len() as f64;
            items
                .into_iter()
                .enumerate()
                .map(move |(index, item)| (time_base + time_step * index as f64, item))
        })
        .tuple_windows();

    let mut cur_trajectory_segment: Option<(
        (f64, TrajectoryCsvRecord),
        (f64, TrajectoryCsvRecord),
    )> = trajectory_iter.next();

    let points_file = BufReader::new(File::open(points_file)?);
    let points = PointReader::new(points_file)?
        .flat_map(|it| {
            if let Err(e) = &it {
                error!("Skip point record: {}", e);
            }
            it
        })
        .flat_map(move |point_record| {
            let t = point_record.time_stamp;
            let ((t1, traj1), (t2, traj2)) = loop {
                let v = match &cur_trajectory_segment {
                    None => {
                        return None;
                    }
                    Some(v) => v,
                };
                if t < v.0 .0 {
                    return None;
                }
                if t <= v.1 .0 {
                    break v;
                }
                cur_trajectory_segment = trajectory_iter.next();
            };
            let (t1, t2) = (*t1, *t2);
            let weight_1 = (t2 - t) / (t2 - t1);
            let weight_2 = (t - t1) / (t2 - t1);
            let interpolated = TrajectoryCsvRecord {
                time_stamp: traj1.time_stamp,
                distance: traj1.distance * weight_1 + traj2.distance * weight_2,
                easting: traj1.easting * weight_1 + traj2.easting * weight_2,
                northing: traj1.northing * weight_1 + traj2.northing * weight_2,
                altitude1: traj1.altitude1 * weight_1 + traj2.altitude1 * weight_2,
                latitude: traj1.latitude * weight_1 + traj2.latitude * weight_2,
                longitude: traj1.longitude * weight_1 + traj2.longitude * weight_2,
                altitude2: traj1.altitude2 * weight_1 + traj2.altitude2 * weight_2,
                roll: traj1.roll * weight_1 + traj2.roll * weight_2,
                pitch: traj1.pitch * weight_1 + traj2.pitch * weight_2,
                heading: traj1.heading * weight_1 + traj2.heading * weight_2,
                velocity_easting: traj1.velocity_easting * weight_1
                    + traj2.velocity_easting * weight_2,
                velocity_northing: traj1.velocity_northing * weight_1
                    + traj2.velocity_northing * weight_2,
                velocity_down: traj1.velocity_down * weight_1 + traj2.velocity_down * weight_2,
            };
            Some((interpolated, point_record))
        })
        .map(move |(traj, point)| {
            let trajectory_position =
                Vector3::new(traj.easting, traj.northing, traj.altitude1) - offset;
            let point_hom =
                Vector4::new(point.point_3d_x, point.point_3d_z, -point.point_3d_y, 1.0);
            let point_hom =
                Matrix4::new_rotation(Vector3::new(0.0, 0.0, -traj.heading / 360.0 * 2.0 * PI))
                    * point_hom;
            let point_position = point_hom.xyz() / point_hom.w;
            let position = trajectory_position + point_position;
            let mut global_point =
                GlobalPoint::new(F64Position::new(position.x, position.y, position.z));
            global_point.set_attribute(GlobalSensorPositionAttribute(F64Position::new(
                trajectory_position.x,
                trajectory_position.y,
                trajectory_position.z,
            )));
            global_point.set_attribute(LasPointAttributes {
                intensity: (point.intensity * u16::MAX as f64) as u16,
                ..Default::default()
            });
            (point.time_stamp, global_point)
        });

    Ok(points)
}

pub struct GroupVecByIterator<I, V, G, F> {
    inner: I,
    func: F,
    state: Option<(G, V)>,
}

pub trait IterExt: Iterator {
    fn group_into_vec_by<F, G>(self, func: F) -> GroupVecByIterator<Self, Self::Item, G, F>
    where
        Self: Sized,
        F: Fn(&Self::Item) -> G,
        G: PartialEq,
    {
        GroupVecByIterator::new(self, func)
    }
}

impl<I> IterExt for I where I: Iterator {}

impl<I, V, G, F> GroupVecByIterator<I, V, G, F>
where
    I: Iterator<Item = V>,
    F: Fn(&V) -> G,
{
    pub fn new(mut iterator: I, func: F) -> Self {
        GroupVecByIterator {
            state: iterator.next().map(|val| (func(&val), val)),
            inner: iterator,
            func,
        }
    }
}

impl<I, V, G, F> Iterator for GroupVecByIterator<I, V, G, F>
where
    I: Iterator<Item = V>,
    F: Fn(&V) -> G,
    G: PartialEq,
{
    type Item = (G, Vec<V>);

    fn next(&mut self) -> Option<Self::Item> {
        match self.state.take() {
            None => None,
            Some((group, first)) => {
                let mut result = vec![first];
                for item in &mut self.inner {
                    let item_group = (self.func)(&item);
                    if item_group == group {
                        result.push(item)
                    } else {
                        self.state = Some((item_group, item));
                        break;
                    }
                }
                Some((group, result))
            }
        }
    }
}
