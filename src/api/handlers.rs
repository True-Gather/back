// Handlers API génériques.

use axum::{Json, extract::State};

// Imports internes.
use crate::{
    error::AppResult,
    models::HealthResponse,
    state::AppState,
};

// Healthcheck de base.
pub async fn health(State(_state): State<AppState>) -> AppResult<Json<HealthResponse>> {
    // Réponse minimale et stable.
    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        service: "truegather-backend".to_string(),
    }))
}
