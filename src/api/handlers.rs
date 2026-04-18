// Handlers API génériques.

use axum::{extract::State, Json};

use crate::{error::AppResult, models::HealthResponse, state::AppState};

// Healthcheck de base.
pub async fn health(State(_state): State<AppState>) -> AppResult<Json<HealthResponse>> {
    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        service: "truegather-backend".to_string(),
    }))
}