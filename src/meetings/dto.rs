use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Requête meeting instantané ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateInstantMeetingRequest {
    pub title: String,
    pub participant_emails: Vec<String>,
    pub group_id: Option<Uuid>,
    pub ai_enabled: bool,
    pub microphone_enabled: bool,
    pub camera_enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct InstantMeetingResponse {
    pub meeting_id: Uuid,
    pub title: String,
    pub meeting_link: String,
    pub room_code: String,
    pub status: String,
    pub participants_count: usize,
}

// ── Requête de création ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateMeetingRequest {
    pub title: String,
    pub description: Option<String>,
    pub scheduled_start_at: DateTime<Utc>,
    pub scheduled_end_at: DateTime<Utc>,
    pub ai_enabled: bool,
    /// Emails des participants invités (résolus en keycloak_id côté back)
    pub participant_emails: Vec<String>,
    /// UUIDs des groupes invités
    pub group_ids: Vec<Uuid>,
}

// ── Détail d'un meeting (accès contrôlé) ─────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MeetingDetailResponse {
    pub meeting_id: Uuid,
    pub title: String,
    pub status: String,
    pub meeting_type: String,
    pub ai_enabled: bool,
    pub room_code: Option<String>,
    pub meeting_link: Option<String>,
    pub host_keycloak_id: String,
    pub participants: Vec<ParticipantResponse>,
    pub user_role: String,
}

// ── Réponse meeting ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MeetingResponse {
    pub meeting_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub host_keycloak_id: String,
    pub scheduled_start_at: DateTime<Utc>,
    pub scheduled_end_at: DateTime<Utc>,
    pub ai_enabled: bool,
    pub status: String,
    pub participants: Vec<ParticipantResponse>,
}

#[derive(Debug, Serialize)]
pub struct ParticipantResponse {
    pub keycloak_id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub role: String,
    pub status: String,
}