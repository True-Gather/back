// Intégration OIDC.
//
// Ce fichier contient la logique de préparation du flow OIDC vers Keycloak :
// - génération du state,
// - génération du nonce,
// - génération du PKCE verifier,
// - calcul du PKCE challenge en S256,
// - construction de l'URL d'autorisation,
// - échange du code contre les tokens,
// - récupération du profil utilisateur via l'endpoint userinfo.

use axum::http::header;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use chrono::Utc;

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
//
// Cette structure correspond à la réponse standard du flow
// Authorization Code après échange du code contre des tokens.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    // Access token OIDC/OAuth2.
    pub access_token: String,

    // ID token éventuel.
    pub id_token: Option<String>,

    // Refresh token éventuel.
    pub refresh_token: Option<String>,

    // Type du token, généralement "Bearer".
    pub token_type: String,

    // Durée de vie éventuelle.
    pub expires_in: Option<u64>,

    // Scopes retournés.
    pub scope: Option<String>,
}

// Profil utilisateur récupéré via l'endpoint userinfo.
#[derive(Debug, Clone, Deserialize)]
pub struct UserInfoClaims {
    // Identifiant stable du fournisseur d'identité.
    pub sub: String,

    // Email éventuel.
    pub email: Option<String>,

    // Email vérifié ou non.
    pub email_verified: Option<bool>,

    // Username éventuel.
    pub preferred_username: Option<String>,

    // Prénom éventuel.
    pub given_name: Option<String>,

    // Nom de famille éventuel.
    pub family_name: Option<String>,

    // Nom complet éventuel.
    pub name: Option<String>,
}

// Génère un code verifier PKCE conforme.
//
// On concatène deux UUID v4 sans tirets.
// Chaque UUID "simple" fait 32 caractères hexadécimaux.
// Le résultat final fait 64 caractères, ce qui respecte bien
// la contrainte PKCE de longueur comprise entre 43 et 128.
fn generate_pkce_verifier() -> String {
    format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

// Calcule le code challenge PKCE en S256.
//
// Le challenge est : BASE64URL_NO_PAD(SHA256(code_verifier))
fn build_pkce_s256_challenge(code_verifier: &str) -> String {
    // Calcul du hash SHA-256 du verifier.
    let digest = Sha256::digest(code_verifier.as_bytes());

    // Encodage base64url sans padding.
    URL_SAFE_NO_PAD.encode(digest)
}

// Prépare un flow OIDC vers Keycloak.
//
// Cette fonction :
// - génère un state OAuth,
// - génère un nonce OIDC,
// - génère un PKCE verifier,
// - calcule le challenge S256,
// - stocke ces infos dans le state partagé,
// - construit l'URL de redirection vers Keycloak.
pub async fn prepare_authorization_redirect(
    state: &AppState,
    is_registration: bool,
) -> OidcAuthorizationRequest {
    // Génération d'un state unique pour protéger le callback.
    let oauth_state = Uuid::new_v4().to_string();

    // Génération d'un nonce OIDC attendu plus tard dans l'ID token.
    let nonce = Uuid::new_v4().to_string();

    // Génération d'un PKCE verifier.
    let pkce_verifier = generate_pkce_verifier();

    // Logs temporaires
    tracing::info!(
        "OIDC start | client_id={} | state={} | pkce_verifier={} | is_registration={}",
        state.config.keycloak.client_id,
        oauth_state,
        pkce_verifier,
        is_registration
    );

    // Calcul du challenge PKCE en S256.
    let pkce_challenge = build_pkce_s256_challenge(&pkce_verifier);

    // Construction de la demande d'auth en attente.
    let pending_request = PendingAuthRequest {
        nonce: nonce.clone(),
        pkce_verifier,
        is_registration,
        created_at: Utc::now(),
    };

    // Stockage temporaire dans le state partagé.
    {
        let mut pending_auth = state.pending_auth.write().await;
        pending_auth.insert(oauth_state.clone(), pending_request);
    }

    // Construction de l'endpoint d'autorisation Keycloak.
    let authorization_endpoint = format!(
        "{}/protocol/openid-connect/auth",
        state.config.keycloak.issuer_url
    );

    // Scopes standard OIDC utiles au projet.
    let scope = "openid profile email";

    // Redirect URI backend.
    let redirect_uri = state.config.auth_callback_url();

    // Paramètre spécifique pour orienter Keycloak vers le parcours inscription.
    let registration_part = if is_registration {
        "&kc_action=register"
    } else {
        ""
    };

    // Construction de l'URL finale.
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

    // Retourne la demande prête à être utilisée.
    OidcAuthorizationRequest { authorization_url }
}

// Échange un authorization code contre des tokens.
//
// Version simplifiée pour client public + PKCE.
// On n'envoie aucun client secret.
pub async fn exchange_code_for_tokens(
    state: &AppState,
    code: &str,
    pkce_verifier: &str,
) -> AppResult<TokenResponse> {
    // Construction de l'endpoint token.
    let token_endpoint = format!(
        "{}/protocol/openid-connect/token",
        state.config.keycloak.issuer_url
    );

    // Construction du formulaire standard OIDC.
    let form_fields = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", state.config.auth_callback_url()),
        ("client_id", state.config.keycloak.client_id.clone()),
        ("code_verifier", pkce_verifier.to_string()),
    ];

    // Encodage du body au format application/x-www-form-urlencoded.
    let encoded_form = serde_urlencoded::to_string(&form_fields)
        .map_err(|error| AppError::Internal(format!("Failed to encode token form: {}", error)))?;

    // Log utile de debug.
    tracing::info!(
        "OIDC start | client_id={} | state={} | is_registration={}",
        state.config.keycloak.client_id,
        state.config.auth_callback_url(),
        pkce_verifier.len(),
    );

    // Envoi de la requête sans secret.
    let response = state
        .http_client
        .post(&token_endpoint)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(encoded_form)
        .send()
        .await?;

    // Vérification du succès HTTP.
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

    // Désérialisation de la réponse token.
    let token_response = response.json::<TokenResponse>().await?;

    Ok(token_response)
}

// Récupère le profil utilisateur via l'endpoint userinfo.
//
// Cette fonction utilise l'access token obtenu après l'échange du code.
pub async fn fetch_userinfo(
    state: &AppState,
    access_token: &str,
) -> AppResult<UserInfoClaims> {
    // Construction de l'endpoint userinfo.
    let userinfo_endpoint = format!(
        "{}/protocol/openid-connect/userinfo",
        state.config.keycloak.issuer_url
    );

    // Appel HTTP authentifié avec l'access token.
    let response = state
        .http_client
        .get(&userinfo_endpoint)
        .bearer_auth(access_token)
        .send()
        .await?;

    // Vérification du succès HTTP.
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

    // Désérialisation du profil utilisateur.
    let userinfo = response.json::<UserInfoClaims>().await?;

    Ok(userinfo)
}