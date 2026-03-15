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
    http::{HeaderValue, Method},
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
        .merge(api::routes::router())
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
fn build_cors_layer(frontend_origin: &str) -> CorsLayer {
    match frontend_origin.parse::<HeaderValue>() {
        Ok(origin) => {
            CorsLayer::new()
                .allow_origin(origin)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers(Any)
        }
        Err(_) => {
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers(Any)
        }
    }
}