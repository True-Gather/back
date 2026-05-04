// Librairie centrale du backend.

pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod mail;
pub mod media;
pub mod meetings;
pub mod models;
pub mod redis;
pub mod state;
pub mod webrtc_engine;
pub mod ws;

use axum::{
    Router,
    http::{HeaderValue, Method, header},
};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};

use crate::state::AppState;

// Construit le router Axum principal.
pub fn build_app(state: AppState) -> Router {
    let api_v1 = Router::new()
        .merge(api::routes::router())
        // Routes d'authentification.
        .nest("/auth", auth::routes::router())
        // Routes meetings.
        .merge(meetings::routes::router());

    Router::new()
        .nest("/api/v1", api_v1)
        .nest_service("/uploads", ServeDir::new("uploads"))
        // Route WebSocket de signalisation WebRTC.
        .merge(ws::router())
        .layer(TraceLayer::new_for_http())
        .layer(build_cors_layer(&state.config.frontend.base_url))
        .with_state(state)
}

// CORS pour le frontend local.
fn build_cors_layer(frontend_origin: &str) -> CorsLayer {
    match frontend_origin.parse::<HeaderValue>() {
        Ok(origin) => CorsLayer::new()
            .allow_origin(origin)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::DELETE,
                Method::PUT,
                Method::PATCH,
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
                Method::DELETE,
                Method::PUT,
                Method::PATCH,
                Method::OPTIONS,
            ])
            .allow_headers([
                header::CONTENT_TYPE,
                header::ACCEPT,
                header::AUTHORIZATION,
            ]),
    }
}
