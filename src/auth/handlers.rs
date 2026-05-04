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
        AuthCallbackQuery, ChangePasswordRequest, ForgotPasswordRequest, MessageResponse,
        ResetPasswordRequest, SessionSnapshotResponse, UpdateProfileRequest, VerifyEmailQuery,
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
    let (local_user, is_new) = crate::auth::sync::sync_user_from_keycloak(&state, &userinfo).await?;

    // Si c'est une première inscription, on envoie un email de vérification.
    // On ne se fie pas à email_verified de Keycloak (en dev il est souvent true par défaut).
    //
    // Important : au redémarrage du backend, le store utilisateur en mémoire est vide.
    // Un login d'un utilisateur Keycloak existant peut donc ressortir `is_new=true`.
    // On ne déclenche la vérification email que pour le parcours inscription.
    if pending_request.is_registration && is_new {
        let token = crate::auth::email_verification::generate_token();
        let token_hash = crate::auth::email_verification::hash_token(&token);

        // Stockage du hash dans Redis (TTL 15 min).
        if let Err(e) = crate::auth::email_verification::store_verification_token(
            &state.redis,
            &token_hash,
            local_user.id,
        )
        .await
        {
            tracing::warn!("Impossible de stocker le token de vérification : {e}");
        } else {
            let verify_url = format!(
                "{}/api/v1/auth/verify-email?token={}",
                state.config.backend.base_url, token
            );
            let display_name = local_user.first_name.as_deref().unwrap_or(&local_user.display_name);

            if let Err(e) = crate::mail::send_verification_email(
                &local_user.email,
                display_name,
                &verify_url,
            )
            .await
            {
                tracing::warn!(
                    email = local_user.email.as_str(),
                    "Échec envoi email de vérification : {e}"
                );
            }
        }

        // Pas de session créée : l'utilisateur doit vérifier son email avant d'accéder au dashboard.
        let pending_url = format!(
            "{}/auth/verify-email?pending=1",
            state.config.frontend.base_url.trim_end_matches('/')
        );
        return Ok(Redirect::to(&pending_url).into_response());
    }

    // Création session (uniquement pour les utilisateurs déjà existants)
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

    let redirect_target =
        crate::auth::oidc::build_logout_redirect_url(&state, maybe_id_token.as_deref());

    tracing::info!(
        "LOGOUT redirect prepared (has_id_token_hint={})",
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
    .bind(&session.keycloak_id)
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

// Changement de mot de passe pour un utilisateur connecté.
//
// Flux :
//   1. Extrait et valide la session depuis le cookie tg_session.
//   2. Valide le payload (14 caractères min, correspondance confirmation).
//   3. Obtient un token admin (client_credentials).
//   4. Appelle l'API admin Keycloak pour mettre à jour le mot de passe.
//   5. Envoie un email de confirmation (non bloquant).
pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ChangePasswordRequest>,
) -> AppResult<Json<MessageResponse>> {
    // Validation structurelle du payload (longueur, non vide).
    payload.validate()?;

    // Vérification des mots de passe identiques.
    if payload.new_password != payload.confirm_password {
        return Err(AppError::BadRequest(
            "Le nouveau mot de passe et sa confirmation ne correspondent pas".to_string(),
        ));
    }

    // Extraction de la session depuis le cookie.
    let session_id = crate::auth::session::extract_session_id_from_headers(
        &headers,
        &state.config.auth.cookie_name,
    )
    .ok_or(AppError::Unauthorized)?;

    let session = {
        let sessions = state.sessions.read().await;
        sessions.get(&session_id).cloned()
    }
    .ok_or(AppError::Unauthorized)?;

    // Construction de l'URL du realm Keycloak.
    let issuer = &state.config.keycloak.issuer_url_internal;

    let realm_base = issuer
        .trim_end_matches('/')
        .trim_end_matches("/realms/truegather");

    let token_url = format!("{}/realms/truegather/protocol/openid-connect/token", realm_base);

    // Obtention d'un token admin via client_credentials.
    let admin_params = [
        ("grant_type", "client_credentials"),
        ("client_id", state.config.keycloak.client_id.as_str()),
        (
            "client_secret",
            state
                .config
                .keycloak
                .client_secret
                .as_deref()
                .unwrap_or(""),
        ),
    ];

    let admin_token_resp = state
        .http_client
        .post(&token_url)
        .form(&admin_params)
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("Keycloak admin token request failed: {e}")))?;

    if !admin_token_resp.status().is_success() {
        return Err(AppError::Internal(
            "Impossible d'obtenir un token admin Keycloak".to_string(),
        ));
    }

    let admin_token_json = admin_token_resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse admin token response: {e}")))?;

    let admin_token = admin_token_json["access_token"]
        .as_str()
        .ok_or_else(|| AppError::Internal("Missing access_token in admin response".to_string()))?
        .to_string();

    // Mise à jour du mot de passe via l'API admin Keycloak.
    let reset_url = format!(
        "{}/admin/realms/truegather/users/{}/reset-password",
        realm_base, session.keycloak_id
    );

    let reset_body = serde_json::json!({
        "type": "password",
        "value": payload.new_password,
        "temporary": false
    });

    let reset_resp = state
        .http_client
        .put(&reset_url)
        .bearer_auth(&admin_token)
        .json(&reset_body)
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("Keycloak reset-password request failed: {e}")))?;

    if !reset_resp.status().is_success() {
        let status = reset_resp.status();
        let body = reset_resp
            .text()
            .await
            .unwrap_or_else(|_| "(corps illisible)".to_string());
        return Err(AppError::Upstream(format!(
            "Keycloak refused password update ({status}): {body}"
        )));
    }

    // Envoi d'un email de confirmation (non bloquant).
    // Si l'envoi échoue, le changement de mot de passe reste valide.
    let display_name = session
        .first_name
        .as_deref()
        .unwrap_or(&session.display_name);

    crate::mail::send_password_changed_email(&session.email, display_name).await;

    Ok(Json(MessageResponse {
        message: "Mot de passe mis à jour avec succès".to_string(),
    }))
}

