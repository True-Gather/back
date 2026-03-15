// Handlers auth.
use axum::{
    extract::{Query, State},
    Json,
};

// Imports internes.
use crate::{
    auth::dto::{
        AuthCallbackQuery,
        AuthFlowStartResponse,
        ForgotPasswordRequest,
        MessageResponse,
        ResetPasswordRequest,
        SessionSnapshotResponse,
    },
    error::{AppError, AppResult},
    state::AppState,
};
use validator::Validate;

// Démarre le flow de login.
pub async fn start_login(State(state): State<AppState>) -> AppResult<Json<AuthFlowStartResponse>> {
    Ok(Json(AuthFlowStartResponse {
        message: "Login flow placeholder ready for future Keycloak redirection".to_string(),
        provider: "keycloak".to_string(),
        client_id: state.config.keycloak.client_id.clone(),
        callback_url: state.config.auth_callback_url(),
        frontend_success_url: state.config.frontend_post_login_url(),
    }))
}

// Démarre le flow d'inscription.
pub async fn start_register(
    State(state): State<AppState>,
) -> AppResult<Json<AuthFlowStartResponse>> {
    Ok(Json(AuthFlowStartResponse {
        message: "Register flow placeholder ready for future Keycloak redirection".to_string(),
        provider: "keycloak".to_string(),
        client_id: state.config.keycloak.client_id.clone(),
        callback_url: state.config.auth_callback_url(),
        frontend_success_url: state.config.frontend_post_login_url(),
    }))
}

// Callback OIDC futur.
pub async fn auth_callback(
    State(_state): State<AppState>,
    Query(query): Query<AuthCallbackQuery>,
) -> AppResult<Json<MessageResponse>> {
    if let Some(error) = query.error {
        let description = query
            .error_description
            .unwrap_or_else(|| "No error description provided".to_string());

        return Err(AppError::BadRequest(format!(
            "OIDC callback returned error: {} ({})",
            error, description
        )));
    }

    // Validation minimale des paramètres attendus.
    let code = query
        .code
        .ok_or_else(|| AppError::BadRequest("Missing authorization code".to_string()))?;

    let state = query
        .state
        .ok_or_else(|| AppError::BadRequest("Missing state parameter".to_string()))?;

    let _ = (code, state, query.session_state);

    Ok(Json(MessageResponse {
        message: "OIDC callback received. Token exchange and user synchronization are not wired yet".to_string(),
    }))
}

// Retourne l'état de session courant.
pub async fn me(State(_state): State<AppState>) -> AppResult<Json<SessionSnapshotResponse>> {
    Ok(Json(SessionSnapshotResponse {
        authenticated: false,
        user: None,
    }))
}

// Déconnexion applicative.
pub async fn logout(State(_state): State<AppState>) -> AppResult<Json<MessageResponse>> {
    Ok(Json(MessageResponse {
        message: "Logout placeholder. App session invalidation will be added later".to_string(),
    }))
}

// Forgot password.
pub async fn forgot_password(
    State(_state): State<AppState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> AppResult<Json<MessageResponse>> {
    // Validation du payload.
    payload.validate()?;

    // Réponse volontairement neutre pour éviter l'énumération de comptes.
    Ok(Json(MessageResponse {
        message: "If an account exists for this email, a reset flow will be triggered".to_string(),
    }))
}

// Reset password.
pub async fn reset_password(
    State(_state): State<AppState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> AppResult<Json<MessageResponse>> {
    // Validation structurelle du payload.
    payload.validate()?;

    // Vérification métier minimale.
    if payload.new_password != payload.confirm_password {
        return Err(AppError::Validation(
            "Password and confirmation do not match".to_string(),
        ));
    }

    // Réponse placeholder.
    Ok(Json(MessageResponse {
        message: "Password reset payload accepted by backend skeleton".to_string(),
    }))
}