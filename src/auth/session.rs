// Gestion de session applicative.
//
// Ce fichier contient une première implémentation simple :
// - création de session mémoire,
// - construction du cookie HTTP-only,
// - lecture du cookie depuis les headers,
// - invalidation du cookie.

use axum::http::{HeaderMap, header};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::User,
    state::{AppSession, AppState},
};

// Crée une session applicative locale à partir d'un utilisateur local.
//
// Pour le moment, cette session est stockée en mémoire.
// Plus tard, elle pourra être stockée dans Redis ou une base.
pub async fn create_session(state: &AppState, user: &User) -> AppResult<String> {
    // Génération d'un identifiant de session opaque.
    let session_id = Uuid::new_v4().to_string();

    let keycloak_sub = user
        .keycloak_sub
        .clone()
        .ok_or_else(|| AppError::Internal("Local user is missing keycloak_sub".to_string()))?;

    // Construction de la session applicative.
    let session = AppSession {
        user_id: user.id,
        keycloak_sub,
        email: user.email.clone(),
        display_name: user.display_name.clone(),
        first_name: user.first_name.clone(),
        last_name: user.last_name.clone(),
    };

    // Stockage mémoire de la session.
    {
        let mut sessions = state.sessions.write().await;
        sessions.insert(session_id.clone(), session);
    }

    // Retourne l'identifiant de session.
    Ok(session_id)
}

// Construit la valeur du cookie de session.
//
// On utilise :
// - HttpOnly pour empêcher l'accès JavaScript,
// - SameSite=Lax pour une base saine,
// - Secure si configuré.
pub fn build_session_cookie(cookie_name: &str, session_id: &str, secure: bool) -> String {
    // Base du cookie.
    let mut cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=28800",
        cookie_name, session_id
    );

    // Ajout du flag Secure si demandé.
    if secure {
        cookie.push_str("; Secure");
    }

    cookie
}

// Construit un cookie d'invalidation de session.
pub fn build_cleared_session_cookie(cookie_name: &str, secure: bool) -> String {
    // Base du cookie de suppression.
    let mut cookie = format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
        cookie_name
    );

    // Ajout du flag Secure si demandé.
    if secure {
        cookie.push_str("; Secure");
    }

    cookie
}

// Extrait l'identifiant de session depuis le header Cookie.
//
// Cette fonction lit le header HTTP Cookie et retrouve
// la valeur du cookie attendu.
pub fn extract_session_id_from_headers(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    // Lecture du header Cookie brut.
    let raw_cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;

    // Parcours des cookies séparés par ';'.
    for cookie_part in raw_cookie_header.split(';') {
        let trimmed = cookie_part.trim();

        // Découpage nom=valeur.
        if let Some((name, value)) = trimmed.split_once('=') && name == cookie_name {
            return Some(value.to_string());
        }
    }

    None
}
