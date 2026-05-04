// Routes liées à l'authentification.

use axum::{
    Router,
    routing::{get, post, put},
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
        .route("/avatar", put(handlers::update_avatar))
        .route("/forgot-password", post(handlers::forgot_password))
        .route("/reset-password", post(handlers::reset_password))
        // Changement de mot de passe pour un utilisateur connecté.
        .route("/password", put(handlers::change_password))
        // Mise à jour du profil (prénom, nom de famille).
        .route("/me", put(handlers::update_profile))
        // Vérification d'email après inscription.
        .route("/verify-email", get(handlers::verify_email))
}
