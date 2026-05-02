// Service d'envoi d'emails via l'API HTTP Brevo.
//
// Ce module fournit :
//   - `send_email`                  : fonction générique d'envoi
//   - `send_password_changed_email` : fonction métier post-changement de mot de passe
//   - `send_profile_changed_email`  : fonction métier post-mise à jour du profil
//
// La clé API Brevo est lue depuis la variable d'environnement APP__MAIL__BREVO_API_KEY.
// Elle n'est jamais stockée en dur dans le code.
//
// En cas d'échec de l'envoi, l'erreur est loggée mais ne fait PAS échouer
// l'appel métier (l'email est best-effort).

use reqwest::Client;
use serde::Serialize;
use std::time::Duration;
use tracing::{error, warn};

use crate::mail::templates;

// URL de l'API Brevo pour l'envoi d'emails transactionnels.
const BREVO_API_URL: &str = "https://api.brevo.com/v3/smtp/email";

// Timeout HTTP pour les appels vers Brevo (évite de bloquer indéfiniment).
const HTTP_TIMEOUT_SECS: u64 = 10;

// Expéditeur temporaire vérifié dans Brevo — à remplacer par no-reply@truegather.app une fois le domaine configuré.
const SENDER_EMAIL: &str = "alinebenissan@gmail.com";
const SENDER_NAME: &str = "TrueGather";

// ─────────────────────────────────────────────────────────────────────────────
// Structures de sérialisation pour le body JSON de l'API Brevo
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct BrevoSender {
    email: String,
    name: String,
}

#[derive(Serialize)]
struct BrevoRecipient {
    email: String,
}

// Body complet attendu par l'endpoint POST /v3/smtp/email de Brevo.
#[derive(Serialize)]
struct BrevoEmailPayload {
    sender: BrevoSender,
    to: Vec<BrevoRecipient>,
    subject: String,
    #[serde(rename = "htmlContent")]
    html_content: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Fonction générique d'envoi d'email
// ─────────────────────────────────────────────────────────────────────────────

/// Envoie un email via l'API Brevo.
///
/// # Arguments
/// * `to`      - Adresse email du destinataire
/// * `subject` - Sujet de l'email
/// * `html`    - Contenu HTML de l'email
///
/// # Erreurs
/// Retourne une erreur si :
/// - La variable d'environnement `APP__MAIL__BREVO_API_KEY` est absente
/// - L'appel HTTP vers Brevo échoue (réseau, timeout)
/// - Brevo retourne un code d'erreur HTTP (4xx, 5xx)
pub async fn send_email(to: &str, subject: &str, html: &str) -> Result<(), String> {
    // Lecture de la clé API depuis l'environnement — jamais en dur dans le code.
    let api_key = std::env::var("APP__MAIL__BREVO_API_KEY").map_err(|_| {
        "Variable d'environnement APP__MAIL__BREVO_API_KEY manquante".to_string()
    })?;

    // Construction du client HTTP avec timeout pour éviter les blocages.
    let client = Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Impossible de créer le client HTTP : {e}"))?;

    // Construction du payload JSON attendu par l'API Brevo.
    let payload = BrevoEmailPayload {
        sender: BrevoSender {
            email: SENDER_EMAIL.to_string(),
            name: SENDER_NAME.to_string(),
        },
        to: vec![BrevoRecipient {
            email: to.to_string(),
        }],
        subject: subject.to_string(),
        html_content: html.to_string(),
    };

    // Envoi de la requête POST vers l'API Brevo.
    let response = client
        .post(BREVO_API_URL)
        .header("api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Échec de l'appel vers Brevo : {e}"))?;

    // Vérification du code de statut HTTP retourné par Brevo.
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "(corps illisible)".to_string());
        return Err(format!(
            "Brevo a retourné une erreur HTTP {status} : {body}"
        ));
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Fonctions métier
// ─────────────────────────────────────────────────────────────────────────────

/// Envoie un email de confirmation après un changement de mot de passe réussi.
///
/// Cette fonction est **non bloquante sur l'erreur** : si l'envoi échoue
/// (Brevo indisponible, clé manquante...), l'erreur est loggée mais le
/// changement de mot de passe n'est pas annulé.
///
/// # Arguments
/// * `to`       - Adresse email du destinataire
/// * `username` - Prénom ou nom d'affichage de l'utilisateur (personnalisation)
pub async fn send_password_changed_email(to: &str, username: &str) {
    let html = templates::password_changed_template(username);
    let subject = "Votre mot de passe TrueGather a été modifié";

    match send_email(to, subject, &html).await {
        Ok(()) => {
            // Confirmation loggée pour traçabilité.
            tracing::info!(
                email = to,
                "Email de confirmation de changement de mot de passe envoyé"
            );
        }
        Err(e) => {
            // L'erreur est loggée mais ne fait pas échouer l'appel métier.
            // Le changement de mot de passe a déjà réussi côté Keycloak.
            warn!(
                email = to,
                error = %e,
                "Impossible d'envoyer l'email de confirmation de changement de mot de passe"
            );
            // Log supplémentaire en niveau error pour alerter en production.
            error!(
                "Échec envoi email post-password-change vers {} : {}",
                to, e
            );
        }
    }
}

/// Envoie un email de confirmation après une mise à jour du profil réussie.
///
/// Non bloquant sur l'erreur : si l'envoi échoue, la mise à jour du profil
/// n'est pas annulée.
pub async fn send_profile_changed_email(to: &str, username: &str) {
    let html = templates::profile_changed_template(username);
    let subject = "Votre profil TrueGather a été mis à jour";

    match send_email(to, subject, &html).await {
        Ok(()) => {
            tracing::info!(
                email = to,
                "Email de confirmation de mise à jour de profil envoyé"
            );
        }
        Err(e) => {
            warn!(
                email = to,
                error = %e,
                "Impossible d'envoyer l'email de confirmation de mise à jour de profil"
            );
            error!(
                "Échec envoi email post-profile-update vers {} : {}",
                to, e
            );
        }
    }
}

/// Envoie un email de vérification d'adresse email lors de l'inscription.
///
/// Contrairement aux autres fonctions métier, celle-ci **retourne une erreur**
/// si l'envoi échoue — l'inscription ne doit pas se terminer silencieusement
/// sans que l'utilisateur reçoive son lien de vérification.
///
/// # Arguments
/// * `to`         - Adresse email du destinataire
/// * `username`   - Prénom ou nom d'affichage (personnalisation)
/// * `verify_url` - Lien complet de vérification (avec token en clair)
pub async fn send_verification_email(to: &str, username: &str, verify_url: &str) -> Result<(), String> {
    let html = templates::email_verification_template(username, verify_url);
    let subject = "Vérifiez votre adresse email TrueGather";
    send_email(to, subject, &html).await
}
