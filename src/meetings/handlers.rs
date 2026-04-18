// Handlers meetings.
//
// La session est extraite manuellement depuis le cookie HTTP-only,
// exactement comme dans auth/handlers.rs (pas d'extractor custom).

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use uuid::Uuid;

use crate::{
    auth::session::extract_session_id_from_headers,
    error::{AppError, AppResult, ParticipantConflictItem},
    state::AppState,
};

use super::dto::{CreateMeetingRequest, MeetingResponse, ParticipantResponse};

// ── Helper : récupère le keycloak_id depuis la session cookie ─────────────────

async fn require_session(state: &AppState, headers: &HeaderMap) -> AppResult<String> {
    let session_id = extract_session_id_from_headers(headers, &state.config.auth.cookie_name)
        .ok_or(AppError::Unauthorized)?;

    let sessions = state.sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or(AppError::Unauthorized)?;

    Ok(session.keycloak_id.clone())
}

// ── POST /api/v1/meetings ─────────────────────────────────────────────────────

pub async fn create_meeting(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateMeetingRequest>,
) -> AppResult<(StatusCode, Json<MeetingResponse>)> {
    let host_id = require_session(&state, &headers).await?;
    let pool = &state.db;

    // Limite de participants
    if body.participant_emails.len() > 50 {
        return Err(AppError::BadRequest(
            "Vous ne pouvez pas inviter plus de 50 participants.".to_string(),
        ));
    }

    // Rate limiting : max 20 meetings créés dans la dernière heure
    let recent_count: Option<i64> = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) FROM meetings
        WHERE host_keycloak_id = $1
          AND created_at > NOW() - INTERVAL '1 hour'
        "#,
        host_id
    )
    .fetch_one(pool)
    .await?;

    if recent_count.unwrap_or(0) >= 20 {
        return Err(AppError::BadRequest(
            "Vous avez créé trop de meetings récemment. Réessayez dans une heure.".to_string(),
        ));
    }

    // 1. Vérifier que l'hôte n'est pas déjà occupé
    // ... reste du code
    // 1. Vérifier que l'hôte n'est pas déjà occupé
    let host_conflict: Option<i64> = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) FROM meetings
        WHERE host_keycloak_id = $1
          AND status NOT IN ('cancelled', 'completed')
          AND scheduled_start_at < $3
          AND scheduled_end_at   > $2
        "#,
        host_id,
        body.scheduled_start_at,
        body.scheduled_end_at,
    )
    .fetch_one(pool)
    .await?;

    if host_conflict.unwrap_or(0) > 0 {
        return Err(AppError::Conflict(
            "Vous avez déjà un meeting prévu sur ce créneau.".to_string(),
        ));
    }

    // 2. Résoudre les emails → (keycloak_id, email, display_name)
    let mut participant_ids: Vec<(String, String, Option<String>)> = Vec::new();

    for email in &body.participant_emails {
        let row = sqlx::query!(
            "SELECT keycloak_id, email, display_name FROM users WHERE email = $1",
            email
        )
        .fetch_optional(pool)
        .await?;

        if let Some(u) = row {
            if u.keycloak_id != host_id {
                participant_ids.push((u.keycloak_id, u.email, u.display_name));
            }
        }
    }

    // 3. Résoudre les groupes → membres
    for group_id in &body.group_ids {
        let members = sqlx::query!(
            r#"
            SELECT u.keycloak_id, u.email, u.display_name
            FROM group_members gm
            JOIN users u ON u.keycloak_id = gm.user_keycloak_id
            WHERE gm.group_id = $1
            "#,
            group_id
        )
        .fetch_all(pool)
        .await?;

        for m in members {
            if m.keycloak_id != host_id
                && !participant_ids.iter().any(|(id, _, _)| *id == m.keycloak_id)
            {
                participant_ids.push((m.keycloak_id, m.email, m.display_name));
            }
        }
    }

    // 4. Détecter les conflits pour chaque participant
    let mut conflicts: Vec<ParticipantConflictItem> = Vec::new();

    for (pid, email, display_name) in &participant_ids {
        let conflict_row = sqlx::query!(
            r#"
            SELECT m.title,
                   m.scheduled_start_at,
                   m.scheduled_end_at
            FROM meetings m
            JOIN meeting_participants mp ON mp.meeting_id = m.meeting_id
            WHERE mp.user_keycloak_id = $1
              AND mp.status NOT IN ('declined', 'absent')
              AND m.status NOT IN ('cancelled', 'completed')
              AND m.scheduled_start_at < $3
              AND m.scheduled_end_at   > $2
            LIMIT 1
            "#,
            pid,
            body.scheduled_start_at,
            body.scheduled_end_at,
        )
        .fetch_optional(pool)
        .await?;

        if let Some(c) = conflict_row {
            conflicts.push(ParticipantConflictItem {
                email: email.clone(),
                display_name: display_name.clone(),
                conflicting_meeting_title: c.title,
                conflicting_start: c
                    .scheduled_start_at
                    .map(|d: chrono::DateTime<chrono::Utc>| d.to_rfc3339())
                    .unwrap_or_default(),
                conflicting_end: c
                    .scheduled_end_at
                    .map(|d: chrono::DateTime<chrono::Utc>| d.to_rfc3339())
                    .unwrap_or_default(),
            });
        }
    }

    if !conflicts.is_empty() {
        return Err(AppError::ConflictParticipants(conflicts));
    }

    // 5. Insérer le meeting
    let meeting_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO meetings (
            meeting_id, host_keycloak_id, title, description,
            meeting_type, status,
            scheduled_start_at, scheduled_end_at,
            ai_enabled
        ) VALUES ($1,$2,$3,$4,'scheduled','scheduled',$5,$6,$7)
        "#,
        meeting_id,
        host_id,
        body.title,
        body.description,
        body.scheduled_start_at,
        body.scheduled_end_at,
        body.ai_enabled,
    )
    .execute(pool)
    .await?;

    // 6. Insérer l'hôte comme participant (host / joined)
    sqlx::query!(
        r#"
        INSERT INTO meeting_participants
            (meeting_participant_id, meeting_id, user_keycloak_id, role, status)
        VALUES ($1,$2,$3,'host','joined')
        "#,
        Uuid::new_v4(),
        meeting_id,
        host_id,
    )
    .execute(pool)
    .await?;

    // 7. Insérer les participants invités
    let mut participants_response: Vec<ParticipantResponse> = Vec::new();

    for (pid, email, display_name) in &participant_ids {
        sqlx::query!(
            r#"
            INSERT INTO meeting_participants
                (meeting_participant_id, meeting_id, user_keycloak_id, role, status)
            VALUES ($1,$2,$3,'participant','invited')
            ON CONFLICT (meeting_id, user_keycloak_id) DO NOTHING
            "#,
            Uuid::new_v4(),
            meeting_id,
            pid,
        )
        .execute(pool)
        .await?;

        participants_response.push(ParticipantResponse {
            keycloak_id: pid.clone(),
            email: email.clone(),
            display_name: display_name.clone(),
            role: "participant".to_string(),
            status: "invited".to_string(),
        });
    }

    // 8. Insérer les invitations de groupes
    for group_id in &body.group_ids {
        sqlx::query!(
            r#"
            INSERT INTO meeting_group_invites
                (meeting_group_invite_id, meeting_id, group_id, invited_by_keycloak_id)
            VALUES ($1,$2,$3,$4)
            ON CONFLICT (meeting_id, group_id) DO NOTHING
            "#,
            Uuid::new_v4(),
            meeting_id,
            group_id,
            host_id,
        )
        .execute(pool)
        .await?;
    }

    // 9. Notifications pour chaque invité
    let host_display: String = sqlx::query_scalar!(
    "SELECT display_name FROM users WHERE keycloak_id = $1",
    host_id
)
.fetch_optional(pool)
.await?
.flatten()
.unwrap_or_else(|| "Quelqu'un".to_string());
    for (pid, _, _) in &participant_ids {
        sqlx::query!(
            r#"
            INSERT INTO notifications
                (notification_id, user_keycloak_id, type, title, message, related_meeting_id)
            VALUES ($1,$2,'meeting_invite',$3,$4,$5)
            "#,
            Uuid::new_v4(),
            pid,
            format!("Invitation : {}", body.title),
            format!("{} vous a invité au meeting \"{}\"", host_display, body.title),
            meeting_id,
        )
        .execute(pool)
        .await?;
    }

    Ok((
        StatusCode::CREATED,
        Json(MeetingResponse {
            meeting_id,
            title: body.title,
            description: body.description,
            host_keycloak_id: host_id,
            scheduled_start_at: body.scheduled_start_at,
            scheduled_end_at: body.scheduled_end_at,
            ai_enabled: body.ai_enabled,
            status: "scheduled".to_string(),
            participants: participants_response,
        }),
    ))
}

