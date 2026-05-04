use lettre::{
    message::{header::ContentType, Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use tracing::{info, warn};

use crate::config::SmtpConfig;

pub struct MailService {
    transport: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from: String,
}

impl MailService {
    pub fn new(config: &SmtpConfig) -> Self {
        if !config.is_configured() {
            warn!("SMTP non configuré — les emails ne seront pas envoyés.");
            return Self { transport: None, from: String::new() };
        }

        let creds = Credentials::new(
            config.username.clone().unwrap(),
            config.password.clone().unwrap(),
        );

        let host = config.host.clone().unwrap();
        let port = config.port.unwrap_or(587);

        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
            .unwrap()
            .port(port)
            .credentials(creds)
            .build();

        let from = config
            .from
            .clone()
            .unwrap_or_else(|| format!("TrueGather <{}>", config.username.clone().unwrap()));

        Self { transport: Some(transport), from }
    }

    pub async fn send_meeting_invitation(
        &self,
        to_email: &str,
        to_name: Option<&str>,
        host_name: &str,
        meeting_title: &str,
        meeting_link: &str,
        room_code: &str,
    ) {
        let Some(transport) = &self.transport else {
            info!("Email ignoré (SMTP non configuré) pour {}", to_email);
            return;
        };

        let display_name = to_name.unwrap_or(to_email);

        let html = format!(
            r#"<!DOCTYPE html>
<html lang="fr">
<head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0"></head>
<body style="margin:0;padding:0;background:#f3f4f6;font-family:Arial,sans-serif;">
  <table width="100%" cellpadding="0" cellspacing="0" style="background:#f3f4f6;padding:40px 0;">
    <tr><td align="center">
      <table width="560" cellpadding="0" cellspacing="0" style="background:#ffffff;border-radius:16px;overflow:hidden;box-shadow:0 4px 24px rgba(0,0,0,0.08);">

        <!-- Header -->
        <tr>
          <td style="background:linear-gradient(135deg,#14b8a6,#0891b2);padding:32px;text-align:center;">
            <h1 style="margin:0;color:#ffffff;font-size:26px;font-weight:700;letter-spacing:-0.5px;">TrueGather</h1>
            <p style="margin:8px 0 0;color:rgba(255,255,255,0.85);font-size:14px;">Plateforme de réunions sécurisées</p>
          </td>
        </tr>

        <!-- Body -->
        <tr>
          <td style="padding:40px 40px 32px;">
            <p style="margin:0 0 8px;font-size:16px;color:#374151;">Bonjour <strong>{display_name}</strong>,</p>
            <p style="margin:0 0 24px;font-size:15px;color:#6b7280;line-height:1.6;">
              <strong style="color:#1f2937;">{host_name}</strong> vous invite à rejoindre la réunion suivante :
            </p>

            <!-- Meeting card -->
            <div style="background:#f9fafb;border:1px solid #e5e7eb;border-radius:12px;padding:24px;margin-bottom:28px;">
              <p style="margin:0 0 4px;font-size:13px;color:#9ca3af;text-transform:uppercase;letter-spacing:0.5px;font-weight:600;">Réunion</p>
              <p style="margin:0 0 16px;font-size:20px;font-weight:700;color:#1f2937;">{meeting_title}</p>
              <p style="margin:0;font-size:13px;color:#6b7280;">Code d'accès : <strong style="color:#1f2937;font-family:monospace;font-size:15px;">{room_code}</strong></p>
            </div>

            <!-- CTA -->
            <div style="text-align:center;margin-bottom:28px;">
              <a href="{meeting_link}"
                 style="display:inline-block;background:linear-gradient(135deg,#14b8a6,#0891b2);color:#ffffff;text-decoration:none;padding:14px 36px;border-radius:12px;font-size:16px;font-weight:600;letter-spacing:0.2px;">
                Rejoindre la réunion
              </a>
            </div>

            <p style="margin:0;font-size:13px;color:#9ca3af;line-height:1.6;">
              Ou copiez ce lien dans votre navigateur :<br>
              <span style="color:#0891b2;word-break:break-all;">{meeting_link}</span>
            </p>
          </td>
        </tr>

        <!-- Footer -->
        <tr>
          <td style="background:#f9fafb;border-top:1px solid #e5e7eb;padding:20px 40px;text-align:center;">
            <p style="margin:0;font-size:12px;color:#9ca3af;">
              Cet email a été envoyé automatiquement par TrueGather.<br>
              Si vous n'attendiez pas cette invitation, vous pouvez ignorer cet email.
            </p>
          </td>
        </tr>

      </table>
    </td></tr>
  </table>
</body>
</html>"#,
            display_name = display_name,
            host_name = host_name,
            meeting_title = meeting_title,
            room_code = room_code,
            meeting_link = meeting_link,
        );

        let text = format!(
            "Bonjour {},\n\n{} vous invite à la réunion \"{}\".\n\nCode : {}\n\nLien : {}\n\nCet email a été envoyé automatiquement par TrueGather.",
            display_name, host_name, meeting_title, room_code, meeting_link
        );

        let to_mailbox: Mailbox = match format!("{} <{}>", display_name, to_email).parse() {
            Ok(m) => m,
            Err(_) => match to_email.parse() {
                Ok(m) => m,
                Err(e) => {
                    warn!("Adresse email invalide '{}': {}", to_email, e);
                    return;
                }
            },
        };

        let from_mailbox: Mailbox = match self.from.parse() {
            Ok(m) => m,
            Err(e) => {
                warn!("Adresse expéditeur invalide '{}': {}", self.from, e);
                return;
            }
        };

        let email = match Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(format!("Invitation : {}", meeting_title))
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(text),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html),
                    ),
            ) {
            Ok(e) => e,
            Err(e) => {
                warn!("Erreur construction email pour {}: {}", to_email, e);
                return;
            }
        };

        match transport.send(email).await {
            Ok(_) => info!("Email envoyé à {}", to_email),
            Err(e) => warn!("Échec envoi email à {}: {}", to_email, e),
        }
    }
}
