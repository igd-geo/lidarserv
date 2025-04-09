use crate::navigation::event::{MouseButton, MouseDragSettings};
use crate::navigation::map_navigation::MapNavigation;
use crate::navigation::{Matrices, Navigation, ViewDirection};
use crate::renderer::backends::glium::animation_helper::{
    AnimationHelper, PolynomialEasingFunction,
};
use crate::renderer::backends::glium::draw_grid::GridRenderer;
use crate::renderer::backends::glium::draw_point_cloud::PointCloudsRenderer;
use crate::renderer::error::{RendererError, RendererResult};
use crate::renderer::renderer_command::{
    FocusTarget, PointAttribute, PointCloudId, RendererCommand, WindowId,
};
use crate::renderer::settings::{
    AnimationEasing, AnimationSettings, BaseRenderSettings, PointCloudRenderSettings,
};
use crate::renderer::vertex_data::VertexData;
use glium::glutin::dpi::{LogicalSize, PhysicalPosition};
use glium::glutin::event::{
    DeviceId, ElementState, ModifiersState, MouseButton as WinitMouseButton, MouseScrollDelta,
    WindowEvent,
};
use glium::glutin::event_loop::EventLoopWindowTarget;
use glium::glutin::window::WindowId as OsWindowId;
use glium::{Display, Surface, glutin};
use log::debug;
use pasture_core::math::AABB;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Copy, Clone, PartialEq)]
enum FocusMode {
    Free,
    BoundingBox(AABB<f64>),
}

pub struct Window {
    display: Display,
    closed_notify_sender: crossbeam_channel::Sender<()>,
    render_settings: BaseRenderSettings,
    grid: Option<GridRenderer>,
    point_clouds: PointCloudsRenderer,
    current_scale_factor: f64,
    modifiers: ModifiersState,
    cursors: HashMap<DeviceId, PhysicalPosition<f64>>,
    current_drag: Option<(DeviceId, WinitMouseButton, ModifiersState)>,
    nav_controller: Box<dyn Navigation>,
    focus: FocusMode,
    last_time_step: Instant,
    camera_animation: AnimationHelper,
    camera_subscribers: Vec<crossbeam_channel::Sender<Matrices>>,
}

pub struct WindowManager {
    window_id_counter: WindowId,
    windows_by_id: HashMap<WindowId, Window>,
}

impl WindowManager {
    pub fn new() -> Self {
        WindowManager {
            window_id_counter: WindowId::begin(),
            windows_by_id: HashMap::new(),
        }
    }

