// Configuration centralisée du backend.
use serde::Deserialize;

// Configuration racine de l'application.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub backend: BackendConfig,
    pub frontend: FrontendConfig,
    pub keycloak: KeycloakConfig,
    pub auth: AuthConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub turn: TurnConfig,
    pub smtp: SmtpConfig,
}

// Configuration du serveur HTTP.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

// Configuration publique du backend.
#[derive(Debug, Clone, Deserialize)]
pub struct BackendConfig {
    pub base_url: String,
}

// Configuration liée au frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct FrontendConfig {
    pub base_url: String,
}

// Configuration liée à Keycloak.
#[derive(Debug, Clone, Deserialize)]
pub struct KeycloakConfig {
    // URL interne Docker (server-to-server) — utilisée par le backend pour les appels OIDC.
    // Valeur par défaut : http://localhost:8081/realms/truegather (Docker Compose internal).
    pub issuer_url_internal: String,
    // URL publique du realm (exposée au navigateur pour les redirections OIDC).
    pub issuer_url_public: String,
    // URL de l'issuer configurée dans les tokens Keycloak — utilisée pour la validation des claims.
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
}

// Configuration auth interne au backend.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub cookie_name: String,
    pub cookie_secure: bool,
}

// Configuration PostgreSQL.
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

// Configuration Redis.
#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub url: String,
}

// Configuration SMTP pour l'envoi d'emails.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SmtpConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: Option<String>,
}

impl SmtpConfig {
    pub fn is_configured(&self) -> bool {
        self.host.is_some() && self.username.is_some() && self.password.is_some()
    }
}

// Configuration TURN / ICE.
//
// `stun_urls`  : liste des serveurs STUN à utiliser (vient de l'env).
// `url`        : URL du serveur TURN (optionnel).
// `secret`     : secret partagé coturn --use-auth-secret (optionnel).
// `ttl_secs`   : durée de validité des credentials TURN temporels.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TurnConfig {
    pub stun_urls: Vec<String>,
    pub url: Option<String>,
    pub secret: Option<String>,
    pub ttl_secs: u64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, config::ConfigError> {
    let _ = dotenvy::dotenv();

    let settings = config::Config::builder()
        .set_default("server.host", "0.0.0.0")?
        .set_default("server.port", 8080)?
        .set_default("backend.base_url", "http://localhost:8080")?
        .set_default("frontend.base_url", "http://localhost:3000")?
        // Keycloak
        .set_default("keycloak.issuer_url", "http://localhost:8081/realms/truegather")?
        .set_default("keycloak.issuer_url_public", "http://localhost:8081/realms/truegather")?
        .set_default("keycloak.issuer_url_internal", "http://localhost:8081/realms/truegather")?
        .set_default("keycloak.client_id", "truegather-backend")?
        .set_default("auth.cookie_name", "tg_session")?
        .set_default("auth.cookie_secure", false)?
        .set_default("database.url", "postgres://tg_user:tg_password@localhost:5434/truegather")?
        .set_default("redis.url", "redis://127.0.0.1:6379")?
        .set_default("turn.stun_urls", vec![
            "stun:stun.l.google.com:19302",
            "stun:stun1.l.google.com:19302",
        ])?
        .set_default("turn.ttl_secs", 3600u64)?
        .add_source(config::Environment::with_prefix("APP").separator("__"))
        .build()?;

    settings.try_deserialize::<Self>()
}

    // Retourne l'adresse complète d'écoute du serveur.
    pub fn server_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }

    // Retourne l'URL publique de callback OIDC.
    pub fn auth_callback_url(&self) -> String {
        format!("{}/api/v1/auth/callback", self.backend.base_url)
    }

    // Retourne l'URL frontend après login réussi.
    pub fn frontend_post_login_url(&self) -> String {
        format!("{}/dashboard", self.frontend.base_url.trim_end_matches('/'))
    }

    // Retourne l'URL frontend après logout.
    pub fn frontend_post_logout_url(&self) -> String {
        format!(
            "{}/", self.frontend.base_url.trim_end_matches('/')
        )
    }
}
