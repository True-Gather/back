// Handlers API génériques.

use axum::{
    extract::{Extension, State},
    Json,
};
use chrono::Utc;
use sqlx::query;
use uuid::Uuid;
use validator::Validate;

use crate::{
    error::AppResult,
    models::{CreateMeetingRequest, CreateMeetingResponse, HealthResponse},
    state::{AppSession, AppState},
};

pub async fn health(State(_state): State<AppState>) -> AppResult<Json<HealthResponse>> {
    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        service: "truegather-backend".to_string(),
    }))
}

pub async fn create_meeting(
    State(state): State<AppState>,
    Extension(session): Extension<AppSession>,
    Json(payload): Json<CreateMeetingRequest>,
) -> AppResult<Json<CreateMeetingResponse>> {
    payload.validate()?;

    let meeting_id = Uuid::new_v4();
    let participant_id = Uuid::new_v4();
    let now = Utc::now();
    let room_code = Uuid::new_v4().simple().to_string()[..8].to_uppercase();
    let meeting_link = format!("{}/meeting/{}", state.config.frontend.base_url, meeting_id);

    query(
        r#"
        INSERT INTO meetings (
            meeting_id,
            host_keycloak_id,
            title,
            description,
            meeting_type,
            status,
            scheduled_start_at,
            scheduled_end_at,
            actual_start_at,
            actual_end_at,
            ai_enabled,
            meeting_link,
            room_code,
            created_at,
            updated_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15
        )
        "#,
    )
    .bind(meeting_id)
    .bind(&session.keycloak_sub)
    .bind(Option::<String>::None) // description
    .bind("instant")
    .bind("live")
    .bind(Option::<chrono::DateTime<Utc>>::None) // scheduled_start_at
    .bind(Option::<chrono::DateTime<Utc>>::None) // scheduled_end_at
    .bind(Some(now)) // actual_start_at
    .bind(Option::<chrono::DateTime<Utc>>::None) // actual_end_at
    .bind(payload.ai_enabled)
    .bind(&meeting_link)
    .bind(&room_code)
    .bind(now)
    .bind(now)
    .execute(&state.db)
    .await?;

    query(
        r#"
        INSERT INTO meeting_participants (
            meeting_participant_id,
            meeting_id,
            user_keycloak_id,
            role,
            status,
            invited_at,
            joined_at,
            left_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8
        )
        "#,
    )
    .bind(participant_id)
    .bind(meeting_id)
    .bind(&session.keycloak_sub)
    .bind("host")
    .bind("joined")
    .bind(now)
    .bind(Some(now))
    .bind(Option::<chrono::DateTime<Utc>>::None)
    .execute(&state.db)
    .await?;

    Ok(Json(CreateMeetingResponse {
        meeting_id: meeting_id.to_string(),
        title: payload.title,
        host_keycloak_id: session.keycloak_sub,
        participants_count: payload.participant_emails.len(),
        status: "live".to_string(),
    }))
}
