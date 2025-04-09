//! The types in this module are the main way of interacting with the renderer.

use self::private::{RenderThreadBuilder, RenderThreadHandle};
use crate::navigation::{Matrices, ViewDirection};
use crate::renderer::error::RendererResult;
pub use crate::renderer::renderer_command::PointCloudId;
use crate::renderer::renderer_command::{FocusTarget, PointAttribute, RendererCommand, WindowId};
use crate::renderer::settings::{AnimationSettings, BaseRenderSettings, PointCloudRenderSettings};
use crate::renderer::vertex_data::point_attribute_to_vertex_data;
use pasture_core::containers::BorrowedBuffer;
use pasture_core::layout::{PointAttributeDefinition, attributes};
use pasture_core::math::AABB;
use std::thread;

pub(crate) mod private {
    //! The traits in this module are only meant to be implemented and used by this crate.
    //! Users of this crate can use [super::RenderThreadBuilderExt] and [super::RenderThread],
    //! that offer a much more convenient and easy-to-use interface to the point cloud renderer.

    use crate::renderer::renderer_command::RendererCommand;
    use crate::renderer::vertex_data::VertexDataType;

    pub trait RenderThreadBuilder {
        type Handle: RenderThreadHandle + 'static;

        fn run(&self, handle_sender: crossbeam_channel::Sender<Self::Handle>);
    }

    pub trait RenderThreadHandle: Send {
        fn name(&self) -> &'static str;

        fn is_vertex_data_type_supported(&self, data_type: VertexDataType) -> bool;

        fn execute_command(&self, command: RendererCommand);
    }
}

/// This trait is implemented by each render backend,
/// allowing to run the render thread with that backend.
pub trait RenderThreadBuilderExt: RenderThreadBuilder {
    /// Runs the render thread.
    ///
    /// Once the renderer is fully initialized, the provided callback will be called with
    /// a [RenderThread] instance, that can be used to control the viewer(s).
    ///
    /// # Limitations
    ///
    /// This will turn the current thread into the render thread. Once started, the
    /// render thread will not terminate, so this method will never return.
    ///
    /// This method needs to be called on the main thread, otherwise it will panic.
    fn run<F>(&self, callback: F)
    where
        F: Send + 'static + FnOnce(RenderThread),
    {
        let (handle_sender, handle_receiver) = crossbeam_channel::bounded::<Self::Handle>(1);

        thread::spawn(move || {
            let handle = handle_receiver.recv().unwrap();
            let handle = Box::new(handle);
            let render_thread = RenderThread { handle };
            callback(render_thread);
        });

        RenderThreadBuilder::run(self, handle_sender);
    }
}

impl<T> RenderThreadBuilderExt for T where T: RenderThreadBuilder {}

/// Handle through which the render can be controlled.
pub struct RenderThread {
    handle: Box<dyn RenderThreadHandle>,
}

impl RenderThread {
    /// Closes all point cloud viewer windows and exits the main thread,
    /// which will terminate the application.
    pub fn terminate(self) { /* Terminating the render thread happens in the Drop impl. */
    }

    /// Opens a new window.
    pub fn open_window(&self) -> RendererResult<Window> {
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);
        let (closed_notify_sender, closed_notify_receiver) = crossbeam_channel::bounded(1);
        self.handle.execute_command(RendererCommand::OpenWindow {
            response_sender,
            closed_notify_sender,
        });
        let window_id = response_receiver.recv().unwrap()?;
        let window = Window {
            renderer: self.handle.as_ref(),
            window_id,
            closed_notify_receiver,
        };
        Ok(window)
    }
}

impl Drop for RenderThread {
    fn drop(&mut self) {
        self.handle.execute_command(RendererCommand::Terminate)
    }
}

