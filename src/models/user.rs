// Modèles liés aux utilisateurs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Représentation interne d'un utilisateur applicatif.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub keycloak_sub: Option<String>,
    pub email: String,
    pub display_name: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

// Vue utilisateur simplifiée envoyée au frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfileView {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}