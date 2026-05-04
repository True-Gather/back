// Handlers API génériques.
//
// Ce fichier contient :
// - healthcheck,
// - dashboard,
// - planning,
// - placeholder meeting,
// - groupes,
// - invitations de groupe,
// - recherche utilisateurs,
// - upload photo groupe.
//
// Important sécurité :
// - l'utilisateur courant est toujours déduit de la session backend,
// - aucun owner / user_id libre n'est accepté depuis le frontend,
// - les invitations de groupe passent par un état pending avant acceptation.

use std::collections::HashSet;

use axum::{
    extract::{Multipart, Path, Query, State},
    Json,
};
use chrono::{NaiveDateTime, Utc};
use sqlx::Row;
use uuid::Uuid;
use validator::Validate;

use crate::{
    auth::middleware::CurrentUser,
    error::{AppError, AppResult},
    models::{
        CreateGroupRequest,
        CreateGroupResponse,
        CreateMeetingRequest,
        CreateMeetingResponse,
        DashboardMeeting,
        DashboardNotification,
        DashboardResponse,
        DashboardStats,
        DashboardUser,
        GroupActionResponse,
        GroupDetailInfo,
        GroupDetailResponse,
        GroupInvitationActionResponse,
        GroupInvitationItem,
        GroupInvitationsListResponse,
        GroupListItem,
        GroupMemberItem,
        GroupsListResponse,
        HealthResponse,
        InviteGroupMemberRequest,
        PlanningMeeting,
        PlanningResponse,
        RespondToGroupInvitationRequest,
        SearchUsersQuery,
        UserSearchItem,
        UserSearchResponse,
    },
    state::AppState,
};

// =========================
// Helpers internes
// =========================

fn build_display_name(
    display_name_raw: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    fallback_email: &str,
) -> String {
    display_name_raw
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            let first = first_name.unwrap_or_default();
            let last = last_name.unwrap_or_default();
            let combined = format!("{} {}", first, last).trim().to_string();

            if combined.is_empty() {
                None
            } else {
                Some(combined)
            }
        })
        .unwrap_or_else(|| fallback_email.to_string())
}

async fn load_group_name(
    db: &sqlx::PgPool,
    group_id: Uuid,
) -> AppResult<String> {
    let row = sqlx::query(
        r#"
        SELECT name
        FROM groups
        WHERE group_id = $1
        "#,
    )
    .bind(group_id)
    .fetch_optional(db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to load group: {}", error)))?;

    let Some(row) = row else {
        return Err(AppError::BadRequest("Group not found".to_string()));
    };

    Ok(row.get::<String, _>("name"))
}

async fn load_group_role(
    db: &sqlx::PgPool,
    group_id: Uuid,
    user_keycloak_id: &str,
) -> AppResult<Option<String>> {
    let row = sqlx::query(
        r#"
        SELECT role::text AS role
        FROM group_members
        WHERE group_id = $1
          AND user_keycloak_id = $2
        "#,
    )
    .bind(group_id)
    .bind(user_keycloak_id)
    .fetch_optional(db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to verify group role: {}", error)))?;

    Ok(row.map(|value| value.get::<String, _>("role")))
}

async fn create_group_invitation_and_notification(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    group_id: Uuid,
    group_name: &str,
    invited_user_keycloak_id: &str,
    invited_user_display_name: &str,
    invited_user_email: &str,
    invited_by_user_keycloak_id: &str,
    invited_by_display_name: &str,
) -> AppResult<()> {
    let invitation_id = Uuid::new_v4();
    let notification_id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        r#"
        INSERT INTO group_invitations (
            group_invitation_id,
            group_id,
            invited_user_keycloak_id,
            invited_by_user_keycloak_id,
            status,
            created_at
        )
        VALUES ($1, $2, $3, $4, 'pending', $5)
        "#,
    )
    .bind(invitation_id)
    .bind(group_id)
    .bind(invited_user_keycloak_id)
    .bind(invited_by_user_keycloak_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to create group invitation: {}", error)))?;

    let title = format!("Ajout au groupe {}", group_name);
    let message = format!(
        "{} vous a invité à rejoindre le groupe {}",
        invited_by_display_name, group_name
    );

        // Notification envoyée au membre invité.
    // Important :
    // la table notifications possède encore une colonne historique `type`
    // en NOT NULL, donc on renseigne à la fois `type` et `notification_type`.
    sqlx::query(
        r#"
        INSERT INTO notifications (
            notification_id,
            user_keycloak_id,
            type,
            title,
            message,
            is_read,
            created_at,
            notification_type,
            related_group_id,
            related_group_invitation_id,
            action_status
        )
        VALUES ($1, $2, $3, $4, $5, FALSE, $6, $7, $8, $9, $10)
        "#,
    )
    .bind(notification_id)
    .bind(invited_user_keycloak_id)
    .bind("group_invitation")
    .bind(&title)
    .bind(&message)
    .bind(now.naive_utc())
    .bind("group_invitation")
    .bind(group_id)
    .bind(invitation_id)
    .bind("pending")
    .execute(&mut **tx)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to create notification: {}", error)))?;

    let _ = invited_user_display_name;
    let _ = invited_user_email;

    Ok(())
}

// =========================
// Health
// =========================

pub async fn health(State(_state): State<AppState>) -> AppResult<Json<HealthResponse>> {
    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        service: "truegather-backend".to_string(),
    }))
}

// =========================
// Meeting placeholder
// =========================

pub async fn create_meeting(
    State(_state): State<AppState>,
    Json(payload): Json<CreateMeetingRequest>,
) -> AppResult<Json<CreateMeetingResponse>> {
    payload.validate()?;

    Ok(Json(CreateMeetingResponse {
        message: "Meeting payload accepted by backend skeleton".to_string(),
        title: payload.title,
        participants_count: payload.participant_emails.len(),
    }))
}

