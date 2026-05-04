// Handlers meetings.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    auth::session::extract_session_id_from_headers,
    error::{AppError, AppResult, ParticipantConflictItem},
    state::AppState,
};

use super::dto::{
    CreateInstantMeetingRequest, CreateMeetingRequest, InstantMeetingResponse,
    MeetingDetailResponse, MeetingResponse, ParticipantResponse,
};

// Helper : récupère le keycloak_id depuis la session cookie.
async fn require_session(state: &AppState, headers: &HeaderMap) -> AppResult<String> {
    let session_id = extract_session_id_from_headers(headers, &state.config.auth.cookie_name)
        .ok_or(AppError::Unauthorized)?;

    let sessions = state.sessions.read().await;
    let session = sessions.get(&session_id).ok_or(AppError::Unauthorized)?;

    Ok(session.keycloak_id.clone())
}

// POST /api/v1/meetings/instant
pub async fn create_instant_meeting(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateInstantMeetingRequest>,
) -> AppResult<(StatusCode, Json<InstantMeetingResponse>)> {
    let host_id = require_session(&state, &headers).await?;
    let pool = &state.db;
    let now = Utc::now();

    if body.title.trim().is_empty() {
        return Err(AppError::BadRequest("Le titre est requis.".to_string()));
    }

    let recent_count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM meetings WHERE host_keycloak_id = $1 AND created_at > NOW() - INTERVAL '1 hour'",
    )
    .bind(&host_id)
    .fetch_one(pool)
    .await?;

    if recent_count.unwrap_or(0) >= 20 {
        return Err(AppError::BadRequest(
            "Vous avez créé trop de meetings récemment. Réessayez dans une heure.".to_string(),
        ));
    }

    let meeting_id = Uuid::new_v4();
    let room_code = Uuid::new_v4().simple().to_string()[..8].to_uppercase();
    let meeting_link = format!(
        "{}/meeting/{}",
        state.config.frontend.base_url, meeting_id
    );

    sqlx::query(
        r#"
        INSERT INTO meetings (
            meeting_id, host_keycloak_id, title, description,
            meeting_type, status, actual_start_at, ai_enabled,
            meeting_link, room_code
        ) VALUES ($1,$2,$3,NULL,'instant','live',$4,$5,$6,$7)
        "#,
    )
    .bind(meeting_id)
    .bind(&host_id)
    .bind(&body.title)
    .bind(now)
    .bind(body.ai_enabled)
    .bind(&meeting_link)
    .bind(&room_code)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO meeting_participants (meeting_participant_id, meeting_id, user_keycloak_id, role, status) VALUES ($1,$2,$3,'host','joined')",
    )
    .bind(Uuid::new_v4())
    .bind(meeting_id)
    .bind(&host_id)
    .execute(pool)
    .await?;

    let mut participant_ids: Vec<(String, String, Option<String>)> = Vec::new();

    for email in &body.participant_emails {
        let row = sqlx::query(
            "SELECT keycloak_id, email, display_name FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(pool)
        .await?;

        if let Some(u) = row {
            let kid: String = u.get("keycloak_id");
            if kid != host_id {
                participant_ids.push((kid, u.get("email"), u.get("display_name")));
            }
        }
    }

    if let Some(group_id) = body.group_id {
        let members = sqlx::query(
            r#"
            SELECT u.keycloak_id, u.email, u.display_name
            FROM group_members gm
            JOIN users u ON u.keycloak_id = gm.user_keycloak_id
            WHERE gm.group_id = $1
            "#,
        )
        .bind(group_id)
        .fetch_all(pool)
        .await?;

        for m in members {
            let kid: String = m.get("keycloak_id");
            if kid != host_id && !participant_ids.iter().any(|(id, _, _)| *id == kid) {
                participant_ids.push((kid, m.get("email"), m.get("display_name")));
            }
        }
    }

    let host_display: Option<String> = sqlx::query_scalar(
        "SELECT display_name FROM users WHERE keycloak_id = $1",
    )
    .bind(&host_id)
    .fetch_optional(pool)
    .await?
    .flatten();
    let host_display = host_display.unwrap_or_else(|| "Quelqu'un".to_string());

    let participants_count = participant_ids.len();

    for (pid, _, _) in &participant_ids {
        sqlx::query(
            "INSERT INTO meeting_participants (meeting_participant_id, meeting_id, user_keycloak_id, role, status) VALUES ($1,$2,$3,'participant','invited') ON CONFLICT (meeting_id, user_keycloak_id) DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(meeting_id)
        .bind(pid)
        .execute(pool)
        .await?;

        sqlx::query(
            "INSERT INTO notifications (notification_id, user_keycloak_id, type, title, message, related_meeting_id) VALUES ($1,$2,'meeting_invite',$3,$4,$5)",
        )
        .bind(Uuid::new_v4())
        .bind(pid)
        .bind(format!("Invitation : {}", body.title))
        .bind(format!(
            "{} vous a invité au meeting \"{}\"",
            host_display, body.title
        ))
        .bind(meeting_id)
        .execute(pool)
        .await?;
    }

    // Envoi des emails aux invités (non bloquant — on ne fait pas échouer si SMTP indisponible)
    for (pid, email, display_name) in &participant_ids {
        let _ = pid;
        let mail = state.mail.clone();
        let email = email.clone();
        let display_name = display_name.clone();
        let host_display = host_display.clone();
        let meeting_title = body.title.clone();
        let meeting_link = meeting_link.clone();
        let room_code = room_code.clone();

        tokio::spawn(async move {
            mail.send_meeting_invitation(
                &email,
                display_name.as_deref(),
                &host_display,
                &meeting_title,
                &meeting_link,
                &room_code,
            )
            .await;
        });
    }

    Ok((
        StatusCode::CREATED,
        Json(InstantMeetingResponse {
            meeting_id,
            title: body.title,
            meeting_link,
            room_code,
            status: "live".to_string(),
            participants_count,
        }),
    ))
}

// GET /api/v1/meetings/:id
pub async fn get_meeting_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meeting_id): Path<Uuid>,
) -> AppResult<Json<MeetingDetailResponse>> {
    let user_id = require_session(&state, &headers).await?;
    let pool = &state.db;

    // Vérifier que l'utilisateur est hôte ou participant invité
    let meeting_row = sqlx::query(
        r#"
        SELECT
            m.meeting_id, m.title, m.status, m.meeting_type,
            m.ai_enabled, m.room_code, m.meeting_link, m.host_keycloak_id
        FROM meetings m
        WHERE m.meeting_id = $1
          AND m.status != 'cancelled'
        "#,
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound("Meeting introuvable.".to_string()))?;

    let host_keycloak_id: String = meeting_row.get("host_keycloak_id");
    let is_host = host_keycloak_id == user_id;

    // Si pas l'hôte, vérifier qu'il est participant
    if !is_host {
        let participant_count: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM meeting_participants
            WHERE meeting_id = $1 AND user_keycloak_id = $2
              AND status NOT IN ('declined', 'absent')
            "#,
        )
        .bind(meeting_id)
        .bind(&user_id)
        .fetch_one(pool)
        .await?;

        if participant_count.unwrap_or(0) == 0 {
            return Err(AppError::Forbidden(
                "Vous n'êtes pas autorisé à accéder à cette réunion.".to_string(),
            ));
        }
    }

    let user_role = if is_host { "host" } else { "participant" };

    let parts = sqlx::query(
        r#"
        SELECT u.keycloak_id, u.email, u.display_name, mp.role, mp.status
        FROM meeting_participants mp
        JOIN users u ON u.keycloak_id = mp.user_keycloak_id
        WHERE mp.meeting_id = $1
        "#,
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;

    let participants = parts
        .into_iter()
        .map(|p| ParticipantResponse {
            keycloak_id: p.get("keycloak_id"),
            email: p.get("email"),
            display_name: p.get("display_name"),
            role: p.get("role"),
            status: p.get("status"),
        })
        .collect();

    Ok(Json(MeetingDetailResponse {
        meeting_id,
        title: meeting_row.get("title"),
        status: meeting_row.get("status"),
        meeting_type: meeting_row.get("meeting_type"),
        ai_enabled: meeting_row.get("ai_enabled"),
        room_code: meeting_row.get("room_code"),
        meeting_link: meeting_row.get("meeting_link"),
        host_keycloak_id,
        participants,
        user_role: user_role.to_string(),
    }))
}

