//! A collection of helpers, that allow glium to be used in a more dynamic manner.
//!
//! This is mostly needed for drawing the point clouds:
//! The shaders for the point clouds are built at runtime based on the render settings,
//! and depending on these settings, other inputs (uniforms, vertex data, etc...) are needed.
//! Since the render settings are only known at runtime, we cannot define these things at compile
//! time, as "vanilla glium" would expect us to do.

use glium::VertexFormat;
use glium::uniforms::{AsUniformValue, UniformValue, Uniforms};
use glium::vertex::{MultiVerticesSource, VerticesSource};
use std::borrow::Cow;

/// Allows passing uniforms to glium, that are built dynamically at runtime.
/// (The [glium::uniform!] macro requires the uniforms to be known at compile time.)
///
/// Example:
///
/// ```ignore
/// let mut uniforms = DynamicUniforms::new();
/// uniforms.add("color", [1.0, 0.5, 0.0]);
/// uniforms.add("point_size", 2.0);
///
/// surface.draw(
///     // ...
///     &uniforms,
///     // ...
/// )
/// ```
#[derive(Clone)]
pub struct DynamicUniforms<'a> {
    uniforms: Vec<(&'static str, UniformValue<'a>)>,
}

impl<'a> DynamicUniforms<'a> {
    /// Creates a new, empty set of uniforms.
    pub fn new() -> Self {
        DynamicUniforms { uniforms: vec![] }
    }

    /// Adds a uniform to the collection.
    pub fn add<T>(&mut self, name: &'static str, value: &'a T)
    where
        T: AsUniformValue,
    {
        self.uniforms.push((name, value.as_uniform_value()))
    }

    /// Adds a uniform to the collection.
    pub fn add_uniform_value(&mut self, name: &'static str, value: UniformValue<'a>) {
        self.uniforms.push((name, value))
    }
}

impl Uniforms for DynamicUniforms<'_> {
    fn visit_values<'a, F: FnMut(&str, UniformValue<'a>)>(&'a self, mut fun: F) {
        for &(name, value) in &self.uniforms {
            fun(name, value);
        }
    }
}

/// Allows to pass a list of vertex sources to the glium draw call, that is built at runtime.
pub struct DynamicMultiVerticesSource<'a> {
    vertex_sources: Vec<VerticesSource<'a>>,
}

impl<'a> DynamicMultiVerticesSource<'a> {
    pub fn new() -> Self {
        DynamicMultiVerticesSource {
            vertex_sources: Vec::new(),
        }
    }

    pub fn add<T>(&mut self, source: T)
    where
        T: Into<VerticesSource<'a>>,
    {
        self.vertex_sources.push(source.into());
    }
}

impl<'a> MultiVerticesSource<'a> for DynamicMultiVerticesSource<'a> {
    type Iterator = <Vec<VerticesSource<'a>> as IntoIterator>::IntoIter;

    fn iter(self) -> Self::Iterator {
        self.vertex_sources.into_iter()
    }
}

/// Allows to adapt the vertex format of a vertices source at runtime.
/// Main use case is to use dynamically defined names for the vertex attributes.
pub struct ModifyVertexFormat<'a> {
    new_vertex_format: Option<VertexFormat>,
    old_vertices_source: VerticesSource<'a>,
}

impl<'a> ModifyVertexFormat<'a> {
    pub fn rename_bindings<T>(base: T, new_names: &[&'static str]) -> ModifyVertexFormat<'a>
    where
        T: Into<VerticesSource<'a>>,
    {
        let old_vertices_source = base.into();

        let new_vertex_format = match old_vertices_source {
            VerticesSource::VertexBuffer(_, vert_format, _) => {
                let mut new_vert_format = Cow::clone(vert_format);
                assert_eq!(new_vert_format.len(), new_names.len());
                new_vert_format
                    .to_mut()
                    .iter_mut()
                    .zip(new_names.iter())
                    .for_each(|((name, _, _, _, _), &b)| {
                        *name = Cow::from(b);
                    });
                Some(new_vert_format)
            }
            VerticesSource::Marker { .. } => None,
        };

        Self {
            new_vertex_format,
            old_vertices_source,
        }
    }
}

impl<'b> From<&'b ModifyVertexFormat<'b>> for VerticesSource<'b> {
    fn from(i: &'b ModifyVertexFormat<'b>) -> Self {
        match i.old_vertices_source {
            VerticesSource::VertexBuffer(buffer, _vertex_format, is_per_instance) => {
                VerticesSource::VertexBuffer(
                    buffer,
                    i.new_vertex_format.as_ref().unwrap(),
                    is_per_instance,
                )
            }
            VerticesSource::Marker { len, per_instance } => {
                VerticesSource::Marker { len, per_instance }
            }
        }
    }
}
