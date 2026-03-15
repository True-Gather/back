// Déclaration des routes génériques de l'API.

use axum::{
    routing::{get, post},
    Router,
};

// Import des handlers.
use crate::api::handlers;

// Construit le router API générique.
pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/meetings", post(handlers::create_meeting))
}