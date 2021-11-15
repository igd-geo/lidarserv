use crate::navigation::{Matrices, ViewDirection};
use crate::renderer::error::RendererResult;
use crate::renderer::settings::{AnimationSettings, BaseRenderSettings, PointCloudRenderSettings};
use crate::renderer::vertex_data::VertexData;
use pasture_core::layout::PointAttributeDefinition;
use pasture_core::math::AABB;

#[derive(Clone, Debug)]
pub enum RendererCommand {
    Terminate,
    OpenWindow {
        closed_notify_sender: crossbeam_channel::Sender<()>,
        response_sender: crossbeam_channel::Sender<RendererResult<WindowId>>,
    },
    CloseWindow {
        window_id: WindowId,
    },
    UpdateSettings {
        window_id: WindowId,
        new_settings: BaseRenderSettings,
        result_sender: crossbeam_channel::Sender<RendererResult<()>>,
    },
    UpdateDefaultPointCloudSettings {
        window_id: WindowId,
        new_settings: PointCloudRenderSettings,
        result_sender: crossbeam_channel::Sender<RendererResult<()>>,
    },
    UpdatePointCloudSettings {
        window_id: WindowId,
        point_cloud_id: PointCloudId,
        new_settings: Option<PointCloudRenderSettings>,
        result_sender: crossbeam_channel::Sender<RendererResult<()>>,
    },
    AddPointCloud {
        window_id: WindowId,
        positions: VertexData,
        attributes: Vec<PointAttribute>,
        render_settings: Option<PointCloudRenderSettings>,
        result_sender: crossbeam_channel::Sender<RendererResult<PointCloudId>>,
    },
    RemovePointCloud {
        window_id: WindowId,
        point_cloud_id: PointCloudId,
        result_sender: crossbeam_channel::Sender<RendererResult<()>>,
    },
    UpdatePoints {
        window_id: WindowId,
        point_cloud_id: PointCloudId,
        positions: VertexData,
        attributes: Vec<PointAttribute>,
        result_sender: crossbeam_channel::Sender<RendererResult<()>>,
    },
    CameraMovement {
        window_id: WindowId,
        focus: Option<FocusTarget>,
        view: Option<ViewDirection>,
        animation: Option<AnimationSettings>,
        result_sender: crossbeam_channel::Sender<RendererResult<()>>,
    },
    AddCameraSubscriber {
        window_id: WindowId,
        result_sender:
            crossbeam_channel::Sender<RendererResult<crossbeam_channel::Receiver<Matrices>>>,
    },
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PointCloudId(usize);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct WindowId(usize);

#[derive(Copy, Clone, Debug)]
pub enum FocusTarget {
    All,
    BoundingBox(AABB<f64>),
    PointCloud(PointCloudId),
}

impl PointCloudId {
    pub fn begin() -> Self {
        PointCloudId(0)
    }

    pub fn increment(&mut self) -> Self {
        self.0 += 1;
        *self
    }
}

impl WindowId {
    pub fn begin() -> Self {
        WindowId(0)
    }

    pub fn next(&mut self) -> Self {
        self.0 += 1;
        *self
    }
}

#[derive(Clone, Debug)]
pub struct PointAttribute {
    pub attribute: PointAttributeDefinition,
    pub data: VertexData,
}
