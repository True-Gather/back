// Handlers auth.
//
// Ce fichier contient :
// - le démarrage du flow login,
// - le démarrage du flow register,
// - le callback OIDC réel,
// - la lecture de session courante,
// - le logout applicatif + OIDC,
// - les placeholders forgot/reset password.

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Redirect, Response},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;

use chrono::{Duration, Utc};
use validator::Validate;

use crate::{
    auth::dto::{
        AuthCallbackQuery, ForgotPasswordRequest, MessageResponse, ResetPasswordRequest,
        SessionSnapshotResponse,
    },
    error::{AppError, AppResult},
    models::UserProfileView,
    state::AppState,
};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct IdTokenClaims {
    sub: String,
    exp: u64,
    iat: u64,
    iss: String,
    aud: String,
    nonce: Option<String>,
}

// Démarre le flow de login.
//
// Ce handler prépare une demande OIDC puis redirige le navigateur
// vers Keycloak.
pub async fn start_login(State(state): State<AppState>) -> AppResult<Redirect> {
    // Préparation du flow OIDC en mode login.
    let auth_request = crate::auth::oidc::prepare_authorization_redirect(&state, false).await;
    Ok(Redirect::to(&auth_request.authorization_url))
}

// Démarre le flow d'inscription.
//
// Ce handler prépare une demande OIDC puis redirige le navigateur
// vers Keycloak en orientant le flow vers le parcours d'inscription.
pub async fn start_register(State(state): State<AppState>) -> AppResult<Redirect> {
    // Préparation du flow OIDC en mode inscription.
    let auth_request = crate::auth::oidc::prepare_authorization_redirect(&state, true).await;
    Ok(Redirect::to(&auth_request.authorization_url))
}

// Callback OIDC réel.
//
// Ce handler :
// - lit code et state,
// - vérifie la présence du state stocké,
// - échange le code contre des tokens,
// - récupère le userinfo,
// - crée une session applicative,
// - pose un cookie HTTP-only,
// - redirige vers le frontend.
pub async fn auth_callback(
    State(state): State<AppState>,
    Query(query): Query<AuthCallbackQuery>,
) -> AppResult<Response> {
    if let Some(error) = query.error {
        let description = query
            .error_description
            .unwrap_or_else(|| "No error description provided".to_string());

        return Err(AppError::BadRequest(format!(
            "OIDC callback returned error: {} ({})",
            error, description
        )));
    }

    let code = query
        .code
        .ok_or_else(|| AppError::BadRequest("Missing authorization code".to_string()))?;

    let oauth_state = query
        .state
        .ok_or_else(|| AppError::BadRequest("Missing state parameter".to_string()))?;

    let pending_request = {
        let mut pending_auth = state.pending_auth.write().await;
        pending_auth.remove(&oauth_state)
    }
    .ok_or_else(|| AppError::BadRequest("Unknown or expired state parameter".to_string()))?;

    let max_age = Duration::minutes(5);

    if Utc::now() - pending_request.created_at > max_age {
        return Err(AppError::BadRequest(
            "Expired authentication request".to_string(),
        ));
    }

    // Exchange code → tokens
    let token_response =
        crate::auth::oidc::exchange_code_for_tokens(&state, &code, &pending_request.pkce_verifier)
            .await?;

    let id_token = token_response
        .id_token
        .ok_or_else(|| AppError::Internal("Missing id_token".to_string()))?;

    let parts: Vec<&str> = id_token.split('.').collect();

    if parts.len() != 3 {
        return Err(AppError::Internal("Invalid id_token format".to_string()));
    }

    let payload = parts[1];

    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| AppError::Internal("Failed to decode id_token".to_string()))?;

    let claims: IdTokenClaims = serde_json::from_slice(&decoded)
        .map_err(|_| AppError::Internal("Invalid id_token payload".to_string()))?;

    if claims.nonce.as_deref() != Some(&pending_request.nonce) {
        return Err(AppError::BadRequest("Invalid nonce".to_string()));
    }

    let now = Utc::now().timestamp() as u64;

    if claims.exp < now {
        return Err(AppError::BadRequest("Expired id_token".to_string()));
    }

    // Récupération userinfo
    let userinfo = crate::auth::oidc::fetch_userinfo(&state, &token_response.access_token).await?;

    // Sync user local
    let local_user = crate::auth::sync::sync_user_from_keycloak(&state, &userinfo).await?;

    // Création session
    let session_id = crate::auth::session::create_session(&state, &local_user, Some(id_token.clone())).await?;

    let cookie_value = crate::auth::session::build_session_cookie(
        &state.config.auth.cookie_name,
        &session_id,
        state.config.auth.cookie_secure,
    );

    // Redirection frontend
    let mut response = Redirect::to(&state.config.frontend_post_login_url()).into_response();

    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie_value)
            .map_err(|error| AppError::Internal(format!("Invalid Set-Cookie header: {}", error)))?,
    );

    Ok(response)
}

