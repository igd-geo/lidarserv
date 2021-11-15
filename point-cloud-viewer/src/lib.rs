#![feature(result_flattening)]
//! This crate provides an interactive viewer for point clouds, such as those captured by terrestrial LiDAR scanners.

pub mod renderer;
pub mod navigation;

pub use crossbeam_channel;
