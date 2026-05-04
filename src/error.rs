// Gestion centralisée des erreurs HTTP du backend.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;
use validator::ValidationErrors;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub status: u16,
    pub error: String,
    pub message: String,
}

// Corps de réponse pour les conflits de participants.
#[derive(Debug, Serialize)]
pub struct ConflictBody {
    pub status: u16,
    pub error: String,
    pub message: String,
    pub conflicts: Vec<ParticipantConflictItem>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ParticipantConflictItem {
    pub email: String,
    pub display_name: Option<String>,
    pub conflicting_meeting_title: String,
    pub conflicting_start: String,
    pub conflicting_end: String,
}

// Erreur applicative principale.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    // Conflit de participants avec détail (409 + liste des conflits).
    #[error("Participant schedule conflict")]
    ConflictParticipants(Vec<ParticipantConflictItem>),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Upstream error: {0}")]
    Upstream(String),

    #[error("Database error: {0}")]
    Database(sqlx::Error),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::ConflictParticipants(conflicts) => {
                let body = ConflictBody {
                    status: 409,
                    error: "Conflict".to_string(),
                    message: "Certains participants ont déjà un meeting sur ce créneau.".to_string(),
                    conflicts,
                };
                (StatusCode::CONFLICT, Json(body)).into_response()
            }

            other => {
                let status = match &other {
                    AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
                    AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
                    AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
                    AppError::Unauthorized => StatusCode::UNAUTHORIZED,
                    AppError::Forbidden(_) => StatusCode::FORBIDDEN,
                    AppError::NotFound(_) => StatusCode::NOT_FOUND,
                    AppError::Conflict(_) => StatusCode::CONFLICT,
                    AppError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
                    AppError::Upstream(_) => StatusCode::BAD_GATEWAY,
                    AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
                    AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
                    AppError::ConflictParticipants(_) => unreachable!(),
                };

                let message = match &other {
                    AppError::Database(err) => {
                        eprintln!("Database error: {:?}", err);
                        "Une erreur interne est survenue.".to_string()
                    }
                    _ => other.to_string(),
                };

                let body = ErrorBody {
                    status: status.as_u16(),
                    error: status
                        .canonical_reason()
                        .unwrap_or("Unknown error")
                        .to_string(),
                    message,
                };

                (status, Json(body)).into_response()
            }
        }
    }
}

impl From<ValidationErrors> for AppError {
    fn from(value: ValidationErrors) -> Self {
        Self::Validation(value.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self {
        Self::Upstream(value.to_string())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}