use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub backend: BackendConfig,
    pub frontend: FrontendConfig,
    pub keycloak: KeycloakConfig,
    pub auth: AuthConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackendConfig {
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FrontendConfig {
    pub base_url: String,
}

// Configuration liée à Keycloak.
#[derive(Debug, Clone, Deserialize)]
pub struct KeycloakConfig {
    pub issuer_url_internal: String,
    pub issuer_url_public: String,
    pub client_id: String,
    pub client_secret: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub cookie_name: String,
    pub cookie_secure: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let _ = dotenvy::dotenv();

        let settings = config::Config::builder()
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 8080)?
            .set_default("backend.base_url", "http://localhost:8080")?
            .set_default("frontend.base_url", "http://localhost:3000")?
            .set_default("keycloak.issuer_url_internal", "http://keycloak:8080/realms/truegather")?
            .set_default("keycloak.issuer_url_public", "http://localhost:8081/realms/truegather")?
            .set_default("keycloak.client_id", "truegather-backend")?
            .set_default("auth.cookie_name", "tg_session")?
            .set_default("auth.cookie_secure", false)?
            .set_default("database.url", "postgres://tg_user:tg_password@localhost:5434/truegather")?
            .add_source(config::Environment::with_prefix("APP").separator("__"))
            .build()?;

        settings.try_deserialize::<Self>()
    }

    pub fn server_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }

    pub fn auth_callback_url(&self) -> String {
        format!("{}/api/v1/auth/callback", self.backend.base_url)
    }

    pub fn frontend_post_login_url(&self) -> String {
        self.frontend.base_url.clone()
    }

    pub fn frontend_post_logout_url(&self) -> String {
        format!("{}/", self.frontend.base_url.trim_end_matches('/'))
    }
}