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
    // URL publique (navigateur → Keycloak) : ex. http://localhost:8081/realms/truegather
    pub issuer_url: String,
    // URL interne (backend → Keycloak) : ex. http://host.docker.internal:8081/realms/truegather
    // Si absent, on retombe sur issuer_url.
    pub issuer_url_internal: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
}

// Configuration auth interne au backend.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub cookie_name: String,
    pub cookie_secure: bool,
}

// Configuration Redis.
#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub url: String,
}

// Configuration PostgreSQL.
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
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

        // Construction d'une config avec valeurs par défaut.
        let settings = config::Config::builder()
            // Defaults serveur.
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 8080)?
            // Default backend public local.
            .set_default("backend.base_url", "http://localhost:8080")?
            // Default frontend local Nuxt.
            .set_default("frontend.base_url", "http://localhost:3000")?
            // Defaults Keycloak.
            .set_default(
                "keycloak.issuer_url",
                "http://localhost:8081/realms/truegather",
            )?
            .set_default("keycloak.client_id", "truegather-backend")?
            .set_default::<_, Option<String>>("keycloak.issuer_url_internal", None)?
            // Defaults auth applicative.
            .set_default("auth.cookie_name", "tg_session")?
            .set_default("auth.cookie_secure", false)?
            // Default Redis local.
            .set_default("redis.url", "redis://127.0.0.1:6379")?
            // Default PostgreSQL local.
            .set_default("database.url", "postgres://tg_user:tg_password@localhost:5434/truegather")?
            // Defaults TURN / ICE.
            // Les URLs STUN sont configurées via APP_TURN__STUN_URLS (liste JSON).
            // En l'absence de variable d'env, on utilise les serveurs STUN Google.
            .set_default(
                "turn.stun_urls",
                vec![
                    "stun:stun.l.google.com:19302",
                    "stun:stun1.l.google.com:19302",
                ],
            )?
            .set_default("turn.ttl_secs", 3600u64)?
            // Surcharge par les variables d'environnement.
            .add_source(config::Environment::with_prefix("APP").separator("__"))
            // Construction finale.
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
