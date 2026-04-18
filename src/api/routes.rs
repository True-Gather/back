// Déclaration des routes génériques de l'API.
// La route /meetings est désormais dans meetings::routes.

use axum::{routing::get, Router};
use crate::api::handlers;

pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route("/health", get(handlers::health))
}