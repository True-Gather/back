// Exports centralisés des modèles du backend.

pub mod meeting;
pub mod user;

// Re-export des types les plus utiles côté handlers.
pub use meeting::{CreateMeetingRequest, CreateMeetingResponse, HealthResponse};
pub use user::{User, UserProfileView};
