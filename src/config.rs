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
            // Defaults auth applicative.
            .set_default("auth.cookie_name", "tg_session")?
            .set_default("auth.cookie_secure", false)?
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
        self.frontend.base_url.clone()
    }

    // Retourne l'URL frontend après logout.
    pub fn frontend_post_logout_url(&self) -> String {
        format!("{}/", self.frontend.base_url.trim_end_matches('/'))
    }
}