// Retourne l'état de session courant.
pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<SessionSnapshotResponse>> {
    let maybe_session_id = crate::auth::session::extract_session_id_from_headers(
        &headers,
        &state.config.auth.cookie_name,
    );

    let Some(session_id) = maybe_session_id else {
        return Ok(Json(SessionSnapshotResponse {
            authenticated: false,
            user: None,
        }));
    };

    let maybe_session = {
        let sessions = state.sessions.read().await;
        sessions.get(&session_id).cloned()
    };

    let Some(session) = maybe_session else {
        return Ok(Json(SessionSnapshotResponse {
            authenticated: false,
            user: None,
        }));
    };

    let user = UserProfileView {
        id: session.user_id,
        email: session.email,
        display_name: session.display_name,
        first_name: session.first_name,
        last_name: session.last_name,
    };

    Ok(Json(SessionSnapshotResponse {
        authenticated: true,
        user: Some(user),
    }))
}

// Déconnexion applicative + logout OIDC.
//
// Cette route :
// - lit la session locale,
// - récupère éventuellement le id_token,
// - supprime la session mémoire,
// - expire le cookie local,
// - redirige le navigateur vers le logout Keycloak,
// - puis Keycloak revient vers le frontend.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let maybe_session_id = crate::auth::session::extract_session_id_from_headers(
        &headers,
        &state.config.auth.cookie_name,
    );

    let mut maybe_id_token: Option<String> = None;

    if let Some(session_id) = maybe_session_id {
        let mut sessions = state.sessions.write().await;

        if let Some(session) = sessions.remove(&session_id) {
            maybe_id_token = session.id_token;
        }
    }

    let cleared_cookie = crate::auth::session::build_cleared_session_cookie(
        &state.config.auth.cookie_name,
        state.config.auth.cookie_secure,
    );

    let redirect_target = if let Some(id_token) = maybe_id_token.as_deref() {
        crate::auth::oidc::build_logout_redirect_url(&state, id_token)
    } else {
        state.config.frontend_post_logout_url()
    };

    tracing::info!("LOGOUT redirect_target = {}", redirect_target);
    let mut response = Redirect::to(&redirect_target).into_response();

    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cleared_cookie)
            .map_err(|error| AppError::Internal(format!("Invalid Set-Cookie header: {}", error)))?,
    );

    Ok(response)
}

// Forgot password.
pub async fn forgot_password(
    State(_state): State<AppState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> AppResult<Json<MessageResponse>> {
    payload.validate()?;

    Ok(Json(MessageResponse {
        message: "If an account exists for this email, a reset flow will be triggered".to_string(),
    }))
}

// Reset password.
pub async fn reset_password(
    State(_state): State<AppState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> AppResult<Json<MessageResponse>> {
    payload.validate()?;

    if payload.new_password != payload.confirm_password {
        return Err(AppError::Validation(
            "Password and confirmation do not match".to_string(),
        ));
    }

    Ok(Json(MessageResponse {
        message: "Password reset payload accepted by backend skeleton".to_string(),
    }))
}
