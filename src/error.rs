use axum::{
    http::{
        HeaderValue, StatusCode,
        header::{CACHE_CONTROL, REFERRER_POLICY},
    },
    response::{IntoResponse, Response},
};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    message: String,
    details: String,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.details)
    }
}

impl std::error::Error for AppError {}

impl AppError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            details: "bad request".to_string(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            status: StatusCode::NOT_FOUND,
            details: message.clone(),
            message,
        }
    }

    pub fn too_many_requests(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            details: message.clone(),
            message,
        }
    }

    pub fn payload_too_large(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            details: message.clone(),
            message,
        }
    }

    pub fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Internal server error.".to_string(),
            details: error.to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let mut response = (self.status, self.message).into_response();
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
            .headers_mut()
            .insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
        response
    }
}
