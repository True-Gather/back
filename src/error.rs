// Gestion centralisée des erreurs HTTP du backend.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;
use validator::ValidationErrors;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Serialize, Clone)]
pub struct ErrorBody {
    pub status: u16,
    pub error: String,
    pub message: String,
}

// Élément de conflit de participant (pour ConflictParticipants).
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
    // Erreur de configuration.
    #[error("Configuration error: {0}")]
    Config(String),

    // Erreur de validation des entrées.
    #[error("Validation error: {0}")]
    Validation(String),

    // Erreur de requête invalide.
    #[error("Bad request: {0}")]
    BadRequest(String),

    // Erreur d'authentification / autorisation.
    #[error("Unauthorized")]
    Unauthorized,

    // Erreur d'accès interdit.
    #[error("Forbidden: {0}")]
    Forbidden(String),

    // Ressource absente.
    #[error("Not found: {0}")]
    NotFound(String),

    // Conflit de ressource.
    #[error("Conflict: {0}")]
    Conflict(String),

    // Conflit de participants (409 avec détails).
    #[error("Participant conflicts")]
    ConflictParticipants(Vec<ParticipantConflictItem>),

    // Fonctionnalité pas encore branchée.
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    // Erreur d'appel externe.
    #[error("Upstream error: {0}")]
    Upstream(String),

    // Erreur interne inattendue.
    #[error("Internal server error: {0}")]
    Internal(String),
}

// Conversion d'une erreur applicative vers une réponse HTTP JSON.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("APP ERROR => {:?}", self);
        let status = match &self {
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::ConflictParticipants(_) => StatusCode::CONFLICT,
            AppError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
            AppError::Upstream(_) => StatusCode::BAD_GATEWAY,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        // Pour ConflictParticipants, on retourne les détails structurés.
        if let AppError::ConflictParticipants(items) = self {
            #[derive(Serialize)]
            struct ConflictResponse {
                status: u16,
                error: &'static str,
                conflicts: Vec<ParticipantConflictItem>,
            }
            return (
                StatusCode::CONFLICT,
                Json(ConflictResponse {
                    status: 409,
                    error: "Participant Conflict",
                    conflicts: items,
                }),
            ).into_response();
        }

        // Construction du body JSON.
        let body = ErrorBody {
            status: status.as_u16(),
            error: status
                .canonical_reason()
                .unwrap_or("Unknown error")
                .to_string(),
            message: self.to_string(),
        };

        // Réponse finale.
        (status, Json(body)).into_response()
    }
}

// Conversion automatique des erreurs de validation vers AppError.
impl From<ValidationErrors> for AppError {
    fn from(value: ValidationErrors) -> Self {
        Self::Validation(value.to_string())
    }
}

// Conversion automatique des erreurs reqwest vers AppError.
impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self {
        Self::Upstream(value.to_string())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Internal(format!("Database error: {}", err))
    }
}