    pub fn open_window(
        &mut self,
        event_loop: &EventLoopWindowTarget<RendererCommand>,
        closed_notify_sender: crossbeam_channel::Sender<()>,
        multisampling: u16,
    ) -> RendererResult<WindowId> {
        // Create window
        let wb = glutin::window::WindowBuilder::new()
            .with_title("Point Cloud Viewer")
            .with_inner_size(LogicalSize::new(500, 500));
        let gl_window = glutin::ContextBuilder::new()
            .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (4, 0)))
            .with_gl_profile(glutin::GlProfile::Core)
            .with_multisampling(multisampling)
            .with_depth_buffer(24)
            .with_vsync(true)
            .build_windowed(wb, event_loop)
            .map_err(|e| RendererError::Graphics {
                source: Box::new(e),
            })?;
        let display = Display::from_gl_window(gl_window).map_err(|e| RendererError::Graphics {
            source: Box::new(e),
        })?;
        let render_settings = BaseRenderSettings::default();
        let grid = render_settings
            .grid
            .as_ref()
            .map(|grid_settings| GridRenderer::new(&display, grid_settings.to_owned()).unwrap());
        let point_clouds = PointCloudsRenderer::new();
        let modifiers = ModifiersState::default();
        let cursors = HashMap::new();
        let current_drag = None;
        let nav_controller: Box<dyn Navigation> = Box::new(MapNavigation::new());
        let current_scale_factor = display.gl_window().window().scale_factor();
        let current_size = display.gl_window().window().inner_size();

        let mut window = Window {
            display,
            closed_notify_sender,
            render_settings,
            grid,
            point_clouds,
            current_scale_factor,
            modifiers,
            cursors,
            current_drag,
            nav_controller,
            focus: FocusMode::Free,
            last_time_step: Instant::now(),
            camera_animation: AnimationHelper::new(),
            camera_subscribers: vec![],
        };

        // initialize size and scale factor
        let mut tmp = current_size;
        window.process_window_event(WindowEvent::ScaleFactorChanged {
            scale_factor: current_scale_factor,
            new_inner_size: &mut tmp,
        });
        window.process_window_event(WindowEvent::Resized(current_size));

        // log
        debug!(
            "Window opened: {:?}",
            window.display.gl_window().window().id()
        );
        debug!(
            "OpenGL version: {}",
            window.display.get_opengl_version_string()
        );
        debug!(
            "OpenGL vendor: {}",
            window.display.get_opengl_vendor_string()
        );
        debug!(
            "OpenGL renderer: {}",
            window.display.get_opengl_renderer_string()
        );
        debug!("OpenGL profile: {:?}", window.display.get_opengl_profile());

        // insert window
        let id = self.window_id_counter.next();
        self.windows_by_id.insert(id, window);
        Ok(id)
    }

    pub fn window_by_id_mut(&mut self, id: WindowId) -> RendererResult<&mut Window> {
        match self.windows_by_id.get_mut(&id) {
            None => Err(RendererError::WindowClosed { id }),
            Some(win) => Ok(win),
        }
    }

    pub fn window_by_os_id_mut(&mut self, os_window_id: OsWindowId) -> Option<&mut Window> {
        self.windows_by_id
            .values_mut()
            .find(|it| it.display.gl_window().window().id() == os_window_id)
    }

    pub fn close(&mut self, id: WindowId) {
        self.windows_by_id.remove(&id);
    }

    pub fn close_os(&mut self, id: OsWindowId) {
        let item = self
            .windows_by_id
            .iter()
            .find(|(_, v)| v.display.gl_window().window().id() == id);
        if let Some((&k, _)) = item {
            self.close(k.to_owned());
        }
    }
}

impl Window {
    pub fn request_redraw(&self) {
        self.display.gl_window().window().request_redraw()
    }

    fn time_step(&mut self) {
        let now = Instant::now();
        let delta_t = now.duration_since(self.last_time_step);
        self.last_time_step = now;
        self.camera_animation.update(delta_t);
    }

    pub fn process_window_event(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers;
            }
            WindowEvent::CursorMoved {
                position,
                device_id,
                ..
            } => {
                let previous = self.cursors.insert(device_id, position);
                if let Some(previous_position) = previous {
                    if let Some((drag_device, drag_button, drag_modifiers)) = self.current_drag {
                        if device_id == drag_device {
                            self.nav_controller.on_drag(
                                previous_position.x / self.current_scale_factor,
                                previous_position.y / self.current_scale_factor,
                                position.x / self.current_scale_factor,
                                position.y / self.current_scale_factor,
                                MouseDragSettings {
                                    button: match drag_button {
                                        WinitMouseButton::Left => MouseButton::Left,
                                        WinitMouseButton::Middle => MouseButton::Middle,
                                        WinitMouseButton::Right => MouseButton::Right,
                                        WinitMouseButton::Other(_) => MouseButton::Other,
                                    },
                                    shift_pressed: drag_modifiers.shift(),
                                    ctrl_pressed: drag_modifiers.ctrl(),
                                    alt_pressed: drag_modifiers.alt(),
                                },
                            );
                            self.free_focus();
                            self.request_redraw();
                        }
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.current_scale_factor = scale_factor;
            }
            WindowEvent::Resized(size) => {
                self.nav_controller.on_window_resized(
                    size.width as f64 / self.current_scale_factor,
                    size.height as f64 / self.current_scale_factor,
                );
                self.update_animation_target();
                self.request_redraw();
            }
            WindowEvent::MouseInput {
                state,
                button,
                device_id,
                ..
            } => match state {
                ElementState::Pressed => {
                    if self.current_drag.is_none() {
                        self.current_drag = Some((device_id, button, self.modifiers));
                        self.free_focus();
                    }
                }
                ElementState::Released => {
                    if let Some((drag_device, drag_button, ..)) = self.current_drag {
                        if device_id == drag_device && button == drag_button {
                            self.current_drag = None;
                            self.free_focus();
                        }
                    }
                }
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll_amount = match delta {
                    MouseScrollDelta::LineDelta(_, delta_y) => {
                        // assume a line to be equivalent to 20 logical pixels
                        delta_y as f64 * 20.0
                    }
                    MouseScrollDelta::PixelDelta(delta) => delta.y / self.current_scale_factor,
                };
                self.nav_controller.on_scroll(scroll_amount);
                self.free_focus();
                self.request_redraw();
            }
            _ => (),
        }
    }

