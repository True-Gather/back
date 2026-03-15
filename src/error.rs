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

    // Ressource absente.
    #[error("Not found: {0}")]
    NotFound(String),

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
        let status = match &self {
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
            AppError::Upstream(_) => StatusCode::BAD_GATEWAY,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

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