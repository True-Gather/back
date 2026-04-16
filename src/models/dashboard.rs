// Modèles de réponse pour le dashboard.
//
// Ces structures représentent la vue "frontend"
// du tableau de bord connecté.
//
// Important :
// même si la base est vide, on renvoie quand même
// une structure complète avec des tableaux vides.
// Cela évite de casser le frontend.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DashboardResponse {
    pub user: DashboardUser,
    pub stats: DashboardStats,
    pub recent_meetings: Vec<DashboardMeeting>,
    pub notifications: Vec<DashboardNotification>,
}

#[derive(Debug, Serialize)]
pub struct DashboardUser {
    pub id: String,
    pub email: String,
    pub display_name: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub total_meetings: i64,
    pub total_groups: i64,
    pub unread_notifications: i64,
}

#[derive(Debug, Serialize)]
pub struct DashboardMeeting {
    pub id: String,
    pub title: String,
    pub status: String,
    pub meeting_type: String,
    pub scheduled_start_at: Option<String>,
    pub scheduled_end_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DashboardNotification {
    pub id: String,
    pub title: String,
    pub message: String,
    pub is_read: bool,
    pub created_at: String,

    // Type de notification :
    // ex: "generic", "group_invitation", "meeting_invitation", etc.
    pub notification_type: String,

    // Groupe éventuellement concerné.
    pub related_group_id: Option<String>,

    // Invitation de groupe éventuellement concernée.
    pub related_group_invitation_id: Option<String>,

    // Etat de l’action si la notif est actionnable :
    // ex: "pending", "accepted", "declined", "cancelled".
    pub action_status: Option<String>,
}