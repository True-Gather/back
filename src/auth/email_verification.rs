// Vérification d'email par token sécurisé.
//
// Flux :
//   1. `generate_token()`      → token brut aléatoire (32 bytes, hex)
//   2. `hash_token()`          → hash SHA-256 du token (jamais stocké en clair)
//   3. `store_verification_token()` → stockage dans Redis avec TTL 15 min
//   4. `verify_and_consume_token()` → vérification + suppression (usage unique)
//
// Le token brut est envoyé dans l'email.
// Seul son hash SHA-256 est stocké dans Redis.

use deadpool_redis::Pool;
use deadpool_redis::redis::AsyncCommands;
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

// Durée de validité du token de vérification (15 minutes).
const TOKEN_TTL_SECS: u64 = 900;

// Préfixe des clés Redis pour éviter les collisions.
const REDIS_KEY_PREFIX: &str = "email_verify:";

// ─────────────────────────────────────────────────────────────────────────────
// Génération et hachage
// ─────────────────────────────────────────────────────────────────────────────

/// Génère un token de vérification sécurisé.
///
/// Produit 32 bytes d'entropie via `rand::thread_rng()` et les encode en hex.
/// Le résultat est une chaîne de 64 caractères hexadécimaux.
/// Ce token est envoyé dans l'email — il ne doit jamais être stocké en clair.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Hache un token avec SHA-256 et retourne le résultat en hex.
///
/// Seul ce hash est stocké dans Redis. Même si Redis est compromis,
/// l'attaquant ne peut pas reconstruire le token original.
pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Stockage Redis
// ─────────────────────────────────────────────────────────────────────────────

/// Stocke le hash du token dans Redis avec une expiration de 15 minutes.
///
/// Clé Redis : `email_verify:{token_hash}`
/// Valeur    : `{user_id}` sous forme de chaîne UUID
/// TTL       : 900 secondes (15 minutes)
pub async fn store_verification_token(
    pool: &Pool,
    token_hash: &str,
    user_id: Uuid,
) -> AppResult<()> {
    let mut conn = pool
        .get()
        .await
        .map_err(|e| AppError::Internal(format!("Redis connection: {e}")))?;

    let key = format!("{}{}", REDIS_KEY_PREFIX, token_hash);

    let _: () = conn
        .set_ex(&key, user_id.to_string(), TOKEN_TTL_SECS)
        .await
        .map_err(|e| AppError::Internal(format!("Redis set_ex: {e}")))?;

    Ok(())
}

/// Vérifie un token brut et le consomme (usage unique).
///
/// Hashe le token reçu → cherche dans Redis → si trouvé, supprime la clé
/// et retourne `Some(user_id)`. Retourne `None` si le token est invalide
/// ou expiré.
pub async fn verify_and_consume_token(pool: &Pool, token: &str) -> AppResult<Option<Uuid>> {
    if token.is_empty() {
        return Ok(None);
    }

    let token_hash = hash_token(token);
    let key = format!("{}{}", REDIS_KEY_PREFIX, token_hash);

    let mut conn = pool
        .get()
        .await
        .map_err(|e| AppError::Internal(format!("Redis connection: {e}")))?;

    // Lecture de la valeur associée au hash.
    let user_id_str: Option<String> = conn
        .get(&key)
        .await
        .map_err(|e| AppError::Internal(format!("Redis get: {e}")))?;

    let Some(user_id_str) = user_id_str else {
        // Token absent : invalide ou déjà expiré.
        return Ok(None);
    };

    // Suppression immédiate — token à usage unique.
    let _: () = conn
        .del(&key)
        .await
        .map_err(|e| AppError::Internal(format!("Redis del: {e}")))?;

    let user_id = Uuid::parse_str(&user_id_str)
        .map_err(|_| AppError::Internal("user_id invalide dans Redis".to_string()))?;

    Ok(Some(user_id))
}
