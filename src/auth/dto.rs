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

    #[validate(length(
        min = 8,
        message = "Password confirmation must contain at least 8 characters"
    ))]
    pub confirm_password: String,
}

// Vue de session pour le frontend.

#[derive(Debug, Serialize)]
pub struct SessionSnapshotResponse {
    pub authenticated: bool,
    pub user: Option<crate::models::UserProfileView>,
}

// Payload de changement de mot de passe (utilisateur connecté).
#[derive(Debug, Deserialize, Validate)]
pub struct ChangePasswordRequest {
    #[validate(length(min = 1, message = "Le mot de passe actuel est requis"))]
    pub current_password: String,

    #[validate(length(
        min = 14,
        message = "Le nouveau mot de passe doit contenir au moins 14 caractères"
    ))]
    pub new_password: String,

    #[validate(length(min = 1, message = "La confirmation du mot de passe est requise"))]
    pub confirm_password: String,
}

// Payload de mise à jour du profil (prénom et/ou nom de famille).
// Au moins un des deux champs doit être fourni — la validation est faite dans le handler.
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateProfileRequest {
    #[validate(length(max = 64, message = "Le prénom ne peut pas dépasser 64 caractères"))]
    pub first_name: Option<String>,

    #[validate(length(max = 64, message = "Le nom ne peut pas dépasser 64 caractères"))]
    pub last_name: Option<String>,
}

// Query string du callback de vérification d'email.
#[derive(Debug, Deserialize)]
pub struct VerifyEmailQuery {
    pub token: Option<String>,
}
