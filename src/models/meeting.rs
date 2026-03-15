// Modèles liés aux meetings.

use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

// Réponse standard du healthcheck.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
}

// Payload de création de meeting.
#[derive(Debug, Deserialize, Validate)]
pub struct CreateMeetingRequest {
    #[validate(length(min = 1, max = 255, message = "Meeting title is required"))]
    pub title: String,

    #[validate(length(min = 1, message = "At least one participant email is required"))]
    #[validate(custom(function = "validate_email_list"))]
    pub participant_emails: Vec<String>,
}

// Réponse minimale de création de meeting.
#[derive(Debug, Serialize)]
pub struct CreateMeetingResponse {
    pub message: String,
    pub title: String,
    pub participants_count: usize,
}

// Structure locale utilisée pour valider un email avec le crate validator.
#[derive(Debug, Validate)]
struct EmailCheck {
    #[validate(email(message = "Invalid participant email"))]
    email: String,
}

// Valide qu'une liste d'emails est correcte.
fn validate_email_list(emails: &[String]) -> Result<(), ValidationError> {
    // On valide chaque email de la liste.
    for email in emails {
        // Construction d'un wrapper validable.
        let candidate = EmailCheck {
            email: email.clone(),
        };

        // Si un email est invalide, on renvoie une erreur.
        if candidate.validate().is_err() {
            return Err(ValidationError::new("invalid_email"));
        }
    }

    // Tout est valide.
    Ok(())
}