// État partagé de l'application.
//
// Cet état est clonable et injecté dans les handlers Axum.
// Il contient tout ce qui doit être mutualisé entre les requêtes.

use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};

use tokio::sync::RwLock;
use uuid::Uuid;

// Import de la configuration.
use crate::config::AppConfig;
use crate::models::User;
use chrono::{DateTime, Utc};

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

    // Identifiant stable renvoyé par Keycloak.
    pub keycloak_sub: String,

    // Email utilisateur.
    pub email: String,

    // Nom affiché.
    pub display_name: String,

    // Prénom éventuel.
    pub first_name: Option<String>,

    // Nom éventuel.
    pub last_name: Option<String>,
}

// État partagé principal.
#[derive(Clone)]
pub struct AppState {
    // Configuration globale.
    pub config: AppConfig,

    // Client HTTP partagé pour les appels externes.
    pub http_client: reqwest::Client,

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
}

// Implémentation du state.
impl AppState {
    // Construit un nouvel état partagé.
    pub fn new(config: AppConfig) -> Result<Self, reqwest::Error> {
        // Construction du client HTTP.
        let http_client = reqwest::Client::builder()
            // User-Agent explicite pour identifier l'application.
            .user_agent("truegather-backend/0.1.0")
            // Timeout global raisonnable.
            .timeout(Duration::from_secs(15))
            // Construction du client final.
            .build()?;

        // Retour de l'état prêt à être injecté.
        Ok(Self {
            config,
            http_client,
            pending_auth: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            users: Arc::new(RwLock::new(HashMap::new())),
            users_by_keycloak_sub: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}