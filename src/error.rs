// region:    --- Imports
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::error;

// endregion: --- Imports

// region:    --- Error Types

/// API 에러 응답 형식
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// 애플리케이션 에러 타입
#[derive(Debug)]
pub enum AppError {
    // 데이터베이스 에러
    Database(sqlx::Error),

    // 비즈니스 로직 에러
    NotFound(String),
    InvalidState(String),
    Validation(String, Option<serde_json::Value>),
    Conflict(String),

    // 인프라 에러
    Kafka(String),

    // 기타 에러
    Internal(String),
}

// endregion: --- Error Types

// region:    --- Error Implementations

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Database(e) => write!(f, "Database error: {}", e),
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
            AppError::InvalidState(msg) => write!(f, "Invalid state: {}", msg),
            AppError::Validation(msg, _) => write!(f, "Validation error: {}", msg),
            AppError::Conflict(msg) => write!(f, "Conflict: {}", msg),
            AppError::Kafka(msg) => write!(f, "Kafka error: {}", msg),
            AppError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl AppError {
    /// 에러 메시지에 특정 문자열이 포함되어 있는지 확인
    pub fn contains(&self, s: &str) -> bool {
        match self {
            AppError::Database(e) => e.to_string().contains(s),
            AppError::NotFound(msg) => msg.contains(s),
            AppError::InvalidState(msg) => msg.contains(s),
            AppError::Validation(msg, _) => msg.contains(s),
            AppError::Conflict(msg) => msg.contains(s),
            AppError::Kafka(msg) => msg.contains(s),
            AppError::Internal(msg) => msg.contains(s),
        }
    }
}

impl std::error::Error for AppError {}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::RowNotFound => AppError::NotFound("Resource not found".to_string()),
            _ => {
                error!("Database error: {:?}", error);
                AppError::Database(error)
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_response) = match &self {
            AppError::Database(e) => {
                error!("Database error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorResponse {
                        code: "DATABASE_ERROR".to_string(),
                        message: "데이터베이스 오류가 발생했습니다".to_string(),
                        details: None,
                    },
                )
            }
            AppError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                ErrorResponse {
                    code: "NOT_FOUND".to_string(),
                    message: msg.clone(),
                    details: None,
                },
            ),
            AppError::InvalidState(msg) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse {
                    code: "INVALID_STATE".to_string(),
                    message: msg.clone(),
                    details: None,
                },
            ),
            AppError::Validation(msg, details) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse {
                    code: "VALIDATION_ERROR".to_string(),
                    message: msg.clone(),
                    details: details.clone(),
                },
            ),
            AppError::Conflict(msg) => (
                StatusCode::CONFLICT,
                ErrorResponse {
                    code: "CONFLICT".to_string(),
                    message: msg.clone(),
                    details: None,
                },
            ),
            AppError::Kafka(msg) => {
                error!("Kafka error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorResponse {
                        code: "KAFKA_ERROR".to_string(),
                        message: "메시지 처리 중 오류가 발생했습니다".to_string(),
                        details: None,
                    },
                )
            }
            AppError::Internal(msg) => {
                error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorResponse {
                        code: "INTERNAL_ERROR".to_string(),
                        message: "내부 서버 오류가 발생했습니다".to_string(),
                        details: None,
                    },
                )
            }
        };

        (status, Json(error_response)).into_response()
    }
}

// 비즈니스 로직 에러 변환 함수들
impl AppError {
    pub fn auction_not_started() -> Self {
        AppError::InvalidState("경매가 아직 시작되지 않았습니다".to_string())
    }

    pub fn auction_already_ended() -> Self {
        AppError::InvalidState("경매가 이미 종료되었습니다".to_string())
    }

    pub fn bid_too_low(current_price: i64, bid_amount: i64) -> Self {
        AppError::Validation(
            "입찰 금액이 현재 가격보다 낮습니다".to_string(),
            Some(serde_json::json!({
                "current_price": current_price,
                "bid_amount": bid_amount
            })),
        )
    }

    pub fn max_retries_exceeded() -> Self {
        AppError::Conflict("최대 재시도 횟수 초과".to_string())
    }
}

// endregion: --- Error Implementations

// region:    --- Result Type

/// 애플리케이션 결과 타입
pub type Result<T> = std::result::Result<T, AppError>;

// endregion: --- Result Type