// POST /api/v1/meetings
pub async fn create_meeting(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateMeetingRequest>,
) -> AppResult<(StatusCode, Json<MeetingResponse>)> {
    let host_id = require_session(&state, &headers).await?;
    let pool = &state.db;

    if body.participant_emails.len() > 50 {
        return Err(AppError::BadRequest(
            "Vous ne pouvez pas inviter plus de 50 participants.".to_string(),
        ));
    }

    // Rate limiting : max 20 meetings créés dans la dernière heure.
    let recent_count: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM meetings
        WHERE host_keycloak_id = $1
          AND created_at > NOW() - INTERVAL '1 hour'
        "#,
    )
    .bind(&host_id)
    .fetch_one(pool)
    .await?;

    if recent_count.unwrap_or(0) >= 20 {
        return Err(AppError::BadRequest(
            "Vous avez créé trop de meetings récemment. Réessayez dans une heure.".to_string(),
        ));
    }

    // Vérifier que l'hôte n'est pas déjà occupé.
    let host_conflict: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM meetings
        WHERE host_keycloak_id = $1
          AND status NOT IN ('cancelled', 'completed')
          AND scheduled_start_at < $3
          AND scheduled_end_at   > $2
        "#,
    )
    .bind(&host_id)
    .bind(body.scheduled_start_at)
    .bind(body.scheduled_end_at)
    .fetch_one(pool)
    .await?;

    if host_conflict.unwrap_or(0) > 0 {
        return Err(AppError::Conflict(
            "Vous avez déjà un meeting prévu sur ce créneau.".to_string(),
        ));
    }

    // Résoudre les emails → (keycloak_id, email, display_name).
    let mut participant_ids: Vec<(String, String, Option<String>)> = Vec::new();

    for email in &body.participant_emails {
        let row = sqlx::query(
            "SELECT keycloak_id, email, display_name FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(pool)
        .await?;

        if let Some(u) = row {
            let kid: String = u.get("keycloak_id");
            if kid != host_id {
                participant_ids.push((kid, u.get("email"), u.get("display_name")));
            }
        }
    }

    // Résoudre les groupes → membres.
    for group_id in &body.group_ids {
        let members = sqlx::query(
            r#"
            SELECT u.keycloak_id, u.email, u.display_name
            FROM group_members gm
            JOIN users u ON u.keycloak_id = gm.user_keycloak_id
            WHERE gm.group_id = $1
            "#,
        )
        .bind(group_id)
        .fetch_all(pool)
        .await?;

        for m in members {
            let kid: String = m.get("keycloak_id");
            if kid != host_id && !participant_ids.iter().any(|(id, _, _)| *id == kid) {
                participant_ids.push((kid, m.get("email"), m.get("display_name")));
            }
        }
    }

    // Détecter les conflits pour chaque participant.
    let mut conflicts: Vec<ParticipantConflictItem> = Vec::new();

    for (pid, email, display_name) in &participant_ids {
        let conflict_row = sqlx::query(
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
        )
        .bind(pid)
        .bind(body.scheduled_start_at)
        .bind(body.scheduled_end_at)
        .fetch_optional(pool)
        .await?;

        if let Some(c) = conflict_row {
            let start: Option<chrono::DateTime<chrono::Utc>> = c.get("scheduled_start_at");
            let end: Option<chrono::DateTime<chrono::Utc>> = c.get("scheduled_end_at");
            conflicts.push(ParticipantConflictItem {
                email: email.clone(),
                display_name: display_name.clone(),
                conflicting_meeting_title: c.get("title"),
                conflicting_start: start.map(|d| d.to_rfc3339()).unwrap_or_default(),
                conflicting_end: end.map(|d| d.to_rfc3339()).unwrap_or_default(),
            });
        }
    }

    if !conflicts.is_empty() {
        return Err(AppError::ConflictParticipants(conflicts));
    }

    // Insérer le meeting.
    let meeting_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO meetings (
            meeting_id, host_keycloak_id, title, description,
            meeting_type, status,
            scheduled_start_at, scheduled_end_at,
            ai_enabled
        ) VALUES ($1,$2,$3,$4,'scheduled','scheduled',$5,$6,$7)
        "#,
    )
    .bind(meeting_id)
    .bind(&host_id)
    .bind(&body.title)
    .bind(&body.description)
    .bind(body.scheduled_start_at)
    .bind(body.scheduled_end_at)
    .bind(body.ai_enabled)
    .execute(pool)
    .await?;

    // Insérer l'hôte comme participant (host / joined).
    sqlx::query(
        r#"
        INSERT INTO meeting_participants
            (meeting_participant_id, meeting_id, user_keycloak_id, role, status)
        VALUES ($1,$2,$3,'host','joined')
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(meeting_id)
    .bind(&host_id)
    .execute(pool)
    .await?;

    // Insérer les participants invités.
    let mut participants_response: Vec<ParticipantResponse> = Vec::new();

    for (pid, email, display_name) in &participant_ids {
        sqlx::query(
            r#"
            INSERT INTO meeting_participants
                (meeting_participant_id, meeting_id, user_keycloak_id, role, status)
            VALUES ($1,$2,$3,'participant','invited')
            ON CONFLICT (meeting_id, user_keycloak_id) DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(meeting_id)
        .bind(pid)
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

    // Insérer les invitations de groupes.
    for group_id in &body.group_ids {
        sqlx::query(
            r#"
            INSERT INTO meeting_group_invites
                (meeting_group_invite_id, meeting_id, group_id, invited_by_keycloak_id)
            VALUES ($1,$2,$3,$4)
            ON CONFLICT (meeting_id, group_id) DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(meeting_id)
        .bind(group_id)
        .bind(&host_id)
        .execute(pool)
        .await?;
    }

    // Notifications pour chaque invité.
    let host_display: Option<String> = sqlx::query_scalar(
        "SELECT display_name FROM users WHERE keycloak_id = $1",
    )
    .bind(&host_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let host_display = host_display.unwrap_or_else(|| "Quelqu'un".to_string());

    for (pid, _, _) in &participant_ids {
        sqlx::query(
            r#"
            INSERT INTO notifications
                (notification_id, user_keycloak_id, type, title, message, related_meeting_id)
            VALUES ($1,$2,'meeting_invite',$3,$4,$5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(pid)
        .bind(format!("Invitation : {}", body.title))
        .bind(format!("{} vous a invité au meeting \"{}\"", host_display, body.title))
        .bind(meeting_id)
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

// GET /api/v1/meetings
pub async fn list_meetings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<MeetingResponse>>> {
    let user_id = require_session(&state, &headers).await?;
    let pool = &state.db;

    let rows = sqlx::query(
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
          AND m.meeting_type = 'scheduled'
          AND (
              m.host_keycloak_id = $1
              OR (mp.user_keycloak_id = $1 AND mp.status != 'declined')
          )
        ORDER BY m.scheduled_start_at ASC
        "#,
    )
    .bind(&user_id)
    .fetch_all(pool)
    .await?;

    let mut meetings: Vec<MeetingResponse> = Vec::new();

    for row in rows {
        let meeting_id: Uuid = row.get("meeting_id");

        let parts = sqlx::query(
            r#"
            SELECT u.keycloak_id, u.email, u.display_name, mp.role, mp.status
            FROM meeting_participants mp
            JOIN users u ON u.keycloak_id = mp.user_keycloak_id
            WHERE mp.meeting_id = $1
            "#,
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;

        let start: Option<chrono::DateTime<chrono::Utc>> = row.get("scheduled_start_at");
        let end: Option<chrono::DateTime<chrono::Utc>> = row.get("scheduled_end_at");

        meetings.push(MeetingResponse {
            meeting_id,
            title: row.get("title"),
            description: row.get("description"),
            host_keycloak_id: row.get("host_keycloak_id"),
            scheduled_start_at: start.unwrap_or_default(),
            scheduled_end_at: end.unwrap_or_default(),
            ai_enabled: row.get("ai_enabled"),
            status: row.get("status"),
            participants: parts
                .into_iter()
                .map(|p| ParticipantResponse {
                    keycloak_id: p.get("keycloak_id"),
                    email: p.get("email"),
                    display_name: p.get("display_name"),
                    role: p.get("role"),
                    status: p.get("status"),
                })
                .collect(),
        });
    }

    Ok(Json(meetings))
}

// DELETE /api/v1/meetings/:id
pub async fn delete_meeting(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meeting_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let user_id = require_session(&state, &headers).await?;
    let pool = &state.db;

    let count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM meetings WHERE meeting_id = $1 AND host_keycloak_id = $2",
    )
    .bind(meeting_id)
    .bind(&user_id)
    .fetch_one(pool)
    .await?;

    if count.unwrap_or(0) == 0 {
        return Err(AppError::Forbidden(
            "Vous n'êtes pas l'hôte de ce meeting.".to_string(),
        ));
    }

    sqlx::query("UPDATE meetings SET status = 'cancelled' WHERE meeting_id = $1")
        .bind(meeting_id)
        .execute(pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
