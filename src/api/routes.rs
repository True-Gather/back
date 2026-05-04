use axum::{
    Router,
    routing::get,
};

use crate::{
    api::handlers,
    state::AppState,
};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health))
        .with_state(state)
}