// =========================
// Dashboard
// =========================

pub async fn get_dashboard(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<DashboardResponse>> {
    let total_meetings = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM meetings
        WHERE host_keycloak_id = $1
        "#,
    )
    .bind(&user.keycloak_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let total_groups = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM group_members
        WHERE user_keycloak_id = $1
        "#,
    )
    .bind(&user.keycloak_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let unread_notifications = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM notifications
        WHERE user_keycloak_id = $1
          AND is_read = FALSE
        "#,
    )
    .bind(&user.keycloak_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let meeting_rows = sqlx::query(
        r#"
        SELECT
            meeting_id,
            title,
            status,
            meeting_type,
            scheduled_start_at,
            scheduled_end_at
        FROM meetings
        WHERE host_keycloak_id = $1
        ORDER BY created_at DESC
        LIMIT 5
        "#,
    )
    .bind(&user.keycloak_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let recent_meetings = meeting_rows
        .into_iter()
        .map(|row| {
            let meeting_id: Uuid = row.get("meeting_id");
            let title: String = row.get("title");
            let status: String = row.get("status");
            let meeting_type: String = row.get("meeting_type");
            let scheduled_start_at: Option<NaiveDateTime> = row.get("scheduled_start_at");
            let scheduled_end_at: Option<NaiveDateTime> = row.get("scheduled_end_at");

            DashboardMeeting {
                id: meeting_id.to_string(),
                title,
                status,
                meeting_type,
                scheduled_start_at: scheduled_start_at.map(|dt| dt.to_string()),
                scheduled_end_at: scheduled_end_at.map(|dt| dt.to_string()),
            }
        })
        .collect::<Vec<_>>();

    let notification_rows = sqlx::query(
        r#"
        SELECT
            notification_id,
            title,
            message,
            is_read,
            created_at,
            notification_type,
            related_group_id,
            related_group_invitation_id,
            action_status
        FROM notifications
        WHERE user_keycloak_id = $1
        ORDER BY created_at DESC
        LIMIT 20
        "#,
    )
    .bind(&user.keycloak_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let notifications = notification_rows
        .into_iter()
        .map(|row| {
            let notification_id: Uuid = row.get("notification_id");
            let title: String = row.get("title");
            let message: String = row.get("message");
            let is_read: bool = row.get("is_read");

            // created_at est en TIMESTAMPTZ côté PostgreSQL.
            let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");

            let notification_type: Option<String> = row.get("notification_type");
            let related_group_id: Option<Uuid> = row.get("related_group_id");
            let related_group_invitation_id: Option<Uuid> =
                row.get("related_group_invitation_id");
            let action_status: Option<String> = row.get("action_status");

            DashboardNotification {
                id: notification_id.to_string(),
                title,
                message,
                is_read,

                // Format propre pour le frontend.
                created_at: created_at.to_rfc3339(),

                notification_type: notification_type
                    .unwrap_or_else(|| "generic".to_string()),
                related_group_id: related_group_id.map(|value| value.to_string()),
                related_group_invitation_id: related_group_invitation_id
                    .map(|value| value.to_string()),
                action_status,
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(DashboardResponse {
        user: DashboardUser {
            id: user.keycloak_id,
            email: user.email,
            display_name: user.display_name,
        },
        stats: DashboardStats {
            total_meetings,
            total_groups,
            unread_notifications,
        },
        recent_meetings,
        notifications,
    }))
}

// =========================
// Planning
// =========================

pub async fn get_planning(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<PlanningResponse>> {
    let meeting_rows = sqlx::query(
        r#"
        SELECT DISTINCT
            m.meeting_id,
            m.title,
            m.scheduled_start_at,
            m.scheduled_end_at,
            m.status,
            m.meeting_type,
            m.ai_enabled,
            m.created_at
        FROM meetings m
        LEFT JOIN meeting_participants mp
            ON mp.meeting_id = m.meeting_id
        WHERE
            m.host_keycloak_id = $1
            OR mp.user_keycloak_id = $1
        ORDER BY
            m.scheduled_start_at ASC NULLS LAST,
            m.created_at DESC
        "#,
    )
    .bind(&user.keycloak_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut meetings = Vec::new();

    for row in meeting_rows {
        let meeting_id: Uuid = row.get("meeting_id");
        let title: String = row.get("title");
        let scheduled_start_at: Option<NaiveDateTime> = row.get("scheduled_start_at");
        let scheduled_end_at: Option<NaiveDateTime> = row.get("scheduled_end_at");
        let status: String = row.get("status");
        let meeting_type: String = row.get("meeting_type");
        let ia_enabled: bool = row.get("ai_enabled");

        let participant_rows = sqlx::query(
            r#"
            SELECT u.email
            FROM meeting_participants mp
            INNER JOIN users u
                ON u.keycloak_id = mp.user_keycloak_id
            WHERE mp.meeting_id = $1
            ORDER BY u.email ASC
            "#,
        )
        .bind(meeting_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let participants = participant_rows
            .into_iter()
            .map(|row| row.get::<String, _>("email"))
            .collect::<Vec<_>>();

        let date = scheduled_start_at
            .map(|dt| dt.date().to_string())
            .unwrap_or_default();

        let start = scheduled_start_at
            .map(|dt| dt.time().format("%H:%M").to_string())
            .unwrap_or_default();

        let end = scheduled_end_at
            .map(|dt| dt.time().format("%H:%M").to_string())
            .unwrap_or_default();

        meetings.push(PlanningMeeting {
            id: meeting_id.to_string(),
            title,
            date,
            start,
            end,
            participants,
            group_ids: vec![],
            ia_enabled,
            status,
            meeting_type,
        });
    }

    Ok(Json(PlanningResponse {
        meetings,
        tasks: vec![],
    }))
}

// =========================
// Groups
// =========================

// Crée un groupe.
//
// Sécurité :
// - l'owner est toujours l'utilisateur connecté,
// - le frontend ne choisit jamais l'owner,
// - les emails fournis à la création génèrent des invitations "pending",
//   et non un ajout direct dans group_members.
pub async fn create_group(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(payload): Json<CreateGroupRequest>,
) -> AppResult<Json<CreateGroupResponse>> {
    payload.validate()?;

    let now = Utc::now().naive_utc();
    let group_id = Uuid::new_v4();
    let group_name = payload.name.trim().to_string();

    let group_description = match payload.description {
        Some(raw_description) => {
            let trimmed = raw_description.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        None => None,
    };

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|error| AppError::Internal(format!("Failed to begin transaction: {}", error)))?;

    sqlx::query(
        r#"
        INSERT INTO groups (
            group_id,
            owner_keycloak_id,
            name,
            description,
            profile_photo_url,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(group_id)
    .bind(&user.keycloak_id)
    .bind(&group_name)
    .bind(&group_description)
    .bind(Option::<String>::None)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to create group: {}", error)))?;

    let owner_group_member_id = Uuid::new_v4();

    sqlx::query(
    r#"
        INSERT INTO group_members (
            group_member_id,
            group_id,
            user_keycloak_id,
            role,
            joined_at
        )
        VALUES ($1, $2, $3, $4::group_member_role, $5)
        "#,
    )
    .bind(owner_group_member_id)
    .bind(group_id)
    .bind(&user.keycloak_id)
    .bind("owner")
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to create owner membership: {}", error)))?;

    let mut seen_emails = HashSet::<String>::new();

    for raw_email in payload.member_emails {
        let normalized_email = raw_email.trim().to_lowercase();

        if normalized_email.is_empty() {
            continue;
        }

        if normalized_email == user.email.to_lowercase() {
            continue;
        }

        if !seen_emails.insert(normalized_email.clone()) {
            continue;
        }

        let maybe_target_user = sqlx::query(
            r#"
            SELECT
                keycloak_id,
                email,
                display_name,
                first_name,
                last_name
            FROM users
            WHERE LOWER(email) = LOWER($1)
            "#,
        )
        .bind(&normalized_email)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::Internal(format!("Failed to load user by email: {}", error)))?;

        let Some(target_user_row) = maybe_target_user else {
            continue;
        };

        let target_user_keycloak_id: String = target_user_row.get("keycloak_id");
        let target_user_email: String = target_user_row.get("email");
        let target_display_name_raw: Option<String> = target_user_row.get("display_name");
        let target_first_name: Option<String> = target_user_row.get("first_name");
        let target_last_name: Option<String> = target_user_row.get("last_name");

        let target_display_name = build_display_name(
            target_display_name_raw,
            target_first_name,
            target_last_name,
            &target_user_email,
        );

        let existing_pending_invitation = sqlx::query(
            r#"
            SELECT 1
            FROM group_invitations
            WHERE group_id = $1
              AND invited_user_keycloak_id = $2
              AND status = 'pending'
            "#,
        )
        .bind(group_id)
        .bind(&target_user_keycloak_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::Internal(format!("Failed to verify existing invitation: {}", error)))?;

        if existing_pending_invitation.is_some() {
            continue;
        }

        create_group_invitation_and_notification(
            &mut tx,
            group_id,
            &group_name,
            &target_user_keycloak_id,
            &target_display_name,
            &target_user_email,
            &user.keycloak_id,
            &user.display_name,
        )
        .await?;
    }

    tx.commit()
        .await
        .map_err(|error| AppError::Internal(format!("Failed to commit transaction: {}", error)))?;

    Ok(Json(CreateGroupResponse {
        group_id: group_id.to_string(),
        name: group_name,
        message: "Group created successfully".to_string(),
    }))
}

// Liste les groupes où l'utilisateur courant est membre.
pub async fn list_groups(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<GroupsListResponse>> {
    let rows = sqlx::query(
        r#"
        SELECT
            g.group_id,
            g.name,
            g.profile_photo_url,
            (
                SELECT COUNT(*)
                FROM group_members gm2
                WHERE gm2.group_id = g.group_id
            ) AS member_count,
            gm.role::text AS role
        FROM groups g
        INNER JOIN group_members gm
            ON gm.group_id = g.group_id
        WHERE gm.user_keycloak_id = $1
        ORDER BY g.name ASC
        "#,
    )
    .bind(&user.keycloak_id)
    .fetch_all(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to list groups: {}", error)))?;

    let groups = rows
        .into_iter()
        .map(|row| {
            let group_id: Uuid = row.get("group_id");
            let name: String = row.get("name");
            let profile_photo_url: Option<String> = row.get("profile_photo_url");
            let member_count: i64 = row.get("member_count");
            let my_role: String = row.get("role");

            GroupListItem {
                group_id: group_id.to_string(),
                name,
                profile_photo_url,
                member_count,
                my_role,
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(GroupsListResponse { groups }))
}

// Détail d'un groupe + ses membres + invitations pending.
//
// Sécurité :
// - l'utilisateur courant doit être membre du groupe,
// - les invitations pending ne sont visibles que par owner/admin.
pub async fn get_group_detail(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(group_id): Path<String>,
) -> AppResult<Json<GroupDetailResponse>> {
    let parsed_group_id = Uuid::parse_str(&group_id)
        .map_err(|_| AppError::BadRequest("Invalid group_id".to_string()))?;

    let maybe_group_row = sqlx::query(
        r#"
        SELECT
            g.group_id,
            g.name,
            g.description,
            g.profile_photo_url,
            gm.role::text AS role,
            (
                SELECT COUNT(*)
                FROM group_members gm2
                WHERE gm2.group_id = g.group_id
            ) AS member_count
        FROM groups g
        INNER JOIN group_members gm
            ON gm.group_id = g.group_id
        WHERE
            g.group_id = $1
            AND gm.user_keycloak_id = $2
        "#,
    )
    .bind(parsed_group_id)
    .bind(&user.keycloak_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to load group detail: {}", error)))?;

    let Some(group_row) = maybe_group_row else {
        return Err(AppError::BadRequest(
            "Group not found or access denied".to_string(),
        ));
    };

    let group_id_value: Uuid = group_row.get("group_id");
    let name: String = group_row.get("name");
    let description: Option<String> = group_row.get("description");
    let profile_photo_url: Option<String> = group_row.get("profile_photo_url");
    let my_role: String = group_row.get("role");
    let member_count: i64 = group_row.get("member_count");

    let member_rows = sqlx::query(
        r#"
        SELECT
            u.keycloak_id,
            u.email,
            u.display_name,
            u.first_name,
            u.last_name,
            gm.role::text AS role
        FROM group_members gm
        INNER JOIN users u
            ON u.keycloak_id = gm.user_keycloak_id
        WHERE gm.group_id = $1
        ORDER BY
            CASE WHEN gm.role = 'owner' THEN 0 ELSE 1 END,
            u.display_name ASC,
            u.email ASC
        "#,
    )
    .bind(group_id_value)
    .fetch_all(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to load group members: {}", error)))?;

    let members = member_rows
        .into_iter()
        .map(|row| {
            let user_keycloak_id: String = row.get("keycloak_id");
            let email: String = row.get("email");
            let display_name_raw: Option<String> = row.get("display_name");
            let first_name: Option<String> = row.get("first_name");
            let last_name: Option<String> = row.get("last_name");
            let role: String = row.get("role");

            let display_name = build_display_name(display_name_raw, first_name, last_name, &email);

            GroupMemberItem {
                user_keycloak_id,
                display_name,
                email,
                role,
            }
        })
        .collect::<Vec<_>>();

    let pending_invitations = if my_role == "owner" || my_role == "admin" {
        let invitation_rows = sqlx::query(
            r#"
            SELECT
                gi.group_invitation_id,
                gi.group_id,
                gi.invited_user_keycloak_id,
                gi.invited_by_user_keycloak_id,
                gi.status::text AS status,
                gi.created_at,
                gi.responded_at,
                gi.cancelled_at,
                u.display_name,
                u.first_name,
                u.last_name,
                u.email
            FROM group_invitations gi
            INNER JOIN users u
                ON u.keycloak_id = gi.invited_user_keycloak_id
            WHERE gi.group_id = $1
              AND gi.status = 'pending'
            ORDER BY gi.created_at DESC
            "#,
        )
        .bind(group_id_value)
        .fetch_all(&state.db)
        .await
        .map_err(|error| {
            AppError::Internal(format!("Failed to load group invitations: {}", error))
        })?;

        invitation_rows
            .into_iter()
            .map(|row| {
                let group_invitation_id: Uuid = row.get("group_invitation_id");
                let invitation_group_id: Uuid = row.get("group_id");
                let invited_user_keycloak_id: String = row.get("invited_user_keycloak_id");
                let invited_by_user_keycloak_id: String = row.get("invited_by_user_keycloak_id");
                let status: String = row.get("status");
                let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
                let responded_at: Option<chrono::DateTime<chrono::Utc>> = row.get("responded_at");
                let cancelled_at: Option<chrono::DateTime<chrono::Utc>> = row.get("cancelled_at");

                let display_name_raw: Option<String> = row.get("display_name");
                let first_name: Option<String> = row.get("first_name");
                let last_name: Option<String> = row.get("last_name");
                let email: String = row.get("email");

                let invited_user_display_name =
                    build_display_name(display_name_raw, first_name, last_name, &email);

                GroupInvitationItem {
                    group_invitation_id: group_invitation_id.to_string(),
                    group_id: invitation_group_id.to_string(),
                    invited_user_keycloak_id,
                    invited_user_display_name,
                    invited_user_email: email,
                    invited_by_user_keycloak_id,
                    status,
                    created_at: created_at.to_rfc3339(),
                    responded_at: responded_at.map(|dt| dt.to_rfc3339()),
                    cancelled_at: cancelled_at.map(|dt| dt.to_rfc3339()),
                }
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    Ok(Json(GroupDetailResponse {
        group: GroupDetailInfo {
            group_id: group_id_value.to_string(),
            name,
            description,
            profile_photo_url,
            member_count,
        },
        members,
        pending_invitations,
        my_role,
        current_user_keycloak_id: user.keycloak_id,
    }))
}

// Invite un utilisateur dans un groupe.
//
// Sécurité :
// - seul owner/admin peut inviter,
// - l'utilisateur ciblé doit exister,
// - il ne doit pas déjà être membre,
// - il ne doit pas déjà avoir une invitation pending.
pub async fn invite_group_member(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(group_id): Path<String>,
    Json(payload): Json<InviteGroupMemberRequest>,
) -> AppResult<Json<GroupInvitationActionResponse>> {
    payload.validate()?;

    let parsed_group_id = Uuid::parse_str(&group_id)
        .map_err(|_| AppError::BadRequest("Invalid group_id".to_string()))?;

    let Some(my_role) = load_group_role(&state.db, parsed_group_id, &user.keycloak_id).await? else {
        return Err(AppError::BadRequest(
            "You are not a member of this group".to_string(),
        ));
    };

    if my_role != "owner" && my_role != "admin" {
        return Err(AppError::BadRequest(
            "Only the group owner or an admin can invite members".to_string(),
        ));
    }

    let normalized_email = payload.email.trim().to_lowercase();

    let maybe_target_user = sqlx::query(
        r#"
        SELECT
            keycloak_id,
            email,
            display_name,
            first_name,
            last_name
        FROM users
        WHERE LOWER(email) = LOWER($1)
        "#,
    )
    .bind(&normalized_email)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to load target user: {}", error)))?;

    let Some(target_user_row) = maybe_target_user else {
        return Err(AppError::BadRequest(
            "No user found for this email".to_string(),
        ));
    };

    let target_user_keycloak_id: String = target_user_row.get("keycloak_id");
    let target_user_email: String = target_user_row.get("email");
    let target_display_name = build_display_name(
        target_user_row.get("display_name"),
        target_user_row.get("first_name"),
        target_user_row.get("last_name"),
        &target_user_email,
    );

    if target_user_keycloak_id == user.keycloak_id {
        return Err(AppError::BadRequest(
            "You cannot invite yourself".to_string(),
        ));
    }

    let existing_member = sqlx::query(
        r#"
        SELECT 1
        FROM group_members
        WHERE group_id = $1
          AND user_keycloak_id = $2
        "#,
    )
    .bind(parsed_group_id)
    .bind(&target_user_keycloak_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        AppError::Internal(format!("Failed to verify existing group membership: {}", error))
    })?;

    if existing_member.is_some() {
        return Err(AppError::BadRequest(
            "This user is already a member of the group".to_string(),
        ));
    }

    let existing_pending_invitation = sqlx::query(
        r#"
        SELECT 1
        FROM group_invitations
        WHERE group_id = $1
          AND invited_user_keycloak_id = $2
          AND status = 'pending'
        "#,
    )
    .bind(parsed_group_id)
    .bind(&target_user_keycloak_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        AppError::Internal(format!("Failed to verify existing invitation: {}", error))
    })?;

    if existing_pending_invitation.is_some() {
        return Err(AppError::BadRequest(
            "A pending invitation already exists for this user".to_string(),
        ));
    }

    let group_name = load_group_name(&state.db, parsed_group_id).await?;

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|error| AppError::Internal(format!("Failed to begin transaction: {}", error)))?;

    create_group_invitation_and_notification(
        &mut tx,
        parsed_group_id,
        &group_name,
        &target_user_keycloak_id,
        &target_display_name,
        &target_user_email,
        &user.keycloak_id,
        &user.display_name,
    )
    .await?;

    tx.commit()
        .await
        .map_err(|error| AppError::Internal(format!("Failed to commit transaction: {}", error)))?;

    Ok(Json(GroupInvitationActionResponse {
        message: format!("Invitation sent to {}", target_display_name),
    }))
}

// Réponse à une invitation de groupe.
//
// action = "accept" | "decline"
pub async fn respond_to_group_invitation(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(group_invitation_id): Path<String>,
    Json(payload): Json<RespondToGroupInvitationRequest>,
) -> AppResult<Json<GroupInvitationActionResponse>> {
    let parsed_invitation_id = Uuid::parse_str(&group_invitation_id)
        .map_err(|_| AppError::BadRequest("Invalid group_invitation_id".to_string()))?;

    let action = payload.action.trim().to_lowercase();

    if action != "accept" && action != "decline" {
        return Err(AppError::BadRequest(
            "Action must be 'accept' or 'decline'".to_string(),
        ));
    }

    let invitation_row = sqlx::query(
        r#"
        SELECT
            gi.group_invitation_id,
            gi.group_id,
            gi.invited_user_keycloak_id,
            gi.invited_by_user_keycloak_id,
            gi.status::text AS status,
            g.name AS group_name
        FROM group_invitations gi
        INNER JOIN groups g
            ON g.group_id = gi.group_id
        WHERE gi.group_invitation_id = $1
        "#,
    )
    .bind(parsed_invitation_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to load invitation: {}", error)))?;

    let Some(invitation_row) = invitation_row else {
        return Err(AppError::BadRequest("Invitation not found".to_string()));
    };

    let group_id: Uuid = invitation_row.get("group_id");
    let invited_user_keycloak_id: String = invitation_row.get("invited_user_keycloak_id");
    let invited_by_user_keycloak_id: String = invitation_row.get("invited_by_user_keycloak_id");
    let current_status: String = invitation_row.get("status");
    let group_name: String = invitation_row.get("group_name");

    if invited_user_keycloak_id != user.keycloak_id {
        return Err(AppError::BadRequest(
            "You are not allowed to respond to this invitation".to_string(),
        ));
    }

    if current_status != "pending" {
        return Err(AppError::BadRequest(
            "This invitation is no longer valid".to_string(),
        ));
    }

    let now = Utc::now();

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|error| AppError::Internal(format!("Failed to begin transaction: {}", error)))?;

    if action == "accept" {
        let existing_member = sqlx::query(
            r#"
            SELECT 1
            FROM group_members
            WHERE group_id = $1
              AND user_keycloak_id = $2
            "#,
        )
        .bind(group_id)
        .bind(&user.keycloak_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| {
            AppError::Internal(format!("Failed to verify existing membership: {}", error))
        })?;

        if existing_member.is_none() {
            let group_member_id = Uuid::new_v4();

            sqlx::query(
                r#"
                INSERT INTO group_members (
                    group_member_id,
                    group_id,
                    user_keycloak_id,
                    role,
                    joined_at
                )
                VALUES ($1, $2, $3, $4::group_member_role, $5)
                "#,
            )
            .bind(group_member_id)
            .bind(group_id)
            .bind(&user.keycloak_id)
            .bind("member")
            .bind(now.naive_utc())
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                AppError::Internal(format!("Failed to add accepted member to group: {}", error))
            })?;
        }

        sqlx::query(
            r#"
            UPDATE group_invitations
            SET status = 'accepted',
                responded_at = $1
            WHERE group_invitation_id = $2
            "#,
        )
        .bind(now)
        .bind(parsed_invitation_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            AppError::Internal(format!("Failed to accept group invitation: {}", error))
        })?;

        sqlx::query(
            r#"
            UPDATE notifications
            SET action_status = 'accepted',
                is_read = TRUE
            WHERE related_group_invitation_id = $1
            "#,
        )
        .bind(parsed_invitation_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            AppError::Internal(format!("Failed to update notification status: {}", error))
        })?;

        // Notification envoyée à l'owner/admin pour signaler l'acceptation.
        sqlx::query(
            r#"
            INSERT INTO notifications (
                notification_id,
                user_keycloak_id,
                type,
                title,
                message,
                is_read,
                created_at,
                notification_type,
                related_group_id,
                related_group_invitation_id,
                action_status
            )
            VALUES ($1, $2, $3, $4, $5, FALSE, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(&invited_by_user_keycloak_id)
        .bind("group_invitation_response")
        .bind(format!("Invitation acceptée - {}", group_name))
        .bind(format!("{} a accepté de rejoindre le groupe {}", user.display_name, group_name))
        .bind(now.naive_utc())
        .bind("group_invitation_response")
        .bind(group_id)
        .bind(parsed_invitation_id)
        .bind("accepted")
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            AppError::Internal(format!("Failed to create acceptance notification: {}", error))
        })?;

        tx.commit()
            .await
            .map_err(|error| AppError::Internal(format!("Failed to commit transaction: {}", error)))?;

        return Ok(Json(GroupInvitationActionResponse {
            message: "Group invitation accepted".to_string(),
        }));
    }

    sqlx::query(
        r#"
        UPDATE group_invitations
        SET status = 'declined',
            responded_at = $1
        WHERE group_invitation_id = $2
        "#,
    )
    .bind(now)
    .bind(parsed_invitation_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        AppError::Internal(format!("Failed to decline group invitation: {}", error))
    })?;

    sqlx::query(
        r#"
        UPDATE notifications
        SET action_status = 'declined',
            is_read = TRUE
        WHERE related_group_invitation_id = $1
        "#,
    )
    .bind(parsed_invitation_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        AppError::Internal(format!("Failed to update notification status: {}", error))
    })?;

    // Notification envoyée à l'owner/admin pour signaler le refus.
    sqlx::query(
        r#"
        INSERT INTO notifications (
            notification_id,
            user_keycloak_id,
            type,
            title,
            message,
            is_read,
            created_at,
            notification_type,
            related_group_id,
            related_group_invitation_id,
            action_status
        )
        VALUES ($1, $2, $3, $4, $5, FALSE, $6, $7, $8, $9, $10)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(&invited_by_user_keycloak_id)
    .bind("group_invitation_response")
    .bind(format!("Invitation refusée - {}", group_name))
    .bind(format!("{} a refusé de rejoindre le groupe {}", user.display_name, group_name))
    .bind(now.naive_utc())
    .bind("group_invitation_response")
    .bind(group_id)
    .bind(parsed_invitation_id)
    .bind("declined")
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        AppError::Internal(format!("Failed to create decline notification: {}", error))
    })?;

    tx.commit()
        .await
        .map_err(|error| AppError::Internal(format!("Failed to commit transaction: {}", error)))?;

    Ok(Json(GroupInvitationActionResponse {
        message: "Group invitation declined".to_string(),
    }))
}

// Annule une invitation pending.
//
// Sécurité :
// - seul owner/admin peut annuler,
// - l'invitation doit encore être pending.
pub async fn cancel_group_invitation(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((group_id, group_invitation_id)): Path<(String, String)>,
) -> AppResult<Json<GroupInvitationActionResponse>> {
    let parsed_group_id = Uuid::parse_str(&group_id)
        .map_err(|_| AppError::BadRequest("Invalid group_id".to_string()))?;

    let parsed_invitation_id = Uuid::parse_str(&group_invitation_id)
        .map_err(|_| AppError::BadRequest("Invalid group_invitation_id".to_string()))?;

    let Some(my_role) = load_group_role(&state.db, parsed_group_id, &user.keycloak_id).await? else {
        return Err(AppError::BadRequest(
            "You are not a member of this group".to_string(),
        ));
    };

    if my_role != "owner" && my_role != "admin" {
        return Err(AppError::BadRequest(
            "Only the group owner or an admin can cancel invitations".to_string(),
        ));
    }

    let invitation_row = sqlx::query(
        r#"
        SELECT status::text AS status
        FROM group_invitations
        WHERE group_invitation_id = $1
          AND group_id = $2
        "#,
    )
    .bind(parsed_invitation_id)
    .bind(parsed_group_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to load invitation: {}", error)))?;

    let Some(invitation_row) = invitation_row else {
        return Err(AppError::BadRequest("Invitation not found".to_string()));
    };

    let status: String = invitation_row.get("status");

    if status != "pending" {
        return Err(AppError::BadRequest(
            "Only pending invitations can be cancelled".to_string(),
        ));
    }

    let now = Utc::now();

    sqlx::query(
        r#"
        UPDATE group_invitations
        SET status = 'cancelled',
            cancelled_at = $1
        WHERE group_invitation_id = $2
        "#,
    )
    .bind(now)
    .bind(parsed_invitation_id)
    .execute(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to cancel invitation: {}", error)))?;

    sqlx::query(
        r#"
        UPDATE notifications
        SET action_status = 'cancelled'
        WHERE related_group_invitation_id = $1
        "#,
    )
    .bind(parsed_invitation_id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        AppError::Internal(format!("Failed to update notification status: {}", error))
    })?;

    Ok(Json(GroupInvitationActionResponse {
        message: "Invitation cancelled successfully".to_string(),
    }))
}

// Liste les invitations de groupe reçues par l'utilisateur connecté.
pub async fn list_my_group_invitations(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<GroupInvitationsListResponse>> {
    let rows = sqlx::query(
        r#"
        SELECT
            gi.group_invitation_id,
            gi.group_id,
            gi.invited_user_keycloak_id,
            gi.invited_by_user_keycloak_id,
            gi.status::text AS status,
            gi.created_at,
            gi.responded_at,
            gi.cancelled_at,
            u.display_name,
            u.first_name,
            u.last_name,
            u.email
        FROM group_invitations gi
        INNER JOIN users u
            ON u.keycloak_id = gi.invited_user_keycloak_id
        WHERE gi.invited_user_keycloak_id = $1
        ORDER BY gi.created_at DESC
        "#,
    )
    .bind(&user.keycloak_id)
    .fetch_all(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to list group invitations: {}", error)))?;

    let invitations = rows
        .into_iter()
        .map(|row| {
            let invitation_id: Uuid = row.get("group_invitation_id");
            let group_id: Uuid = row.get("group_id");
            let invited_user_keycloak_id: String = row.get("invited_user_keycloak_id");
            let invited_by_user_keycloak_id: String = row.get("invited_by_user_keycloak_id");
            let status: String = row.get("status");
            let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
            let responded_at: Option<chrono::DateTime<chrono::Utc>> = row.get("responded_at");
            let cancelled_at: Option<chrono::DateTime<chrono::Utc>> = row.get("cancelled_at");

            let email: String = row.get("email");
            let invited_user_display_name = build_display_name(
                row.get("display_name"),
                row.get("first_name"),
                row.get("last_name"),
                &email,
            );

            GroupInvitationItem {
                group_invitation_id: invitation_id.to_string(),
                group_id: group_id.to_string(),
                invited_user_keycloak_id,
                invited_user_display_name,
                invited_user_email: email,
                invited_by_user_keycloak_id,
                status,
                created_at: created_at.to_rfc3339(),
                responded_at: responded_at.map(|dt| dt.to_rfc3339()),
                cancelled_at: cancelled_at.map(|dt| dt.to_rfc3339()),
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(GroupInvitationsListResponse { invitations }))
}

// Retire un membre déjà accepté du groupe.
//
// Sécurité :
// - owner/admin seulement,
// - on ne retire pas l'owner via cette route.
pub async fn remove_group_member(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((group_id, user_keycloak_id)): Path<(String, String)>,
) -> AppResult<Json<GroupActionResponse>> {
    let parsed_group_id = Uuid::parse_str(&group_id)
        .map_err(|_| AppError::BadRequest("Invalid group_id".to_string()))?;

    let Some(my_role) = load_group_role(&state.db, parsed_group_id, &user.keycloak_id).await? else {
        return Err(AppError::BadRequest(
            "You are not a member of this group".to_string(),
        ));
    };

    if my_role != "owner" && my_role != "admin" {
        return Err(AppError::BadRequest(
            "Only the group owner or an admin can remove members".to_string(),
        ));
    }

    if user_keycloak_id == user.keycloak_id {
        return Err(AppError::BadRequest(
            "You cannot remove yourself from this route".to_string(),
        ));
    }

    let target_role_row = sqlx::query(
        r#"
        SELECT role::text AS role
        FROM group_members
        WHERE group_id = $1
          AND user_keycloak_id = $2
        "#,
    )
    .bind(parsed_group_id)
    .bind(&user_keycloak_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to load target role: {}", error)))?;

    let Some(target_role_row) = target_role_row else {
        return Err(AppError::BadRequest(
            "Member not found in this group".to_string(),
        ));
    };

    let target_role: String = target_role_row.get("role");

    if target_role == "owner" {
        return Err(AppError::BadRequest(
            "The owner cannot be removed".to_string(),
        ));
    }

    if my_role == "admin" && target_role == "admin" {
        return Err(AppError::BadRequest(
            "An admin cannot remove another admin".to_string(),
        ));
    }

    let delete_result = sqlx::query(
        r#"
        DELETE FROM group_members
        WHERE group_id = $1
          AND user_keycloak_id = $2
        "#,
    )
    .bind(parsed_group_id)
    .bind(&user_keycloak_id)
    .execute(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to remove group member: {}", error)))?;

    if delete_result.rows_affected() == 0 {
        return Err(AppError::BadRequest(
            "Member not found in this group".to_string(),
        ));
    }

    Ok(Json(GroupActionResponse {
        message: "Member removed successfully".to_string(),
    }))
}

// =========================
// User search
// =========================

// Recherche des utilisateurs TrueGather existants.
//
// Sécurité :
// - limitée,
// - minimum 2 caractères,
// - exclut l'utilisateur courant.
pub async fn search_users(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<SearchUsersQuery>,
) -> AppResult<Json<UserSearchResponse>> {
    let raw_query = query.q.unwrap_or_default();
    let search_term = raw_query.trim();

    if search_term.len() < 2 {
        return Ok(Json(UserSearchResponse { users: vec![] }));
    }

    let like_value = format!("%{}%", search_term);

    let rows = sqlx::query(
        r#"
        SELECT
            keycloak_id,
            display_name,
            first_name,
            last_name,
            email
        FROM users
        WHERE
            is_active = TRUE
            AND keycloak_id <> $1
            AND (
                LOWER(email) LIKE LOWER($2)
                OR LOWER(display_name) LIKE LOWER($2)
                OR LOWER(first_name) LIKE LOWER($2)
                OR LOWER(last_name) LIKE LOWER($2)
            )
        ORDER BY
            display_name ASC NULLS LAST,
            email ASC
        LIMIT 8
        "#,
    )
    .bind(&user.keycloak_id)
    .bind(&like_value)
    .fetch_all(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to search users: {}", error)))?;

    let users = rows
        .into_iter()
        .map(|row| {
            let keycloak_id: String = row.get("keycloak_id");
            let email: String = row.get("email");

            let display_name = build_display_name(
                row.get("display_name"),
                row.get("first_name"),
                row.get("last_name"),
                &email,
            );

            UserSearchItem {
                keycloak_id,
                display_name,
                first_name: row.get("first_name"),
                last_name: row.get("last_name"),
                email,
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(UserSearchResponse { users }))
}

// =========================
// Group photo
// =========================

pub async fn upload_group_photo(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(group_id): Path<String>,
    mut multipart: Multipart,
) -> AppResult<Json<GroupActionResponse>> {
    let parsed_group_id = Uuid::parse_str(&group_id)
        .map_err(|_| AppError::BadRequest("Invalid group_id".to_string()))?;

    let Some(role) = load_group_role(&state.db, parsed_group_id, &user.keycloak_id).await? else {
        return Err(AppError::BadRequest("Not member".to_string()));
    };

    if role != "owner" {
        return Err(AppError::BadRequest(
            "Only owner can upload photo".to_string(),
        ));
    }

    let field = multipart
        .next_field()
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?
        .ok_or_else(|| AppError::BadRequest("No file".to_string()))?;

    let data = field
        .bytes()
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?;

    let filename = format!("{}.jpg", parsed_group_id);

    let upload_dir = "uploads/groups";
    tokio::fs::create_dir_all(upload_dir)
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?;

    let filepath = format!("{}/{}", upload_dir, filename);

    tokio::fs::write(&filepath, &data)
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?;

    let public_url = format!("http://localhost:8080/uploads/groups/{}", filename);

    sqlx::query(
        r#"
        UPDATE groups
        SET profile_photo_url = $1,
            updated_at = $2
        WHERE group_id = $3
        "#,
    )
    .bind(&public_url)
    .bind(Utc::now().naive_utc())
    .bind(parsed_group_id)
    .execute(&state.db)
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?;

    Ok(Json(GroupActionResponse {
        message: "Photo uploaded".to_string(),
    }))
}

// =========================
// Delete group
// =========================

pub async fn delete_group(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(group_id): Path<String>,
) -> AppResult<Json<GroupActionResponse>> {
    let parsed_group_id = Uuid::parse_str(&group_id)
        .map_err(|_| AppError::BadRequest("Invalid group_id".to_string()))?;

    let owner_row = sqlx::query(
        r#"
        SELECT owner_keycloak_id, profile_photo_url
        FROM groups
        WHERE group_id = $1
        "#,
    )
    .bind(parsed_group_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to load group: {}", error)))?;

    let Some(owner_row) = owner_row else {
        return Err(AppError::BadRequest("Group not found".to_string()));
    };

    let owner_keycloak_id: String = owner_row.get("owner_keycloak_id");
    let profile_photo_url: Option<String> = owner_row.get("profile_photo_url");

    if owner_keycloak_id != user.keycloak_id {
        return Err(AppError::BadRequest(
            "Only the group owner can delete the group".to_string(),
        ));
    }

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|error| AppError::Internal(format!("Failed to begin transaction: {}", error)))?;

    sqlx::query(
        r#"
        DELETE FROM group_members
        WHERE group_id = $1
        "#,
    )
    .bind(parsed_group_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to delete group members: {}", error)))?;

    sqlx::query(
        r#"
        DELETE FROM group_invitations
        WHERE group_id = $1
        "#,
    )
    .bind(parsed_group_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to delete group invitations: {}", error)))?;

    sqlx::query(
        r#"
        DELETE FROM notifications
        WHERE related_group_id = $1
        "#,
    )
    .bind(parsed_group_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to delete group notifications: {}", error)))?;

    sqlx::query(
        r#"
        DELETE FROM groups
        WHERE group_id = $1
        "#,
    )
    .bind(parsed_group_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to delete group: {}", error)))?;

    tx.commit()
        .await
        .map_err(|error| AppError::Internal(format!("Failed to commit transaction: {}", error)))?;

    if let Some(photo_url) = profile_photo_url {
        let path_fragment = photo_url
            .strip_prefix("http://localhost:8080")
            .unwrap_or(&photo_url);

        let relative_path = path_fragment.trim_start_matches('/');
        let _ = tokio::fs::remove_file(relative_path).await;
    }

    Ok(Json(GroupActionResponse {
        message: "Group deleted successfully".to_string(),
    }))
}

// =========================
// Notifications
// =========================

// Marque toutes les notifications du user connecté comme lues.
pub async fn mark_all_notifications_as_read(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<GroupActionResponse>> {
    sqlx::query(
        r#"
        UPDATE notifications
        SET is_read = TRUE
        WHERE user_keycloak_id = $1
          AND is_read = FALSE
        "#,
    )
    .bind(&user.keycloak_id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        AppError::Internal(format!("Failed to mark all notifications as read: {}", error))
    })?;

    Ok(Json(GroupActionResponse {
        message: "All notifications marked as read".to_string(),
    }))
}

// Marque une notification précise comme lue.
pub async fn mark_notification_as_read(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(notification_id): Path<String>,
) -> AppResult<Json<GroupActionResponse>> {
    let parsed_notification_id = Uuid::parse_str(&notification_id)
        .map_err(|_| AppError::BadRequest("Invalid notification_id".to_string()))?;

    let update_result = sqlx::query(
        r#"
        UPDATE notifications
        SET is_read = TRUE
        WHERE notification_id = $1
          AND user_keycloak_id = $2
        "#,
    )
    .bind(parsed_notification_id)
    .bind(&user.keycloak_id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        AppError::Internal(format!("Failed to mark notification as read: {}", error))
    })?;

    if update_result.rows_affected() == 0 {
        return Err(AppError::BadRequest(
            "Notification not found".to_string(),
        ));
    }

    Ok(Json(GroupActionResponse {
        message: "Notification marked as read".to_string(),
    }))
}
