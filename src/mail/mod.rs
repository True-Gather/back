// Module mail — envoi d'emails transactionnels via Brevo.
//
// Structure :
//   service.rs   → fonction générique send_email + fonctions métier
//   templates.rs → templates HTML des emails

pub mod service;
pub mod templates;

// Ré-export des fonctions publiques pour un accès simplifié depuis l'extérieur.
// Usage : mail::send_password_changed_email(to, username).await
pub use service::send_email;
pub use service::send_password_changed_email;
pub use service::send_profile_changed_email;
pub use service::send_verification_email;