/// Handle to a single window.
///
/// Each window has its own [render settings](BaseRenderSettings), defining general things like
/// the background color.
///
/// A window can hold multiple point clouds.
/// Point clouds can be created using any of the methods [Self::add_point_cloud],
/// [Self::add_point_cloud_with_attributes] or [Self::add_point_cloud_with_attributes_and_settings].
/// Each of these methods will return an id, that can be used for referencing this point
/// cloud later. Note, that point clouds are not shared
/// between windows, so using a point cloud ID from one window on another window is illegal.
///
/// A point cloud consists of at least the [3d position](attributes::POSITION_3D) of each point,
/// but can optionally have additional attributes. To use any additional attributes for rendering,
/// they have to be explicitly specified when creating the point cloud, so that the renderer knows,
/// which attributes data it needs to uploaded to the GPU.
///
/// The look of each point cloud is defined by its [render settings](PointCloudRenderSettings).
/// A point cloud can either have its own specialized render settings, or it will fall back to
/// the default render settings of the window.
///
pub struct Window<'a> {
    renderer: &'a dyn RenderThreadHandle,
    window_id: WindowId,
    closed_notify_receiver: crossbeam_channel::Receiver<()>,
}

impl Window<'_> {
    /// Closes the window.
    pub fn close(self) { /* Closing the window happens in the Drop impl. */
    }

    /// Waits for the user to close the window.
    pub fn join(self) {
        self.closed_notify_receiver.recv().unwrap();
    }

    /// Changes the render settings affecting the general look of the viewer
    pub fn set_render_settings(&self, new_settings: BaseRenderSettings) -> RendererResult<()> {
        let (result_sender, result_receiver) = crossbeam_channel::bounded(1);
        self.renderer
            .execute_command(RendererCommand::UpdateSettings {
                window_id: self.window_id,
                new_settings,
                result_sender,
            });
        result_receiver.recv().unwrap()
    }

    /// Changes the default settings for how point clouds should look.
    ///
    /// Every point cloud, that does not have custom render settings
    /// (provided via [Self::add_point_cloud_with_attributes_and_settings],
    /// or [Self::set_point_cloud_settings]) will fall back to these default settings.
    pub fn set_default_point_cloud_settings(
        &self,
        new_settings: PointCloudRenderSettings,
    ) -> RendererResult<()> {
        let (result_sender, result_receiver) = crossbeam_channel::bounded(1);
        self.renderer
            .execute_command(RendererCommand::UpdateDefaultPointCloudSettings {
                window_id: self.window_id,
                new_settings,
                result_sender,
            });
        result_receiver.recv().unwrap()
    }

    fn add_point_cloud_impl(
        &self,
        points: &impl BorrowedBuffer,
        attributes: &[&PointAttributeDefinition],
        settings: Option<PointCloudRenderSettings>,
    ) -> RendererResult<PointCloudId> {
        // position vertex data
        let positions =
            point_attribute_to_vertex_data(points, &attributes::POSITION_3D, self.renderer)?;

        // vertex data for attributes
        let mut attributes_vertex_data = Vec::with_capacity(attributes.len());
        for &attr in attributes {
            attributes_vertex_data.push(PointAttribute {
                attribute: attr.to_owned(),
                data: point_attribute_to_vertex_data(points, attr, self.renderer)?,
            });
        }

        // send to renderer
        let (result_sender, result_receiver) = crossbeam_channel::bounded(1);
        self.renderer
            .execute_command(RendererCommand::AddPointCloud {
                window_id: self.window_id,
                positions,
                attributes: attributes_vertex_data,
                render_settings: settings,
                result_sender,
            });
        result_receiver.recv().unwrap() // will always receive exactly one value. (unless there is a bug in the renderer thread)
    }

    /// Adds the given points to the renderer.
    /// Only the POSITION_3D will be uploaded to the GPU by default.
    /// If you need to visualize additional attributes (color, intensity, ...), use [Window::add_point_cloud_with_attributes] instead.
    pub fn add_point_cloud(&self, points: &impl BorrowedBuffer) -> RendererResult<PointCloudId> {
        self.add_point_cloud_impl(points, &[], None)
    }

    /// Adds the given points to the renderer.
    /// This will transfer the position (required) as well as any attribute given as the second parameter to the GPU.
    ///
    /// The settings that have been set using [Window::set_default_point_cloud_settings] will be used for rendering this point cloud.
    /// You can use [Window::add_point_cloud_with_attributes_and_settings] to create a point cloud with customized render settings.
    pub fn add_point_cloud_with_attributes(
        &self,
        points: &impl BorrowedBuffer,
        attributes: &[&PointAttributeDefinition],
    ) -> RendererResult<PointCloudId> {
        self.add_point_cloud_impl(points, attributes, None)
    }

    /// Adds the given points to the renderer, with custom render settings.
    pub fn add_point_cloud_with_attributes_and_settings(
        &self,
        points: &impl BorrowedBuffer,
        attributes: &[&PointAttributeDefinition],
        settings: PointCloudRenderSettings,
    ) -> RendererResult<PointCloudId> {
        self.add_point_cloud_impl(points, attributes, Some(settings))
    }

    fn set_point_cloud_settings_impl(
        &self,
        point_cloud_id: PointCloudId,
        new_settings: Option<PointCloudRenderSettings>,
    ) -> RendererResult<()> {
        let (result_sender, result_receiver) = crossbeam_channel::bounded(1);
        self.renderer
            .execute_command(RendererCommand::UpdatePointCloudSettings {
                window_id: self.window_id,
                point_cloud_id,
                result_sender,
                new_settings,
            });
        result_receiver.recv().unwrap()
    }

    /// Sets the render settings for a specific point cloud.
    /// This will overwrite the default settings for that point cloud.
    pub fn set_point_cloud_settings(
        &self,
        id: PointCloudId,
        new_settings: PointCloudRenderSettings,
    ) -> RendererResult<()> {
        self.set_point_cloud_settings_impl(id, Some(new_settings))
    }

    /// Resets the settings of that point cloud to the default settings, after overriding them with [Self::set_point_cloud_settings].
    pub fn reset_point_cloud_settings(&self, id: PointCloudId) -> RendererResult<()> {
        self.set_point_cloud_settings_impl(id, None)
    }

    /// Replaces the points of an existing point cloud with new point data.
    pub fn update_point_cloud(
        &self,
        id: PointCloudId,
        points: &impl BorrowedBuffer,
        attributes: &[&PointAttributeDefinition],
    ) -> RendererResult<()> {
        // position vertex data
        let positions =
            point_attribute_to_vertex_data(points, &attributes::POSITION_3D, self.renderer)?;

        // vertex data for attributes
        let mut attributes_vertex_data = Vec::with_capacity(attributes.len());
        for &attr in attributes {
            attributes_vertex_data.push(PointAttribute {
                attribute: attr.to_owned(),
                data: point_attribute_to_vertex_data(points, attr, self.renderer)?,
            });
        }

        // send to renderer
        let (result_sender, result_receiver) = crossbeam_channel::bounded(1);
        self.renderer
            .execute_command(RendererCommand::UpdatePoints {
                window_id: self.window_id,
                positions,
                attributes: attributes_vertex_data,
                result_sender,
                point_cloud_id: id,
            });
        result_receiver.recv().unwrap()
    }

    /// Removes the point cloud with the given id.
    pub fn remove_point_cloud(&self, id: PointCloudId) -> RendererResult<()> {
        let (result_sender, result_receiver) = crossbeam_channel::bounded(1);
        self.renderer
            .execute_command(RendererCommand::RemovePointCloud {
                window_id: self.window_id,
                point_cloud_id: id,
                result_sender,
            });
        result_receiver.recv().unwrap()
    }

    /// Returns a builder, that can be used to initiate a camera movement.
    pub fn camera_movement(&self) -> CameraMovementBuilder {
        CameraMovementBuilder {
            window: self,
            focus: None,
            view: None,
            animation: None,
        }
    }

    /// Moves the camera, such that all point clouds are fully visible.
    /// This is just a shorthand method for the more flexible [Self::camera_movement].
    pub fn focus_on_all(&self) -> RendererResult<()> {
        self.camera_movement().focus_on_all().execute()
    }

    /// Returns a mpsc::Receiver,
    /// where the camera matrix will always be sent to, whenever the user moves the camera.
    pub fn subscribe_to_camera(&self) -> RendererResult<crossbeam_channel::Receiver<Matrices>> {
        let (result_sender, result_receiver) = crossbeam_channel::bounded(1);
        self.renderer
            .execute_command(RendererCommand::AddCameraSubscriber {
                window_id: self.window_id,
                result_sender,
            });
        result_receiver.recv().unwrap()
    }
}

