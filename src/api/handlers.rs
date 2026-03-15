// Handlers API génériques.

use axum::{
    extract::State,
    Json,
};

// Imports internes.
use crate::{
    error::AppResult,
    models::{CreateMeetingRequest, CreateMeetingResponse, HealthResponse},
    state::AppState,
};
use validator::Validate;

// Healthcheck de base.
pub async fn health(State(_state): State<AppState>) -> AppResult<Json<HealthResponse>> {
    // Réponse minimale et stable.
    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        service: "truegather-backend".to_string(),
    }))
}

// Placeholder propre pour la création de meeting.
pub async fn create_meeting(
    State(_state): State<AppState>,
    Json(payload): Json<CreateMeetingRequest>,
) -> AppResult<Json<CreateMeetingResponse>> {
    // Validation du payload.
    payload.validate()?;

    // Réponse minimale cohérente avec le front.
    Ok(Json(CreateMeetingResponse {
        message: "Meeting payload accepted by backend skeleton".to_string(),
        title: payload.title,
        participants_count: payload.participant_emails.len(),
    }))
}