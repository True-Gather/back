// Modèles liés aux meetings.

use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

// Réponse standard du healthcheck.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
}

// Payload de création de meeting depuis la popup dashboard.
#[derive(Debug, Deserialize, Validate)]
pub struct CreateMeetingRequest {
    #[validate(length(min = 1, max = 255, message = "Meeting title is required"))]
    pub title: String,

    #[validate(length(min = 1, message = "At least one participant email is required"))]
    #[validate(custom(function = "validate_email_list"))]
    pub participant_emails: Vec<String>,

    pub group_id: Option<String>,

    pub ai_enabled: bool,
    pub microphone_enabled: bool,
    pub camera_enabled: bool,
}

// Réponse backend après création réelle.
#[derive(Debug, Serialize)]
pub struct CreateMeetingResponse {
    pub meeting_id: String,
    pub title: String,
    pub host_keycloak_id: String,
    pub participants_count: usize,
    pub status: String,
}

// Validation locale email.
#[derive(Debug, Validate)]
struct EmailCheck {
    #[validate(email(message = "Invalid participant email"))]
    email: String,
}

// Validation liste emails.
fn validate_email_list(emails: &[String]) -> Result<(), ValidationError> {
    for email in emails {
        let candidate = EmailCheck {
            email: email.clone(),
        };

        if candidate.validate().is_err() {
            return Err(ValidationError::new("invalid_email"));
        }
    }

    Ok(())
}