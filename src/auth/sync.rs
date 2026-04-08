// Synchronisation utilisateur.
//
// Ce fichier contient la logique de synchronisation JIT (Just-In-Time)
// d'un utilisateur Keycloak vers la table `users` PostgreSQL.
//
// Objectif :
// - créer l'utilisateur local s'il n'existe pas encore,
// - mettre à jour ses informations s'il existe déjà,
// - retourner une représentation applicative cohérente.

use chrono::Utc;
use sqlx::query_as;

use crate::{
    auth::oidc::UserInfoClaims,
    error::{AppError, AppResult},
    models::User,
    state::AppState,
};

// Synchronise un utilisateur Keycloak dans la base PostgreSQL.
//
// Cette fonction :
// - utilise `sub` comme identifiant stable,
// - récupère les infos disponibles depuis userinfo,
// - fait un UPSERT sur la table `users`,
// - retourne l'utilisateur applicatif final.
pub async fn sync_user_from_oidc(
    state: &AppState,
    userinfo: &UserInfoClaims,
) -> AppResult<User> {
    // Identifiant stable Keycloak.
    let keycloak_id = userinfo.sub.clone();

    // Email récupéré depuis userinfo.
    let email = userinfo
        .email
        .clone()
        .ok_or_else(|| AppError::BadRequest("Missing email in OIDC userinfo".to_string()))?;

    // Prénom éventuel.
    let first_name = userinfo.given_name.clone();

    // Nom éventuel.
    let last_name = userinfo.family_name.clone();

    // Nom affiché.
    //
    // Ordre de préférence :
    // 1. name
    // 2. preferred_username
    // 3. email
    let display_name = userinfo
        .name
        .clone()
        .or_else(|| userinfo.preferred_username.clone())
        .unwrap_or_else(|| email.clone());

    // Date de connexion actuelle.
    let now = Utc::now();

    // UPSERT PostgreSQL.
    //
    // Si l'utilisateur existe déjà :
    // - on met à jour email / nom / prénom / display_name
    // - on met à jour last_login_at
    //
    // Sinon :
    // - on crée la ligne
    let user = query_as::<_, User>(
        r#"
        INSERT INTO users (
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
        )
        VALUES ($1, $2, $3, $4, $5, NULL, $6, $6, $6, TRUE)
        ON CONFLICT (keycloak_id)
        DO UPDATE SET
            first_name = EXCLUDED.first_name,
            last_name = EXCLUDED.last_name,
            display_name = EXCLUDED.display_name,
            email = EXCLUDED.email,
            updated_at = EXCLUDED.updated_at,
            last_login_at = EXCLUDED.last_login_at
        RETURNING
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
        "#
    )
    .bind(keycloak_id)
    .bind(first_name)
    .bind(last_name)
    .bind(display_name)
    .bind(email)
    .bind(now)
    .fetch_one(&state.db)
    .await
    .map_err(|error| AppError::Internal(format!("Failed to sync user in database: {}", error)))?;

    Ok(user)
}