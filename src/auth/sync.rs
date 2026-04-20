// Synchronisation utilisateur locale depuis les claims Keycloak.
//
// Ce fichier gère :
// - la création JIT d'un user local,
// - la mise à jour du profil depuis les claims Keycloak,
// - le mapping entre sub Keycloak et user applicatif.

use chrono::Utc;
use uuid::Uuid;

use crate::{
    auth::oidc::UserInfoClaims,
    error::{AppError, AppResult},
    models::User,
    state::AppState,
};

// Construit un nom d'affichage cohérent à partir des claims Keycloak.
fn build_display_name(userinfo: &UserInfoClaims) -> String {
    userinfo
        .name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| match (&userinfo.given_name, &userinfo.family_name) {
            (Some(first_name), Some(last_name)) => Some(format!("{} {}", first_name, last_name)),
            (Some(first_name), None) => Some(first_name.clone()),
            (None, Some(last_name)) => Some(last_name.clone()),
            (None, None) => None,
        })
        .or_else(|| userinfo.preferred_username.clone())
        .or_else(|| userinfo.email.clone())
        .unwrap_or_else(|| "TrueGather User".to_string())
}

// Crée ou met à jour un utilisateur local à partir du profil Keycloak.
//
// Retourne `(User, bool)` où le bool indique si l'utilisateur vient d'être créé.
pub async fn sync_user_from_keycloak(
    state: &AppState,
    userinfo: &UserInfoClaims,
) -> AppResult<(User, bool)> {
    let keycloak_sub = userinfo.sub.trim().to_string();

    if keycloak_sub.is_empty() {
        return Err(AppError::Internal(
            "Keycloak userinfo returned an empty sub".to_string(),
        ));
    }

    let display_name = build_display_name(userinfo);
    let email = userinfo.email.clone().unwrap_or_default();
    let first_name = userinfo.given_name.clone();
    let last_name = userinfo.family_name.clone();
    let now = Utc::now();

    // Étape 1 : retrouver un user existant via le mapping sub -> user_id.
    let maybe_user_id = {
        let users_by_keycloak_sub = state.users_by_keycloak_sub.read().await;
        users_by_keycloak_sub.get(&keycloak_sub).copied()
    };

    // Étape 2 : si trouvé, mise à jour du user existant.
    if let Some(user_id) = maybe_user_id {
        let mut users = state.users.write().await;

        if let Some(existing_user) = users.get_mut(&user_id) {
            existing_user.keycloak_sub = Some(keycloak_sub.clone());
            existing_user.email = email;
            existing_user.display_name = display_name;
            existing_user.first_name = first_name;
            existing_user.last_name = last_name;
            existing_user.updated_at = now;
            existing_user.last_login_at = Some(now);
            // Si Keycloak confirme la vérification, on l'entérine (jamais de retour en arrière).
            if userinfo.email_verified.unwrap_or(false) {
                existing_user.email_verified = true;
            }

            return Ok((existing_user.clone(), false));
        }
    }

    // Étape 3 : sinon création JIT d'un nouvel utilisateur local.
    let new_user = User {
        id: Uuid::new_v4(),
        keycloak_sub: Some(keycloak_sub.clone()),
        email,
        display_name,
        first_name,
        last_name,
        // Reprend le statut Keycloak — si Keycloak a déjà vérifié l'email, on l'accepte.
        email_verified: userinfo.email_verified.unwrap_or(false),
        created_at: now,
        updated_at: now,
        last_login_at: Some(now),
    };

    {
        let mut users = state.users.write().await;
        users.insert(new_user.id, new_user.clone());
    }

    {
        let mut users_by_keycloak_sub = state.users_by_keycloak_sub.write().await;
        users_by_keycloak_sub.insert(keycloak_sub, new_user.id);
    }

    Ok((new_user, true))
}
