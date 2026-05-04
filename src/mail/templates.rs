// Templates HTML des emails envoyés par TrueGather.
//
// Chaque fonction retourne une chaîne HTML complète prête à être passée
// au champ `htmlContent` de l'API Brevo.
// Les templates sont volontairement simples et lisibles — pas de dépendance
// à un moteur de templates pour éviter la complexité.

/// Template envoyé après un changement de mot de passe réussi.
///
/// L'email informe l'utilisateur que son mot de passe a été modifié.
/// Si le changement n'est pas de son fait, il est invité à contacter le support.
/// Le mot de passe ne figure JAMAIS dans cet email.
pub fn password_changed_template(username: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Changement de mot de passe — TrueGather</title>
  <style>
    body {{
      font-family: 'Segoe UI', Arial, sans-serif;
      background-color: #f4f6f9;
      margin: 0;
      padding: 0;
    }}
    .container {{
      max-width: 560px;
      margin: 40px auto;
      background: #ffffff;
      border-radius: 8px;
      overflow: hidden;
      box-shadow: 0 2px 8px rgba(0,0,0,0.08);
    }}
    .header {{
      background-color: #14b8a6;
      padding: 32px 40px;
      text-align: center;
    }}
    .header h1 {{
      color: #ffffff;
      font-size: 22px;
      margin: 0;
      letter-spacing: 0.5px;
    }}
    .body {{
      padding: 32px 40px;
      color: #374151;
      line-height: 1.6;
    }}
    .body p {{
      margin: 0 0 16px;
    }}
    .alert-box {{
      background-color: #fef3c7;
      border-left: 4px solid #f59e0b;
      border-radius: 4px;
      padding: 14px 18px;
      margin: 24px 0;
      font-size: 14px;
      color: #92400e;
    }}
    .footer {{
      background-color: #f9fafb;
      padding: 20px 40px;
      text-align: center;
      font-size: 12px;
      color: #9ca3af;
      border-top: 1px solid #e5e7eb;
    }}
    .footer a {{
      color: #14b8a6;
      text-decoration: none;
    }}
  </style>
</head>
<body>
  <div class="container">
    <div class="header">
      <h1>🔒 TrueGather</h1>
    </div>
    <div class="body">
      <p>Bonjour <strong>{username}</strong>,</p>
      <p>
        Nous vous confirmons que le mot de passe de votre compte TrueGather
        a été modifié avec succès.
      </p>
      <div class="alert-box">
        ⚠️ <strong>Ce changement ne vient pas de vous ?</strong><br />
        Si vous n'êtes pas à l'origine de cette modification, contactez
        immédiatement notre support à
        <a href="mailto:support@truegather.app">support@truegather.app</a>
        et sécurisez votre compte.
      </div>
      <p>
        Pour toute question, notre équipe est disponible à
        <a href="mailto:support@truegather.app">support@truegather.app</a>.
      </p>
      <p>L'équipe TrueGather</p>
    </div>
    <div class="footer">
      © 2026 TrueGather · Cet email a été envoyé automatiquement, merci de ne pas y répondre.
    </div>
  </div>
</body>
</html>"#,
        username = username
    )
}

/// Template envoyé après une mise à jour du profil (prénom/nom) réussie.
pub fn profile_changed_template(username: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Profil mis à jour — TrueGather</title>
  <style>
    body {{
      font-family: 'Segoe UI', Arial, sans-serif;
      background-color: #f4f6f9;
      margin: 0;
      padding: 0;
    }}
    .container {{
      max-width: 560px;
      margin: 40px auto;
      background: #ffffff;
      border-radius: 8px;
      overflow: hidden;
      box-shadow: 0 2px 8px rgba(0,0,0,0.08);
    }}
    .header {{
      background-color: #14b8a6;
      padding: 32px 40px;
      text-align: center;
    }}
    .header h1 {{
      color: #ffffff;
      font-size: 22px;
      margin: 0;
      letter-spacing: 0.5px;
    }}
    .body {{
      padding: 32px 40px;
      color: #374151;
      line-height: 1.6;
    }}
    .body p {{
      margin: 0 0 16px;
    }}
    .footer {{
      background-color: #f9fafb;
      padding: 20px 40px;
      text-align: center;
      font-size: 12px;
      color: #9ca3af;
      border-top: 1px solid #e5e7eb;
    }}
    .footer a {{
      color: #14b8a6;
      text-decoration: none;
    }}
  </style>
</head>
<body>
  <div class="container">
    <div class="header">
      <h1>👤 TrueGather</h1>
    </div>
    <div class="body">
      <p>Bonjour <strong>{username}</strong>,</p>
      <p>
        Votre profil TrueGather a été mis à jour avec succès.
        Vos nouvelles informations sont maintenant actives sur votre compte.
      </p>
      <p>
        Pour toute question, notre équipe est disponible à
        <a href="mailto:support@truegather.app">support@truegather.app</a>.
      </p>
      <p>L'équipe TrueGather</p>
    </div>
    <div class="footer">
      © 2026 TrueGather · Cet email a été envoyé automatiquement, merci de ne pas y répondre.
    </div>
  </div>
</body>
</html>"#,
        username = username
    )
}
