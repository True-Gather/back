// État partagé de l'application.
//
// Cet état est clonable et injecté dans les handlers Axum.
// Il contient tout ce qui doit être mutualisé entre les requêtes.

use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};

use chrono::{DateTime, Utc};
use deadpool_redis::Pool as RedisPool;
use sqlx::PgPool;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::mail::MailService;
use crate::models::User;

// Représente une demande d'authentification en attente.
//
// Cette structure est utilisée pour stocker temporairement
// les données nécessaires au flow OIDC avant le callback Keycloak.
#[derive(Debug, Clone)]
pub struct PendingAuthRequest {
    // Nonce OIDC attendu plus tard dans l'ID token.
    pub nonce: String,

    // PKCE verifier associé à la demande.
    pub pkce_verifier: String,

    // Indique si le flow avait été lancé pour une inscription.
    pub is_registration: bool,

    // Date de création de la demande pour limiter sa durée de vie.
    pub created_at: DateTime<Utc>,
}

// Représente une session applicative locale.
//
// Pour le moment, cette session est stockée en mémoire.
// Plus tard, elle pourra être migrée vers Redis ou une base.
#[derive(Debug, Clone)]
pub struct AppSession {
    // Identifiant interne local de l'utilisateur.
    pub user_id: Uuid,

    // Identifiant stable renvoyé par Keycloak (sub OIDC = keycloak_id en base).
    pub keycloak_id: String,

    // Email utilisateur.
    pub email: String,

    // Nom affiché.
    pub display_name: String,

    // Prénom éventuel.
    pub first_name: Option<String>,

    // Nom éventuel.
    pub last_name: Option<String>,

    // ID token conservé pour pouvoir faire un logout OIDC propre côté Keycloak.
    pub id_token: Option<String>,

    // URL de la photo de profil (base64 data URL ou chemin).
    pub profile_photo_url: Option<String>,
}

// État partagé principal.
#[derive(Clone)]
pub struct AppState {
    // Configuration globale.
    pub config: AppConfig,

    // Client HTTP partagé pour les appels externes.
    pub http_client: reqwest::Client,

    // Pool de connexions PostgreSQL.
    pub db: PgPool,

    // Store mémoire temporaire des flows OIDC en attente.
    //
    // Clé : state OAuth/OIDC.
    pub pending_auth: Arc<RwLock<HashMap<String, PendingAuthRequest>>>,

    // Store mémoire temporaire des sessions applicatives.
    //
    // Clé : session_id.
    pub sessions: Arc<RwLock<HashMap<String, AppSession>>>,

    // Store mémoire des utilisateurs applicatifs locaux.
    //
    // Clé : user_id.
    pub users: Arc<RwLock<HashMap<Uuid, User>>>,

    // Index mémoire pour retrouver rapidement un utilisateur
    // applicatif local depuis le sub Keycloak.
    //
    // Clé : keycloak_sub
    // Valeur : user_id
    pub users_by_keycloak_sub: Arc<RwLock<HashMap<String, Uuid>>>,

    // Pool de connexions Redis (utilisé pour WebRTC et autres features).
    pub redis: RedisPool,

    // Service d'envoi d'emails.
    pub mail: Arc<MailService>,

    // Rooms de signalisation WebRTC actives.
    //
    // Clé externe : room_id
    // Clé interne : user_id
    // Valeur      : sender vers le canal WebSocket du pair
    pub signaling_rooms: SignalingRooms,
}

// Implémentation du state.
impl AppState {
    // Construit un nouvel état partagé.
    pub async fn new(
        config: AppConfig,
        redis: RedisPool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let http_client = reqwest::Client::builder()
            .user_agent("truegather-backend/0.1.0")
            .timeout(Duration::from_secs(15))
            .build()?;

        let db = sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(&config.database.url)
            .await?;

        let mail = Arc::new(MailService::new(&config.smtp));

        Ok(Self {
            config,
            http_client,
            db,
            pending_auth: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            users: Arc::new(RwLock::new(HashMap::new())),
            users_by_keycloak_sub: Arc::new(RwLock::new(HashMap::new())),
            redis,
            mail,
            signaling_rooms: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

// Alias pour la map des rooms de signalisation en mémoire.
//
// Structure : room_id -> { user_id -> sender de messages JSON }
pub type SignalingRooms =
    Arc<RwLock<HashMap<String, HashMap<Uuid, mpsc::UnboundedSender<String>>>>>;
