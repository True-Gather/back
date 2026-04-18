// Modèles liés aux meetings.
// Les DTOs de création/réponse meeting sont dans meetings/dto.rs.

use serde::Serialize;

// Réponse standard du healthcheck.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
}