impl Drop for Window<'_> {
    fn drop(&mut self) {
        self.renderer.execute_command(RendererCommand::CloseWindow {
            window_id: self.window_id,
        });
    }
}

/// Builder that is used to define and execute a camera movement.
#[must_use]
pub struct CameraMovementBuilder<'a> {
    window: &'a Window<'a>,
    focus: Option<FocusTarget>,
    view: Option<ViewDirection>,
    animation: Option<AnimationSettings>,
}

impl CameraMovementBuilder<'_> {
    /// Positions the camera, such that all point clouds are visible.
    pub fn focus_on_all(self) -> Self {
        CameraMovementBuilder {
            focus: Some(FocusTarget::All),
            ..self
        }
    }

    /// Positions the camera, such that the contents of the given bounding
    /// box are all visible on screen.
    pub fn focus_on_bounding_box(self, aabb: AABB<f64>) -> Self {
        CameraMovementBuilder {
            focus: Some(FocusTarget::BoundingBox(aabb)),
            ..self
        }
    }

    /// Positions the camera, such that the given point cloud is fully visible on screen.
    pub fn focus_on_point_cloud(self, id: PointCloudId) -> Self {
        CameraMovementBuilder {
            focus: Some(FocusTarget::PointCloud(id)),
            ..self
        }
    }

    /// The focused area will be viewed from the top, with the camera looking down.
    pub fn view_top(self) -> Self {
        CameraMovementBuilder {
            view: Some(ViewDirection::Top),
            ..self
        }
    }

    /// The focused area will be viewed from the front, with the camera looking
    /// into the positive y direction.
    pub fn view_front(self) -> Self {
        CameraMovementBuilder {
            view: Some(ViewDirection::Front),
            ..self
        }
    }

    /// The focused area will be viewed from the left, with the camera looking
    /// into the positive x direction.
    pub fn view_left(self) -> Self {
        CameraMovementBuilder {
            view: Some(ViewDirection::Left),
            ..self
        }
    }

    /// The focused area will be viewed from the right, with the camera looking
    /// into the negative x direction.
    pub fn view_right(self) -> Self {
        CameraMovementBuilder {
            view: Some(ViewDirection::Right),
            ..self
        }
    }

    /// The focused area will be viewed from the back, with the camera looking
    /// into the negative y direction.
    pub fn view_back(self) -> Self {
        CameraMovementBuilder {
            view: Some(ViewDirection::Back),
            ..self
        }
    }

    /// The focused area will be viewed at an angle, from the top left.
    pub fn view_topleft(self) -> Self {
        CameraMovementBuilder {
            view: Some(ViewDirection::TopLeft),
            ..self
        }
    }

    /// The focused area will be viewed at an angle, from the top front.
    pub fn view_topfront(self) -> Self {
        CameraMovementBuilder {
            view: Some(ViewDirection::TopFront),
            ..self
        }
    }

    /// The focused area will be viewed at an angle, from the top right.
    pub fn view_topright(self) -> Self {
        CameraMovementBuilder {
            view: Some(ViewDirection::TopRight),
            ..self
        }
    }

    /// The focused area will be viewed at an angle, from the top back.
    pub fn view_topback(self) -> Self {
        CameraMovementBuilder {
            view: Some(ViewDirection::TopBack),
            ..self
        }
    }

    /// The camera movement will be animated.
    pub fn animated(self, anim: AnimationSettings) -> Self {
        CameraMovementBuilder {
            animation: Some(anim),
            ..self
        }
    }

    /// Executes the camera movement.
    /// Animations do not block - the method will return immediately and the animation will
    /// continue "in the background".
    pub fn execute(self) -> RendererResult<()> {
        let (result_sender, result_receiver) = crossbeam_channel::bounded(1);
        self.window
            .renderer
            .execute_command(RendererCommand::CameraMovement {
                window_id: self.window.window_id,
                focus: self.focus,
                view: self.view,
                animation: self.animation,
                result_sender,
            });
        result_receiver.recv().unwrap()
    }
}
