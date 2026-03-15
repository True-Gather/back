// État partagé de l'application.
use std::time::Duration;

// Import de la configuration.
use crate::config::AppConfig;

// État partagé principal.
#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub http_client: reqwest::Client,
}

// Implémentation du state.
impl AppState {
    pub fn new(config: AppConfig) -> Result<Self, reqwest::Error> {
        let http_client = reqwest::Client::builder()
            .user_agent("back/0.1.0")
            .timeout(Duration::from_secs(15))
            .build()?;

        Ok(Self {
            config,
            http_client,
        })
    }
}