    fn broadcast_camera_matrix(&mut self, matrices: &Matrices) {
        let mut it = 0;
        while it < self.camera_subscribers.len() {
            let subscriber = &self.camera_subscribers[it];
            match subscriber.send(matrices.clone()) {
                Ok(_) => it += 1,
                Err(_) => {
                    // a send operation can only fail, if the receiving end of the channel is
                    // disconnected.
                    self.camera_subscribers.swap_remove(it);
                }
            }
        }
    }

    pub fn draw(&mut self) -> RendererResult<()> {
        // elapsed time since last draw
        self.time_step();

        // get the camera matrices
        let matrices = match self.camera_animation.get_animated_value() {
            None => {
                let m = self.nav_controller.update();
                self.broadcast_camera_matrix(&m);
                m
            }
            Some(animated_matrices) => {
                self.request_redraw();
                animated_matrices
            }
        };
        let view_projection = matrices.projection_matrix * matrices.view_matrix;

        // draw background
        let mut frame = self.display.draw();
        frame.clear_color_and_depth(
            (
                self.render_settings.bg_color.r,
                self.render_settings.bg_color.g,
                self.render_settings.bg_color.b,
                1.0,
            ),
            1.0,
        );

        // draw point clouds
        if self.render_settings.enable_edl {
            self.point_clouds
                .draw_with_edl(
                    &self.display,
                    &mut frame,
                    self.current_scale_factor,
                    &view_projection,
                    &matrices.projection_matrix_inv,
                )
                .unwrap();
        } else {
            self.point_clouds
                .draw(&mut frame, self.current_scale_factor, &view_projection)
                .unwrap();
        }

        // draw grid
        if let Some(g) = &self.grid {
            g.draw(
                &mut frame,
                &view_projection,
                &matrices.view_matrix_inv,
                self.current_scale_factor,
            )
            .unwrap()
        }
        frame.finish().map_err(|e| RendererError::Graphics {
            source: Box::new(e),
        })
    }

    pub fn set_title(&mut self, title: &str) {
        self.display.gl_window().window().set_title(title);
    }

    pub fn update_settings(&mut self, new_settings: BaseRenderSettings) -> RendererResult<()> {
        // grid
        let result = if let Some(grid_settings) = &new_settings.grid {
            let new_grid_result = match self.grid.take() {
                None => GridRenderer::new(&self.display, grid_settings.to_owned()),
                Some(mut g) => {
                    let update_result = g.update_settings(&self.display, grid_settings.to_owned());
                    update_result.map(move |_| g)
                }
            };

            new_grid_result.map(|g| {
                self.grid = Some(g);
            })
        } else {
            self.grid = None;
            Ok(())
        };

        // settings
        if result.is_ok() {
            self.set_title(&new_settings.window_title);
            self.render_settings = new_settings;
        }

        // redraw
        self.request_redraw();

        result
    }

    pub fn update_default_point_cloud_render_settings(
        &mut self,
        new_settings: PointCloudRenderSettings,
    ) -> RendererResult<()> {
        let result = self
            .point_clouds
            .update_default_settings(&self.display, &new_settings);
        self.request_redraw();
        result
    }

