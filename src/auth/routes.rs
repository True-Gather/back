// Routes liées à l'authentification.

use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    auth::handlers,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(handlers::start_login))
        .route("/register", get(handlers::start_register))
        .route("/callback", get(handlers::auth_callback))
        .route("/me", get(handlers::me))
        .route("/logout", get(handlers::logout))
        .route("/forgot-password", post(handlers::forgot_password))
        .route("/reset-password", post(handlers::reset_password))
}