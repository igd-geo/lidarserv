use lidarserv_common::geometry::bounding_box::OptionAABB;
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::las::{I32LasReadWrite, Las, LasReadWrite};
use lidarserv_common::utils::thread_pool::Threads;
use lidarserv_server::index::point::LasPoint;
use std::io::Cursor;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn main() {
    let las_loader = I32LasReadWrite::new(true);
    let data = std::fs::read(
        "/home/tobias/Documents/studium/master/lidarserver/data/bvg/10__10__731--892-27.laz",
    )
    .unwrap();
    let las: Las<Vec<LasPoint>, i32, I32CoordinateSystem> =
        las_loader.read_las(Cursor::new(data)).unwrap();
    println!("Points: {}", las.points.len());

    measure("Single threaded", || {
        las_write_single_threaded(
            &las.points,
            las.bounds.clone(),
            las.coordinate_system.clone(),
        )
    });

    let mut threads_1 = Threads::new(1);
    let mut threads_2 = Threads::new(2);
    let mut threads_3 = Threads::new(3);
    let mut threads_4 = Threads::new(4);
    let mut threads_8 = Threads::new(8);
    measure("1 thread", || {
        las_write_parallel(
            &las.points,
            las.bounds.clone(),
            las.coordinate_system.clone(),
            &mut threads_1,
        )
    });
    measure("2 threads", || {
        las_write_parallel(
            &las.points,
            las.bounds.clone(),
            las.coordinate_system.clone(),
            &mut threads_2,
        )
    });
    measure("3 threads", || {
        las_write_parallel(
            &las.points,
            las.bounds.clone(),
            las.coordinate_system.clone(),
            &mut threads_3,
        )
    });

    measure("4 threads", || {
        las_write_parallel(
            &las.points,
            las.bounds.clone(),
            las.coordinate_system.clone(),
            &mut threads_4,
        )
    });
    measure("8 threads", || {
        las_write_parallel(
            &las.points,
            las.bounds.clone(),
            las.coordinate_system.clone(),
            &mut threads_8,
        )
    });

    // measure("overhead 1", || thread_overhead(&mut threads_1));
    // measure("overhead 2", || thread_overhead(&mut threads_2));
    // measure("overhead 3", || thread_overhead(&mut threads_3));
    // measure("overhead 4", || thread_overhead(&mut threads_4));
}

fn measure(name: &str, mut f: impl FnMut() -> (Duration, u8)) {
    let mut d = Duration::new(0, 0);
    let mut s = 0;
    // warmup
    //while d < Duration::from_secs(2) {
    //    let (id, is) = f();
    //    d += id;
    //    s += is;
    //}
    //d = Duration::new(0, 0);
    sleep(Duration::from_secs(10));
    for _ in 0..100 {
        let (id, is) = f();
        d += id;
        s += is;
    }
    println!("{}", s);
    println!("{}: {} sec", name, d.as_secs_f64());
}

fn las_write_single_threaded(
    points: &Vec<LasPoint>,
    bounds: OptionAABB<i32>,
    coordinate_system: I32CoordinateSystem,
) -> (Duration, u8) {
    let las_loader = I32LasReadWrite::new(true);

    let t1 = Instant::now();
    let mut write_to = Vec::new();
    LasReadWrite::<LasPoint, I32CoordinateSystem>::write_las(
        &las_loader,
        Las {
            points: points.iter(),
            non_bogus_points: None,
            bounds,
            coordinate_system,
        },
        Cursor::new(&mut write_to),
    )
    .unwrap();
    let t2 = Instant::now();
    (t2 - t1, write_to.into_iter().sum::<u8>())
}

fn las_write_parallel(
    points: &Vec<LasPoint>,
    bounds: OptionAABB<i32>,
    coordinate_system: I32CoordinateSystem,
    thread_pool: &mut Threads,
) -> (Duration, u8) {
    let las_loader = I32LasReadWrite::new(true);
    let t1 = Instant::now();
    let points = points.clone();
    let t2 = Instant::now();
    let write_to = LasReadWrite::<LasPoint, I32CoordinateSystem>::write_las_par(
        &las_loader,
        Las {
            &points,
            non_bogus_points: None,
            bounds,
            coordinate_system,
        },
        thread_pool,
    );
    let t3 = Instant::now();
    (t3 - t2, write_to.into_iter().sum::<u8>())
}

fn thread_overhead(thread_pool: &mut Threads) -> (Duration, u8) {
    let t1 = Instant::now();
    let state = AtomicU8::new(0);
    thread_pool
        .execute(|tid| {
            state.fetch_add(tid as u8, Ordering::AcqRel);
        })
        .join();
    let t2 = Instant::now();
    (t2 - t1, state.into_inner())
}
