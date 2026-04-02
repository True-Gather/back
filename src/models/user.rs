// Modèles liés aux utilisateurs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// Représentation interne d'un utilisateur applicatif.
//
// Cette structure est alignée avec la table `users` PostgreSQL.
// L'identifiant principal est l'identifiant stable Keycloak.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub keycloak_id: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub display_name: String,
    pub email: String,
    pub profile_photo_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

// Vue utilisateur simplifiée envoyée au frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfileView {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}