use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};

use crate::state::AppState;

pub async fn require_session(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Lire le session id depuis le cookie tg_session
    let session_id = crate::auth::session::extract_session_id_from_headers(
        req.headers(),
        &state.config.auth.cookie_name,
    )
    .ok_or(StatusCode::UNAUTHORIZED)?;

    // Lire session stockée en mémoire
    let session = {
        let sessions = state.sessions.read().await;
        sessions.get(&session_id).cloned()
    }
    .ok_or(StatusCode::UNAUTHORIZED)?;

    // Injecter session pour handlers
    req.extensions_mut().insert(session);

    Ok(next.run(req).await)
}