// Mise à jour du profil (prénom et/ou nom de famille) pour un utilisateur connecté.
//
// Flux :
//   1. Extrait et valide la session depuis le cookie tg_session.
//   2. Valide le payload (au moins un champ fourni, longueur max).
//   3. Obtient un token admin (client_credentials).
//   4. Appelle l'API admin Keycloak pour mettre à jour firstName/lastName.
//   5. Met à jour le store local et la session en mémoire.
//   6. Envoie un email de confirmation (non bloquant).
pub async fn update_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateProfileRequest>,
) -> AppResult<Json<UserProfileView>> {
    // Validation structurelle (longueur max).
    payload.validate()?;

    // Au moins un champ doit être fourni et non vide.
    let first_name_trimmed = payload.first_name.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let last_name_trimmed = payload.last_name.as_deref().map(str::trim).filter(|s| !s.is_empty());

    if first_name_trimmed.is_none() && last_name_trimmed.is_none() {
        return Err(AppError::BadRequest(
            "Au moins un champ (prénom ou nom) doit être fourni".to_string(),
        ));
    }

    // Extraction de la session depuis le cookie.
    let session_id = crate::auth::session::extract_session_id_from_headers(
        &headers,
        &state.config.auth.cookie_name,
    )
    .ok_or(AppError::Unauthorized)?;

    let session = {
        let sessions = state.sessions.read().await;
        sessions.get(&session_id).cloned()
    }
    .ok_or(AppError::Unauthorized)?;

    // Construction de l'URL de base Keycloak.
    let issuer = &state.config.keycloak.issuer_url_internal;

    let realm_base = issuer
        .trim_end_matches('/')
        .trim_end_matches("/realms/truegather");

    let token_url = format!("{}/realms/truegather/protocol/openid-connect/token", realm_base);

    // Obtention d'un token admin via client_credentials.
    let admin_params = [
        ("grant_type", "client_credentials"),
        ("client_id", state.config.keycloak.client_id.as_str()),
        (
            "client_secret",
            state
                .config
                .keycloak
                .client_secret
                .as_deref()
                .unwrap_or(""),
        ),
    ];

    let admin_token_resp = state
        .http_client
        .post(&token_url)
        .form(&admin_params)
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("Keycloak admin token request failed: {e}")))?;

    if !admin_token_resp.status().is_success() {
        return Err(AppError::Internal(
            "Impossible d'obtenir un token admin Keycloak".to_string(),
        ));
    }

    let admin_token_json = admin_token_resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse admin token response: {e}")))?;

    let admin_token = admin_token_json["access_token"]
        .as_str()
        .ok_or_else(|| AppError::Internal("Missing access_token in admin response".to_string()))?
        .to_string();

    // Mise à jour via l'API admin Keycloak.
    let update_url = format!(
        "{}/admin/realms/truegather/users/{}",
        realm_base, session.keycloak_id
    );

    let mut update_body = serde_json::Map::new();
    if let Some(first) = first_name_trimmed {
        update_body.insert("firstName".to_string(), serde_json::Value::String(first.to_string()));
    }
    if let Some(last) = last_name_trimmed {
        update_body.insert("lastName".to_string(), serde_json::Value::String(last.to_string()));
    }

    let update_resp = state
        .http_client
        .put(&update_url)
        .bearer_auth(&admin_token)
        .json(&serde_json::Value::Object(update_body))
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("Keycloak user update request failed: {e}")))?;

    if !update_resp.status().is_success() {
        let status = update_resp.status();
        let body = update_resp
            .text()
            .await
            .unwrap_or_else(|_| "(corps illisible)".to_string());
        return Err(AppError::Upstream(format!(
            "Keycloak refused profile update ({status}): {body}"
        )));
    }

    // Calcul du nouveau display_name à partir des nouvelles valeurs.
    let new_first = first_name_trimmed
        .map(str::to_string)
        .or_else(|| session.first_name.clone());
    let new_last = last_name_trimmed
        .map(str::to_string)
        .or_else(|| session.last_name.clone());

    let new_display_name = match (&new_first, &new_last) {
        (Some(f), Some(l)) => format!("{} {}", f, l),
        (Some(f), None) => f.clone(),
        (None, Some(l)) => l.clone(),
        (None, None) => session.display_name.clone(),
    };

    // Mise à jour du store local en mémoire.
    {
        let mut users = state.users.write().await;
        if let Some(user) = users.get_mut(&session.user_id) {
            user.first_name = new_first.clone();
            user.last_name = new_last.clone();
            user.display_name = new_display_name.clone();
            user.updated_at = chrono::Utc::now();
        }
    }

    // Mise à jour de la session courante en mémoire.
    {
        let mut sessions = state.sessions.write().await;
        if let Some(s) = sessions.get_mut(&session_id) {
            s.first_name = new_first.clone();
            s.last_name = new_last.clone();
            s.display_name = new_display_name.clone();
        }
    }

    // Email de confirmation (non bloquant).
    let display_name_for_email = new_first.as_deref().unwrap_or(&new_display_name);
    crate::mail::send_profile_changed_email(&session.email, display_name_for_email).await;

    Ok(Json(UserProfileView {
        id: session.user_id,
        email: session.email.clone(),
        display_name: new_display_name,
        first_name: new_first,
        last_name: new_last,
        profile_photo_url: session.profile_photo_url,
    }))
}

