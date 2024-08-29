#![deny(unused_must_use)]

extern crate core;

mod f64_utils;
pub mod geometry;
pub mod index;
pub mod io;
pub mod lru_cache;
pub mod query;

pub use nalgebra;
