use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Serialize)]
pub struct GroupInvitationItem {
    pub group_invitation_id: String,
    pub group_id: String,
    pub invited_user_keycloak_id: String,
    pub invited_user_display_name: String,
    pub invited_user_email: String,
    pub invited_by_user_keycloak_id: String,
    pub status: String,
    pub created_at: String,
    pub responded_at: Option<String>,
    pub cancelled_at: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct InviteGroupMemberRequest {
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct RespondToGroupInvitationRequest {
    pub action: String, // "accept" | "decline"
}

#[derive(Debug, Serialize)]
pub struct GroupInvitationActionResponse {
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct GroupInvitationsListResponse {
    pub invitations: Vec<GroupInvitationItem>,
}