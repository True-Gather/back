use axum::{
    Router,
    routing::get,
};

use crate::{
    api::handlers,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health))
}
