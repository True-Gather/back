use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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