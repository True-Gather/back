// DTO du module auth.

use serde::{Deserialize, Serialize};
use validator::Validate;

// Query string reçue sur le callback OIDC.
#[derive(Debug, Deserialize)]
pub struct AuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
    pub session_state: Option<String>,
}

// Réponse JSON simple pour les flows login/register placeholder.
#[derive(Debug, Serialize)]
pub struct AuthFlowStartResponse {
    pub message: String,
    pub provider: String,
    pub client_id: String,
    pub callback_url: String,
    pub frontend_success_url: String,
}

// Réponse d'information simple.
#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub message: String,
}

// Payload de mot de passe oublié.
#[derive(Debug, Deserialize, Validate)]
pub struct ForgotPasswordRequest {
    #[validate(email(message = "A valid email address is required"))]
    pub email: String,
}

// Payload de reset de mot de passe.
#[derive(Debug, Deserialize, Validate)]
pub struct ResetPasswordRequest {
    #[validate(length(min = 1, message = "Reset token is required"))]
    pub token: String,

    #[validate(length(min = 8, message = "Password must contain at least 8 characters"))]
    pub new_password: String,

    #[validate(length(min = 8, message = "Password confirmation must contain at least 8 characters"))]
    pub confirm_password: String,
}

// Vue de session pour le frontend.

#[derive(Debug, Serialize)]
pub struct SessionSnapshotResponse {
    pub authenticated: bool,
    pub user: Option<crate::models::UserProfileView>,
}