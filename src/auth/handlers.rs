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
use serde::{Deserialize, Serialize};

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

    let session = {
        let sessions = state.sessions.read().await;
        sessions.get(&session_id).cloned()
    };

    let Some(session) = session else {
        return Ok(Json(SessionSnapshotResponse {
            authenticated: false,
            user: None,
        }));
    };

    // Construction de la vue user pour le frontend.
    let user = UserProfileView {
        id: session.user_id,
        email: session.email,
        display_name: session.display_name,
        first_name: session.first_name,
        last_name: session.last_name,
        profile_photo_url: session.profile_photo_url,
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

    tracing::info!(
        "LOGOUT redirect prepared (oidc_logout={})",
        maybe_id_token.is_some()
    );
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

// Payload pour la mise à jour de l'avatar.
#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateAvatarRequest {
    // Base64 data URL (ex. "data:image/png;base64,...") ou null pour supprimer.
    pub avatar_url: Option<String>,
}

// Met à jour la photo de profil de l'utilisateur connecté.
//
// Persiste la valeur en base PostgreSQL et met à jour la session mémoire.
pub async fn update_avatar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateAvatarRequest>,
) -> AppResult<Json<MessageResponse>> {
    // Extraction de la session.
    let session_id = crate::auth::session::extract_session_id_from_headers(
        &headers,
        &state.config.auth.cookie_name,
    )
    .ok_or_else(|| AppError::Unauthorized)?;

    let session = {
        let sessions = state.sessions.read().await;
        sessions.get(&session_id).cloned()
    }
    .ok_or_else(|| AppError::Unauthorized)?;

    // Mise à jour en base.
    sqlx::query(
        r#"
        UPDATE users
        SET profile_photo_url = $1, updated_at = NOW()
        WHERE keycloak_id = $2
        "#,
    )
    .bind(&payload.avatar_url)
    .bind(&session.keycloak_sub)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("DB avatar update error: {}", e)))?;

    // Mise à jour de la session mémoire.
    {
        let mut sessions = state.sessions.write().await;
        if let Some(s) = sessions.get_mut(&session_id) {
            s.profile_photo_url = payload.avatar_url;
        }
    }

    Ok(Json(MessageResponse {
        message: "Avatar updated".to_string(),
    }))
}

// Payload pour le changement de mot de passe.
#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
    pub confirm_password: String,
}

// Change le mot de passe de l'utilisateur connecté via l'API Keycloak.
//
// Flow :
// 1. Extraction de la session courante.
// 2. Validation du payload (correspondance, longueur minimale).
// 3. Vérification du mot de passe actuel via ROPC (Resource Owner Password Credentials).
// 4. Obtention d'un token admin via client_credentials.
// 5. Mise à jour du mot de passe via l'API admin Keycloak.
pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ChangePasswordRequest>,
) -> AppResult<Json<MessageResponse>> {
    // 1. Extraction de la session.
    let session_id = crate::auth::session::extract_session_id_from_headers(
        &headers,
        &state.config.auth.cookie_name,
    )
    .ok_or_else(|| AppError::Unauthorized)?;

    let session = {
        let sessions = state.sessions.read().await;
        sessions.get(&session_id).cloned()
    }
    .ok_or_else(|| AppError::Unauthorized)?;

    // 2. Validation basique du payload.
    if payload.new_password != payload.confirm_password {
        return Err(AppError::Validation(
            "Les mots de passe ne correspondent pas".to_string(),
        ));
    }
    if payload.new_password.len() < 14 {
        return Err(AppError::Validation(
            "Le nouveau mot de passe doit contenir au moins 14 caractères".to_string(),
        ));
    }

    // 3. Construction des URLs Keycloak à partir de l'issuer_url interne.
    //    Format attendu : http(s)://host:port/realms/{realm}
    let kc_internal = state
        .config
        .keycloak
        .issuer_url_internal
        .as_deref()
        .unwrap_or(&state.config.keycloak.issuer_url);

    let (kc_host, realm) = kc_internal
        .split_once("/realms/")
        .map(|(h, r)| (h.to_string(), r.to_string()))
        .ok_or_else(|| {
            AppError::Internal("Format d'issuer_url Keycloak invalide".to_string())
        })?;

    let token_url = format!(
        "{}/realms/{}/protocol/openid-connect/token",
        kc_host, realm
    );
    let reset_url = format!(
        "{}/admin/realms/{}/users/{}/reset-password",
        kc_host, realm, session.keycloak_sub
    );

    let client_id = &state.config.keycloak.client_id;
    let client_secret = state
        .config
        .keycloak
        .client_secret
        .as_deref()
        .unwrap_or("");

    // 4. Vérification du mot de passe actuel via ROPC.
    //    Si le grant échoue (401), le mot de passe fourni est incorrect.
    let ropc_params: &[(&str, &str)] = &[
        ("grant_type", "password"),
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret),
        ("username", session.email.as_str()),
        ("password", payload.current_password.as_str()),
    ];
    let ropc_resp = state
        .http_client
        .post(&token_url)
        .form(ropc_params)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Keycloak ROPC error: {}", e)))?;

    if !ropc_resp.status().is_success() {
        return Err(AppError::Validation(
            "Mot de passe actuel incorrect".to_string(),
        ));
    }

    // 5. Obtention d'un token admin via client_credentials.
    let admin_params: &[(&str, &str)] = &[
        ("grant_type", "client_credentials"),
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret),
    ];
    let admin_token_resp = state
        .http_client
        .post(&token_url)
        .form(admin_params)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Keycloak admin token error: {}", e)))?;

    if !admin_token_resp.status().is_success() {
        return Err(AppError::Internal(
            "Impossible d'obtenir le token admin Keycloak — vérifier les rôles du service account"
                .to_string(),
        ));
    }

    let admin_json = admin_token_resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| AppError::Internal(format!("Admin token parse error: {}", e)))?;

    let admin_token = admin_json["access_token"]
        .as_str()
        .ok_or_else(|| AppError::Internal("access_token manquant dans la réponse admin".to_string()))?;

    // 6. Mise à jour du mot de passe via l'API admin Keycloak.
    let reset_resp = state
        .http_client
        .put(&reset_url)
        .bearer_auth(admin_token)
        .json(&serde_json::json!({
            "type": "password",
            "value": payload.new_password,
            "temporary": false
        }))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Keycloak reset-password error: {}", e)))?;

    if !reset_resp.status().is_success() {
        let status = reset_resp.status();
        let body = reset_resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "Keycloak reset-password failed ({}): {}",
            status, body
        )));
    }

    Ok(Json(MessageResponse {
        message: "Mot de passe modifié avec succès".to_string(),
    }))
}