// ── GET /api/v1/meetings ──────────────────────────────────────────────────────
// Retourne UNIQUEMENT les meetings où l'user est host OU participant invité.

pub async fn list_meetings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<MeetingResponse>>> {
    let user_id = require_session(&state, &headers).await?;
    let pool = &state.db;

    let rows = sqlx::query!(
        r#"
        SELECT DISTINCT
            m.meeting_id,
            m.title,
            m.description,
            m.host_keycloak_id,
            m.scheduled_start_at,
            m.scheduled_end_at,
            m.ai_enabled,
            m.status
        FROM meetings m
        LEFT JOIN meeting_participants mp ON mp.meeting_id = m.meeting_id
        WHERE m.status != 'cancelled'
          AND (
              m.host_keycloak_id = $1
              OR (mp.user_keycloak_id = $1 AND mp.status != 'declined')
          )
        ORDER BY m.scheduled_start_at ASC
        "#,
        user_id
    )
    .fetch_all(pool)
    .await?;

    let mut meetings: Vec<MeetingResponse> = Vec::new();

    for row in rows {
        let parts = sqlx::query!(
            r#"
            SELECT u.keycloak_id, u.email, u.display_name, mp.role, mp.status
            FROM meeting_participants mp
            JOIN users u ON u.keycloak_id = mp.user_keycloak_id
            WHERE mp.meeting_id = $1
            "#,
            row.meeting_id
        )
        .fetch_all(pool)
        .await?;

        meetings.push(MeetingResponse {
            meeting_id: row.meeting_id,
            title: row.title,
            description: row.description,
            host_keycloak_id: row.host_keycloak_id,
            scheduled_start_at: row.scheduled_start_at.expect("start not null"),
            scheduled_end_at: row.scheduled_end_at.expect("end not null"),
            ai_enabled: row.ai_enabled,
            status: row.status,
            participants: parts
                .into_iter()
                .map(|p| ParticipantResponse {
                    keycloak_id: p.keycloak_id,
                    email: p.email,
                    display_name: p.display_name,
                    role: p.role,
                    status: p.status,
                })
                .collect(),
        });
    }

    Ok(Json(meetings))
}

// ── DELETE /api/v1/meetings/:id ───────────────────────────────────────────────

pub async fn delete_meeting(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meeting_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let user_id = require_session(&state, &headers).await?;
    let pool = &state.db;

    let count: Option<i64> = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM meetings WHERE meeting_id = $1 AND host_keycloak_id = $2",
        meeting_id,
        user_id
    )
    .fetch_one(pool)
    .await?;

    if count.unwrap_or(0) == 0 {
        return Err(AppError::Forbidden(
            "Vous n'êtes pas l'hôte de ce meeting.".to_string(),
        ));
    }

    sqlx::query!(
        "UPDATE meetings SET status = 'cancelled' WHERE meeting_id = $1",
        meeting_id
    )
    .execute(pool)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}