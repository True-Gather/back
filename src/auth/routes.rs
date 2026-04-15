// Déclaration des routes auth.

use axum::{
    Router,
    routing::{get, post, put},
};

use crate::auth::handlers;

pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route("/login", get(handlers::start_login))
        .route("/register", get(handlers::start_register))
        .route("/callback", get(handlers::auth_callback))
        .route("/logout", post(handlers::logout))
        .route("/me", get(handlers::me))
        .route("/avatar", put(handlers::update_avatar))
        .route("/password", put(handlers::change_password))
        .route("/forgot-password", post(handlers::forgot_password))
        .route("/reset-password", post(handlers::reset_password))
}
