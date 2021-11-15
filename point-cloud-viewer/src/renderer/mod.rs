//! Contains everything related to drawing point clouds.
//!
//! The renderer is split into two parts: A render backend backend is doing the actual
//! heavylifting, while the frontend provides a convenient to use interface.
mod renderer_command;
mod vertex_data;

pub mod error;
pub mod backends;
pub mod viewer;
pub mod settings;
