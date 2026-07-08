//! Error and response envelope shared by Tauri commands.

pub use app_models::AppError;
use serde::Serialize;

/// API response envelope returned by Tauri commands.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    /// Construct a successful response.
    pub const fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Construct an error response from a message.
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

impl From<AppError> for ApiResponse<String> {
    fn from(err: AppError) -> Self {
        Self::err(err.to_string())
    }
}