// Vérification d'email via le token reçu par email.
//
// Flux :
//   1. Lit le token depuis la query string.
//   2. Hashe le token et cherche dans Redis.
//   3. Si valide : marque email_verified=true.
//   4. Redirige vers la page frontend de confirmation.
//
// Important : on ne crée pas de session ici. La session applicative doit être
// créée par le callback OIDC après une vraie connexion Keycloak, afin de
// conserver l'id_token nécessaire au logout OIDC.
pub async fn verify_email(
    State(state): State<AppState>,
    Query(query): Query<VerifyEmailQuery>,
) -> AppResult<Response> {
    let token = query
        .token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| AppError::BadRequest("Token de vérification manquant".to_string()))?;

    let maybe_user_id =
        crate::auth::email_verification::verify_and_consume_token(&state.redis, &token).await?;

    let Some(user_id) = maybe_user_id else {
        // Token invalide ou expiré — redirection vers le frontend avec erreur.
        let redirect_url = format!(
            "{}/auth/verify-email?error=invalid_token",
            state.config.frontend.base_url.trim_end_matches('/')
        );
        return Ok(Redirect::to(&redirect_url).into_response());
    };

    // Marquer l'email comme vérifié.
    let user_exists = {
        let mut users = state.users.write().await;
        if let Some(user) = users.get_mut(&user_id) {
            user.email_verified = true;
            true
        } else {
            false
        }
    };

    if !user_exists {
        return Ok(Redirect::to(&format!(
            "{}/auth/verify-email?error=user_not_found",
            state.config.frontend.base_url.trim_end_matches('/')
        )).into_response());
    }

    tracing::info!(user_id = %user_id, "Email vérifié avec succès");

    let redirect_url = format!(
        "{}/auth/verify-email?verified=1",
        state.config.frontend.base_url.trim_end_matches('/')
    );

    Ok(Redirect::to(&redirect_url).into_response())
}
