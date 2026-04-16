// Modèles liés aux groupes.
//
// Cette version couvre :
// - création de groupe,
// - listing des groupes du user connecté,
// - détail d'un groupe,
// - membres acceptés,
// - invitations en attente,
// - actions génériques sur les groupes.
//
// Important :
// le frontend ne choisit jamais l'owner.
// L'owner est toujours déduit côté backend via la session.

use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::models::GroupInvitationItem;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateGroupRequest {
    #[validate(length(min = 2, max = 100))]
    pub name: String,

    #[validate(length(max = 500))]
    pub description: Option<String>,

    // Emails proposés à la création.
    // Dans la nouvelle logique, ils ne sont plus ajoutés directement
    // comme membres définitifs : le backend peut créer des invitations.
    #[serde(default)]
    pub member_emails: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateGroupResponse {
    pub group_id: String,
    pub name: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct GroupsListResponse {
    pub groups: Vec<GroupListItem>,
}

#[derive(Debug, Serialize)]
pub struct GroupListItem {
    pub group_id: String,
    pub name: String,
    pub profile_photo_url: Option<String>,
    pub member_count: i64,
    pub my_role: String,
}

#[derive(Debug, Serialize)]
pub struct GroupDetailResponse {
    pub group: GroupDetailInfo,

    // Membres déjà acceptés / présents dans le groupe.
    pub members: Vec<GroupMemberItem>,

    // Invitations encore en attente.
    pub pending_invitations: Vec<GroupInvitationItem>,

    pub my_role: String,
    pub current_user_keycloak_id: String,
}

#[derive(Debug, Serialize)]
pub struct GroupDetailInfo {
    pub group_id: String,
    pub name: String,
    pub description: Option<String>,
    pub profile_photo_url: Option<String>,
    pub member_count: i64,
}

#[derive(Debug, Serialize)]
pub struct GroupMemberItem {
    pub user_keycloak_id: String,
    pub display_name: String,
    pub email: String,
    pub role: String,
}

// Réponse simple réutilisable pour des actions groupe / invitation.
#[derive(Debug, Serialize)]
pub struct GroupActionResponse {
    pub message: String,
}