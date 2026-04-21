use axum::{
    middleware,
    routing::{get, post},
    Router,
};

use crate::{
    api::handlers,
    auth::middleware::require_session,
    state::AppState,
};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health))
        .route(
            "/meetings",
            post(handlers::create_meeting)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    require_session,
                )),
        )
        .with_state(state)
}