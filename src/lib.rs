// Librairie centrale du backend.

pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod mail;
pub mod media;
pub mod meetings;   // ← NOUVEAU
pub mod models;
pub mod redis;
pub mod state;
pub mod webrtc_engine;
pub mod ws;

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
    let api_v1 = Router::new()
        // Routes générales (health, etc.)
        .merge(api::routes::router())
        // Routes auth
        .nest("/auth", auth::routes::router())
        // Routes meetings ← NOUVEAU
        .merge(meetings::routes::router());

    Router::new()
        .nest("/api/v1", api_v1)
        .layer(TraceLayer::new_for_http())
        .layer(build_cors_layer(&state.config.frontend.base_url))
        .with_state(state)
}

fn build_cors_layer(frontend_origin: &str) -> CorsLayer {
    match frontend_origin.parse::<HeaderValue>() {
        Ok(origin) => CorsLayer::new()
            .allow_origin(origin)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::DELETE,  // ← NOUVEAU (pour DELETE /meetings/:id)
                Method::OPTIONS,
            ])
            .allow_headers([
                header::CONTENT_TYPE,
                header::ACCEPT,
                header::AUTHORIZATION,
            ])
            .allow_credentials(true),
        Err(_) => CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::DELETE,  // ← NOUVEAU
                Method::OPTIONS,
            ])
            .allow_headers([
                header::CONTENT_TYPE,
                header::ACCEPT,
                header::AUTHORIZATION,
            ]),
    }
}