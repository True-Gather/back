// Déclaration des routes auth.

use axum::{
    routing::{get, post},
    Router,
};

use crate::auth::handlers;

pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route("/login", get(handlers::start_login))
        .route("/register", get(handlers::start_register))
        .route("/callback", get(handlers::auth_callback))
        .route("/logout", post(handlers::logout))
        .route("/me", get(handlers::me))
        .route("/forgot-password", post(handlers::forgot_password))
        .route("/reset-password", post(handlers::reset_password))
}