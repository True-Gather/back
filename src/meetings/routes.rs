use axum::{
    routing::{get, post},
    Router,
};

use crate::state::AppState;
use super::handlers::{create_instant_meeting, create_meeting, delete_meeting, get_meeting_detail, list_meetings};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/meetings/instant", post(create_instant_meeting))
        .route("/meetings", post(create_meeting).get(list_meetings))
        .route("/meetings/{id}", get(get_meeting_detail).delete(delete_meeting))
}