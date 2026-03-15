// Configuration centralisée du backend.

use serde::Deserialize;

// Configuration racine de l'application.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    // Configuration du serveur HTTP.
    pub server: ServerConfig,
    // Configuration liée au frontend Nuxt.
    pub frontend: FrontendConfig,
    // Configuration Keycloak / OIDC.
    pub keycloak: KeycloakConfig,
    // Configuration applicative liée à l'auth.
    pub auth: AuthConfig,
}

// Configuration du serveur HTTP.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    // Hôte d'écoute.
    pub host: String,
    // Port d'écoute.
    pub port: u16,
}

// Configuration liée au frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct FrontendConfig {
    pub base_url: String,
}

// Configuration liée à Keycloak.
#[derive(Debug, Clone, Deserialize)]
pub struct KeycloakConfig {
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

// Implémentation des helpers de configuration.
impl AppConfig {
    // Charge la configuration depuis l'environnement.
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let _ = dotenvy::dotenv();

        // Construction d'une config avec valeurs par défaut.
        let settings = config::Config::builder()
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 8080)?
            .set_default("frontend.base_url", "http://localhost:3000")?
            .set_default("keycloak.issuer_url", "http://localhost:8081/realms/truegather")?
            .set_default("keycloak.client_id", "truegather-backend")?
            .set_default("auth.cookie_name", "tg_session")?
            .set_default("auth.cookie_secure", false)?
            .add_source(config::Environment::with_prefix("APP").separator("__"))
            .build()?;

        settings.try_deserialize::<Self>()
    }

    pub fn server_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }

    pub fn auth_callback_url(&self) -> String {
        format!(
            "http://{}:{}/api/v1/auth/callback",
            self.server.host, self.server.port
        )
    }

    pub fn frontend_post_login_url(&self) -> String {
        format!("{}/meetings/create", self.frontend.base_url)
    }
}