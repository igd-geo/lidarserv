use std::error::Error;
use std::io::BufReader;

pub fn main() -> Result<(), Box<dyn Error>> {
    let mut counts = Vec::new();
    let reader = std::fs::File::open(
        "/home/tobias/Downloads/20210427_messjob/20210427_mess3/IAPS_20210427_162821.txt",
    )?;
    let buf_reader = BufReader::new(reader);
    let points = file_replay::file_reader::PointReader::new(buf_reader)?;
    for point in points {
        let point = point?;
        let dist =
            (point.point_3d_x.powi(2) + point.point_3d_y.powi(2) + point.point_3d_z.powi(2)).sqrt();
        let bucket = dist.floor() as usize;
        while counts.len() <= bucket {
            counts.push(0);
        }
        counts[bucket] += 1;
    }
    for (bucket, counts) in counts.into_iter().enumerate() {
        println!("{} {}", bucket, counts)
    }
    Ok(())
}
