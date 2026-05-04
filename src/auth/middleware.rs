// Middleware / extractor d'authentification.
//
// Ce fichier permet de récupérer l'utilisateur courant
// à partir du cookie de session applicatif.
//
// But :
// - éviter de relire le cookie à la main dans chaque handler,
// - centraliser la vérification de session,
// - garantir que l'identité utilisateur vient du backend.

use axum::{
    extract::{FromRequestParts},
    http::{request::Parts, StatusCode},
};

use crate::{
    models::User,
    state::AppState,
};

/// Extractor personnalisé représentant l'utilisateur courant.
///
/// Exemple d'utilisation dans un handler :
/// `CurrentUser(user): CurrentUser`
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 1. Lire l'identifiant de session depuis les headers/cookies.
        let maybe_session_id = crate::auth::session::extract_session_id_from_headers(
            &parts.headers,
            &state.config.auth.cookie_name,
        );

        let Some(session_id) = maybe_session_id else {
            return Err(StatusCode::UNAUTHORIZED);
        };

        // 2. Charger la session depuis le store mémoire.
        // Ici on utilise bien read().await car sessions est un RwLock async.
        let maybe_session = {
            let sessions = state.sessions.read().await;
            sessions.get(&session_id).cloned()
        };

        let Some(session) = maybe_session else {
            return Err(StatusCode::UNAUTHORIZED);
        };

        // 3. Recharger l'utilisateur depuis PostgreSQL.
        // Sécurité :
        // on ne fait pas confiance uniquement aux infos stockées en session.
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT
                keycloak_id,
                first_name,
                last_name,
                display_name,
                email,
                profile_photo_url,
                created_at,
                updated_at,
                last_login_at,
                is_active
            FROM users
            WHERE keycloak_id = $1
            "#,
        )
        .bind(&session.keycloak_id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

        // 4. Refuser l'accès si le compte est désactivé.
        if !user.is_active {
            return Err(StatusCode::UNAUTHORIZED);
        }

        Ok(CurrentUser(user))
    }
}