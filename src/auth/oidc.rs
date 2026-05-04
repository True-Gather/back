// Intégration OIDC.
//
// Ce fichier contient la logique de préparation du flow OIDC vers Keycloak :
// - génération du state,
// - génération du nonce,
// - génération du PKCE verifier,
// - calcul du PKCE challenge en S256,
// - construction de l'URL d'autorisation,
// - échange du code contre les tokens,
// - récupération du profil utilisateur via l'endpoint userinfo,
// - construction de l'URL de logout OIDC.

use axum::http::header;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    state::{AppState, PendingAuthRequest},
};

// Représente le résultat de préparation d'un flow OIDC.
#[derive(Debug, Clone)]
pub struct OidcAuthorizationRequest {
    // URL complète vers laquelle rediriger le navigateur.
    pub authorization_url: String,
}

// Réponse du token endpoint OIDC.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub id_token: Option<String>,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
}

// Profil utilisateur récupéré via l'endpoint userinfo.
#[derive(Debug, Clone, Deserialize)]
pub struct UserInfoClaims {
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub preferred_username: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub name: Option<String>,
}

// Génère un code verifier PKCE conforme.
fn generate_pkce_verifier() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

// Calcule le code challenge PKCE en S256.
fn build_pkce_s256_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

// Prépare un flow OIDC vers Keycloak.
pub async fn prepare_authorization_redirect(
    state: &AppState,
    is_registration: bool,
) -> OidcAuthorizationRequest {
    let oauth_state = Uuid::new_v4().to_string();
    let nonce = Uuid::new_v4().to_string();
    let pkce_verifier = generate_pkce_verifier();

    tracing::info!(
        "OIDC start | client_id={} | state={} | is_registration={}",
        state.config.keycloak.client_id,
        oauth_state,
        is_registration
    );

    let pkce_challenge = build_pkce_s256_challenge(&pkce_verifier);

    let pending_request = PendingAuthRequest {
        nonce: nonce.clone(),
        pkce_verifier,
        is_registration,
        created_at: Utc::now(),
    };

    {
        let mut pending_auth = state.pending_auth.write().await;
        pending_auth.insert(oauth_state.clone(), pending_request);
    }

    let authorization_endpoint = format!(
        "{}/protocol/openid-connect/auth",
        state.config.keycloak.issuer_url
    );

    let scope = "openid profile email";
    let redirect_uri = state.config.auth_callback_url();

    let registration_part = if is_registration {
        // Force Keycloak à réafficher l'écran au lieu de réutiliser une
        // session SSO existante, sinon "Créer un compte" peut reconnecter
        // directement l'utilisateur courant et revenir au dashboard.
        "&prompt=login&kc_action=register"
    } else {
        // Force Keycloak à demander les identifiants même si une session SSO
        // est active. Sans ça, si le backend redémarre (sessions en mémoire
        // perdues), l'utilisateur voit la page d'accueil alors que Keycloak
        // le reconnecte silencieusement au clic "Se connecter".
        "&prompt=login"
    };

    let authorization_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&nonce={}&code_challenge={}&code_challenge_method=S256{}",
        authorization_endpoint,
        urlencoding::encode(&state.config.keycloak.client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(scope),
        urlencoding::encode(&oauth_state),
        urlencoding::encode(&nonce),
        urlencoding::encode(&pkce_challenge),
        registration_part
    );

    OidcAuthorizationRequest { authorization_url }
}

// Construit l'URL de logout OIDC vers Keycloak.
//
// Important : cette URL doit être ouverte par le navigateur,
// pas appelée via fetch côté front.
pub fn build_logout_redirect_url(
    state: &AppState,
    id_token_hint: Option<&str>,
) -> String {
    let logout_endpoint = format!(
        "{}/protocol/openid-connect/logout",
        state.config.keycloak.issuer_url
    );

    let post_logout_redirect_uri = state.config.frontend_post_logout_url();

    let mut query_parts = vec![
        format!(
            "post_logout_redirect_uri={}",
            urlencoding::encode(&post_logout_redirect_uri)
        ),
        format!(
            "client_id={}",
            urlencoding::encode(&state.config.keycloak.client_id)
        ),
    ];

    if let Some(id_token_hint) = id_token_hint {
        query_parts.push(format!(
            "id_token_hint={}",
            urlencoding::encode(id_token_hint)
        ));
    }

    format!("{}?{}", logout_endpoint, query_parts.join("&"))
}

// Échange un authorization code contre des tokens.
pub async fn exchange_code_for_tokens(
    state: &AppState,
    code: &str,
    pkce_verifier: &str,
) -> AppResult<TokenResponse> {
    // Construction de l'endpoint token — utilise l'URL interne Docker si disponible.
    let issuer = &state.config.keycloak.issuer_url_internal;

    let token_endpoint = format!(
        "{}/protocol/openid-connect/token",
        issuer
    );

    let mut form_fields = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", state.config.auth_callback_url()),
        ("client_id", state.config.keycloak.client_id.clone()),
        ("code_verifier", pkce_verifier.to_string()),
    ];

    // Si le client est configuré comme confidentiel (client_secret présent),
    // l'inclure dans l'échange de code.
    if let Some(secret) = state.config.keycloak.client_secret.as_deref() {
        if !secret.is_empty() {
            form_fields.push(("client_secret", secret.to_string()));
        }
    }

    let encoded_form = serde_urlencoded::to_string(&form_fields)
        .map_err(|error| AppError::Internal(format!("Failed to encode token form: {}", error)))?;

    tracing::info!(
        "OIDC token exchange | client_id={} | redirect_uri={}",
        state.config.keycloak.client_id,
        state.config.auth_callback_url(),
    );

    let response = state
        .http_client
        .post(&token_endpoint)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(encoded_form)
        .send()
        .await?;

    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read token endpoint response body".to_string());

        return Err(AppError::Upstream(format!(
            "Token endpoint returned {}: {}",
            status, body
        )));
    }

    let token_response = response.json::<TokenResponse>().await?;

    Ok(token_response)
}

// Récupère le profil utilisateur via l'endpoint userinfo.
//
// Cette fonction utilise l'access token obtenu après l'échange du code.
pub async fn fetch_userinfo(state: &AppState, access_token: &str) -> AppResult<UserInfoClaims> {
    // Construction de l'endpoint userinfo — utilise l'URL interne Docker si disponible.
    let issuer = &state.config.keycloak.issuer_url_internal;

    let userinfo_endpoint = format!(
        "{}/protocol/openid-connect/userinfo",
        issuer
    );

    let response = state
        .http_client
        .get(&userinfo_endpoint)
        .bearer_auth(access_token)
        .send()
        .await?;

    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read userinfo response body".to_string());

        return Err(AppError::Upstream(format!(
            "Userinfo endpoint returned {}: {}",
            status, body
        )));
    }

    let userinfo = response.json::<UserInfoClaims>().await?;

    Ok(userinfo)
}