    pub fn update_point_cloud_render_settings(
        &mut self,
        point_cloud_id: PointCloudId,
        new_settings: Option<PointCloudRenderSettings>,
    ) -> RendererResult<()> {
        let result = self
            .point_clouds
            .update_settings(&self.display, point_cloud_id, new_settings);
        self.request_redraw();
        result
    }

    pub fn add_point_cloud(
        &mut self,
        positions: &VertexData,
        attributes: &[PointAttribute],
        render_settings: &Option<PointCloudRenderSettings>,
    ) -> RendererResult<PointCloudId> {
        let result = self
            .point_clouds
            .add(&self.display, positions, attributes, render_settings);
        self.request_redraw();
        result
    }

    pub fn update_points(
        &mut self,
        id: PointCloudId,
        positions: &VertexData,
        attributes: &[PointAttribute],
    ) -> RendererResult<()> {
        let result = self
            .point_clouds
            .update_points(&self.display, id, positions, attributes);
        self.request_redraw();
        result
    }

    pub fn remove_point_cloud(&mut self, id: PointCloudId) -> RendererResult<()> {
        let result = self.point_clouds.remove(id);
        self.request_redraw();
        result
    }

    fn free_focus(&mut self) {
        self.camera_animation.abort();
        self.focus = FocusMode::Free;
        self.request_redraw();
    }

    fn update_animation_target(&mut self) {
        if let FocusMode::BoundingBox(aabb) = self.focus {
            self.nav_controller.focus_on(aabb);
            let to = self.nav_controller.update();
            self.broadcast_camera_matrix(&to);
            self.camera_animation.update_animation_target(to);
        }
    }

    pub fn move_camera(
        &mut self,
        focus: Option<FocusTarget>,
        view: Option<ViewDirection>,
        animation: Option<AnimationSettings>,
    ) -> RendererResult<()> {
        // backup old matrices - for the animation
        let from = self.nav_controller.update();

        // calculate aabb to focus on
        let new_focus_aabb = match focus {
            None => None,
            Some(FocusTarget::All) => self.point_clouds.bounding_box(),
            Some(FocusTarget::BoundingBox(aabb)) => Some(aabb),
            Some(FocusTarget::PointCloud(id)) => self.point_clouds.point_cloud_bounding_box(id)?,
        };

        // change the orientation
        if let Some(view) = view {
            self.nav_controller.view_direction(view);
        }

        // focus on the new aabb
        if let Some(aabb) = new_focus_aabb {
            self.nav_controller.focus_on(aabb);
        }

        // new matrices - for the animation
        let to = self.nav_controller.update();
        self.broadcast_camera_matrix(&to);

        // finish the current time step so the animation timing is correct
        self.time_step();

        // start the animation
        if let Some(anim) = animation {
            let easing = match anim.easing {
                AnimationEasing::Linear => PolynomialEasingFunction::linear(),
                AnimationEasing::EaseIn => PolynomialEasingFunction::ease_in(),
                AnimationEasing::EaseOut => PolynomialEasingFunction::ease_out(),
                AnimationEasing::EaseInOut => PolynomialEasingFunction::ease_in_out(),
            };
            self.camera_animation.start(from, to, anim.duration, easing);
        }

        // update
        self.focus = new_focus_aabb.map_or(FocusMode::Free, FocusMode::BoundingBox);
        self.request_redraw();
        Ok(())
    }

    pub fn add_camera_subscriber(&mut self) -> crossbeam_channel::Receiver<Matrices> {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let initial = self.nav_controller.update();
        sender.send(initial).unwrap(); // unwrap: can only fail, if receiver was dropped - but we still hold the receiver, so this will always succeed.
        self.camera_subscribers.push(sender);
        receiver
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.closed_notify_sender.send(()).ok();
        debug!(
            "Window closed: {:?}",
            self.display.gl_window().window().id()
        );
    }
}
