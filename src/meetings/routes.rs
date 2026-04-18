use axum::{
    routing::{delete, post},
    Router,
};

use crate::state::AppState;
use super::handlers::{create_meeting, delete_meeting, list_meetings};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/meetings", post(create_meeting).get(list_meetings))
        .route("/meetings/{id}", delete(delete_meeting))
}