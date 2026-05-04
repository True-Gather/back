// Module Redis — pool de connexions et helpers pour les rooms de signalisation.

use deadpool_redis::{Config, Pool, Runtime};
use deadpool_redis::redis::AsyncCommands;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

// Crée un pool de connexions Redis à partir d'une URL.
pub fn create_pool(url: &str) -> AppResult<Pool> {
    Config::from_url(url)
        .create_pool(Some(Runtime::Tokio1))
        .map_err(|e| AppError::Config(format!("Redis pool error: {e}")))
}

// ─── Gestion des membres de room ─────────────────────────────────────────────

// Ajoute un utilisateur dans une room Redis.
//
// La clé `room:<room_id>:members` est un Set Redis.
// Elle expire automatiquement après 24 h d'inactivité.
pub async fn room_add_member(pool: &Pool, room_id: &str, user_id: Uuid) -> AppResult<()> {
    let mut conn = pool
        .get()
        .await
        .map_err(|e| AppError::Internal(format!("Redis connection: {e}")))?;

    let key = format!("room:{room_id}:members");
    let _: () = conn
        .sadd(&key, user_id.to_string())
        .await
        .map_err(|e| AppError::Internal(format!("Redis sadd: {e}")))?;

    // Réinitialise le TTL à chaque nouvel entrant pour garder la room vivante.
    let _: () = conn
        .expire(&key, 86_400_i64)
        .await
        .map_err(|e| AppError::Internal(format!("Redis expire: {e}")))?;

    Ok(())
}

// Retire un utilisateur d'une room Redis.
pub async fn room_remove_member(pool: &Pool, room_id: &str, user_id: Uuid) -> AppResult<()> {
    let mut conn = pool
        .get()
        .await
        .map_err(|e| AppError::Internal(format!("Redis connection: {e}")))?;

    let key = format!("room:{room_id}:members");
    let _: () = conn
        .srem(&key, user_id.to_string())
        .await
        .map_err(|e| AppError::Internal(format!("Redis srem: {e}")))?;

    Ok(())
}

// Retourne la liste des membres d'une room Redis.
pub async fn room_list_members(pool: &Pool, room_id: &str) -> AppResult<Vec<Uuid>> {
    let mut conn = pool
        .get()
        .await
        .map_err(|e| AppError::Internal(format!("Redis connection: {e}")))?;

    let key = format!("room:{room_id}:members");
    let members: Vec<String> = conn
        .smembers(&key)
        .await
        .map_err(|e| AppError::Internal(format!("Redis smembers: {e}")))?;

    let uuids = members
        .iter()
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect();

    Ok(uuids)
}
