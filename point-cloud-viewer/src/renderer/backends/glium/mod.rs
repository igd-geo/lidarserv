//! Render backend based on the `glium` crate.
mod animation_helper;
mod draw_grid;
mod draw_point_cloud;
mod dynamic_drawing;
mod event_loop;
mod util;
mod windows;

use crate::renderer::renderer_command::RendererCommand;
use crate::renderer::vertex_data::VertexDataType;
use crate::renderer::viewer::private::{RenderThreadBuilder, RenderThreadHandle};
use glium::glutin::event_loop::EventLoopProxy;

/// Settings for the Glium point cloud renderer backend.
#[derive(Debug, Clone)]
pub struct GliumRenderOptions {
    /// The multisampling level used during rendering.
    /// The value **must** be a power of 2. Any other value will result in a panic during window creation.
    /// A value of `0` indicates, that multisampling is disabled.
    pub multisampling: u16,
}

#[doc(hidden)]
pub struct GliumRenderThreadHandle {
    proxy: EventLoopProxy<RendererCommand>,
}

impl Default for GliumRenderOptions {
    fn default() -> Self {
        GliumRenderOptions { multisampling: 2 }
    }
}

const BACKEND_NAME: &str = "Open GL (glium)";

impl RenderThreadBuilder for GliumRenderOptions {
    type Handle = GliumRenderThreadHandle;

    fn run(&self, handle_sender: crossbeam_channel::Sender<Self::Handle>) {
        let event_loop = event_loop::new();
        handle_sender
            .send(GliumRenderThreadHandle {
                proxy: event_loop.create_proxy(),
            })
            .unwrap();
        event_loop::run(event_loop, self);
    }
}

impl RenderThreadHandle for GliumRenderThreadHandle {
    fn name(&self) -> &'static str {
        BACKEND_NAME
    }

    fn is_vertex_data_type_supported(&self, data_type: VertexDataType) -> bool {
        match data_type {
            VertexDataType::F32 => true,
            VertexDataType::Vec3F32 => true,
            VertexDataType::U8 => true,
            VertexDataType::Vec3F32Transform => true,
        }
    }

    fn execute_command(&self, command: RendererCommand) {
        // This panics, if the event loop is already terminated.
        // However, the interface of crate::renderer::render_thread::RenderThread makes sure,
        // that this never happens, because the terminate() function consumes the object, so that
        // no further commands can be issued.
        self.proxy.send_event(command).unwrap();
    }
}
