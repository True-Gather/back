<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Mot de passe oublié - TrueGather</title>
  <link rel="stylesheet" href="${url.resourcesPath}/css/truegather.css">
</head>
<body>
  <div class="tg-auth-layout">
    <div class="tg-auth-background-orb tg-auth-background-orb--one"></div>
    <div class="tg-auth-background-orb tg-auth-background-orb--two"></div>

    <main class="tg-auth-main">
      <div class="tg-auth-container">
        <div class="tg-auth-brand-center">
          <div class="tg-auth-brand-icon">T</div>
          <div class="tg-auth-brand-copy">
            <span class="tg-auth-brand-name">TrueGather</span>
            <span class="tg-auth-brand-subtitle">Plateforme sécurisée de visioconférence</span>
          </div>
        </div>

        <section class="tg-auth-simple-page">
          <div class="tg-auth-simple-card">
            <span class="tg-auth-chip">Réinitialisation</span>

            <h1 class="tg-auth-title tg-auth-title--sm">Mot de passe oublié ?</h1>

            <p class="tg-auth-description tg-auth-description--sm">
              Renseignez votre adresse e-mail pour recevoir un lien de réinitialisation.
            </p>

            <#if message?has_content>
              <div class="tg-auth-feedback <#if message.type == 'error'>tg-auth-feedback--error<#else>tg-auth-feedback--success</#if>">
                ${message.summary}
              </div>
            </#if>

            <form id="kc-reset-password-form" class="tg-auth-form" action="${url.loginAction}" method="post">
              <div class="tg-auth-field">
                <label for="username">Adresse e-mail</label>
                <div class="tg-auth-input-wrap">
                  <span class="tg-auth-input-icon">✉</span>
                  <input
                    id="username"
                    name="username"
                    type="text"
                    value="${auth.attemptedUsername!''}"
                    placeholder="Ex : sophie.martin@entreprise.com"
                    autocomplete="email"
                    autofocus
                  />
                </div>
              </div>

              <button class="tg-auth-primary-btn" type="submit">
                Envoyer le lien
              </button>

              <p class="tg-auth-bottom-text">
                <a href="${url.loginUrl}" class="tg-auth-inline-link">
                  Retour à la connexion
                </a>
              </p>
            </form>
          </div>
        </section>
      </div>
    </main>
  </div>
</body>
</html>