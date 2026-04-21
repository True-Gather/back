// Librairie centrale du backend.

pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod mail;
pub mod media;
pub mod models;
pub mod redis;
pub mod state;
pub mod ws;

// Imports nécessaires pour construire le router global.
use axum::{
    http::{header, HeaderValue, Method},
    Router,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::state::AppState;

// Construit le router Axum principal.
pub fn build_app(state: AppState) -> Router {
    // Construction du bloc /api/v1.
    let api_v1 = Router::new()
        // Routes "générales" de l'API.
        .merge(api::routes::router(state.clone()))
        // Routes d'authentification.
        .nest("/auth", auth::routes::router());

    // Construction du router final.
    Router::new()
        .nest("/api/v1", api_v1)
        .layer(TraceLayer::new_for_http())
        .layer(build_cors_layer(&state.config.frontend.base_url))
        .with_state(state)
}

// Construit une couche CORS simple et adaptée au front fourni.
//
// En local, ton front Nuxt sera souvent sur http://localhost:3000.
// Cette fonction autorise explicitement cette origine si elle est valide.
// Sinon, on retombe sur un mode permissif de dev sans credentials.
fn build_cors_layer(frontend_origin: &str) -> CorsLayer {
    // Tentative de parsing de l'origine front.
    match frontend_origin.parse::<HeaderValue>() {
        Ok(origin) => {
            // Cas nominal : on autorise seulement l'origine configurée,
            // avec credentials activés et une liste explicite de headers.
            CorsLayer::new()
                .allow_origin(origin)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers([
                    header::CONTENT_TYPE,
                    header::ACCEPT,
                    header::AUTHORIZATION,
                ])
                .allow_credentials(true)
        }
        Err(_) => {
            // Fallback de dev : on autorise tout si la config est invalide.
            //
            // Important :
            // ici on n'active PAS les credentials, sinon la config serait invalide.
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers([
                    header::CONTENT_TYPE,
                    header::ACCEPT,
                    header::AUTHORIZATION,
                ])
        }
    }
}