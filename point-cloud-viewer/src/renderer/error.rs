//! Error types returned by the renderer/viewer.

use crate::renderer::renderer_command::{PointCloudId, WindowId};
use pasture_core::layout::PointAttributeDefinition;
use std::error::Error;
use std::fmt;
use std::fmt::{Display, Formatter};

/// Shorthand for a [Result] with a [RendererError].
pub type RendererResult<T> = Result<T, RendererError>;

/// Error type returned by the renderer.
#[derive(Debug)]
pub enum RendererError {
    /// The window was closed by the user.
    WindowClosed { id: WindowId },

    /// Some GPU operation was unsuccessful (e.g. because of not sufficient graphics memory)
    Graphics { source: Box<dyn Error + Send> },

    /// Operation is not supported by the selected backend on the current platform.
    UnsupportedOperation {
        /// Name of the backend that is in use.
        backend_name: &'static str,

        /// Operation that was attempted.
        operation_name: String,

        /// True, if this is not available on this specific platform (e.g. because some GL extension is missing etc...).
        /// False, if the operation is not supported by the backend in general.
        platform_specific: bool,
    },

    /// The point cloud, that was referred to, does not exist.
    PointCloudDoesNotExist {
        /// The id of the point cloud, that we tried to access.
        id: PointCloudId,
    },

    /// Incompatible point attributes in some way.
    ///   - The attribute, that was referred to, does not exist in the given point cloud.
    ///   - Missing a required attribute
    ///   - Attribute has the wrong data type
    ///   - ...
    AttributeMismatch {
        attribute: PointAttributeDefinition,
        problem: AttributeMismatchType,
    },
}

impl Display for RendererError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            RendererError::WindowClosed { .. } => write!(f, "The window is closed."),

            RendererError::Graphics { source } => write!(f, "Gpu error: {}", source),

            RendererError::UnsupportedOperation {
                backend_name,
                operation_name,
                platform_specific: true,
            } => write!(
                f,
                "The graphics backend `{}` does not support `{}` on this platform.",
                *backend_name, *operation_name
            ),

            RendererError::UnsupportedOperation {
                backend_name,
                operation_name,
                platform_specific: false,
            } => write!(
                f,
                "The graphics backend `{}` does not support `{}`.",
                *backend_name, *operation_name
            ),

            RendererError::PointCloudDoesNotExist { .. } => {
                write!(f, "The point cloud does not exist.")
            }

            RendererError::AttributeMismatch { attribute, problem } => match problem {
                AttributeMismatchType::DoesNotExist => write!(
                    f,
                    "The attribute  {} is not present in the point cloud.",
                    attribute
                ),
                AttributeMismatchType::WrongType => {
                    write!(f, "The attribute  {} is of the wrong type.", attribute)
                }
            },
        }
    }
}

impl Error for RendererError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RendererError::Graphics { source } => Some(&**source),
            RendererError::UnsupportedOperation { .. } => None,
            RendererError::PointCloudDoesNotExist { .. } => None,
            RendererError::AttributeMismatch { .. } => None,
            RendererError::WindowClosed { .. } => None,
        }
    }
}

/// Details the reason for an [RendererError::AttributeMismatch] error.
#[derive(Debug)]
pub enum AttributeMismatchType {
    /// The caller referred to an attribute, that does not exist.
    DoesNotExist,

    /// The referred-to attribute is of the wrong or an invalid type.
    WrongType,
}
