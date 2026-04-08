<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Connexion - TrueGather</title>
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
            <span class="tg-auth-chip">Connexion</span>

            <h1 class="tg-auth-title tg-auth-title--sm">Connexion</h1>

            <p class="tg-auth-description tg-auth-description--sm">
              Accédez à votre espace TrueGather
            </p>

            <#if message?has_content>
              <div class="tg-auth-feedback <#if message.type == 'error'>tg-auth-feedback--error<#else>tg-auth-feedback--success</#if>">
                ${message.summary}
              </div>
            </#if>

            <form id="kc-form-login" class="tg-auth-form" action="${url.loginAction}" method="post">
              <div class="tg-auth-field">
                <label for="username">Adresse e-mail</label>
                <div class="tg-auth-input-wrap">
                  <span class="tg-auth-input-icon">✉</span>
                  <input
                    id="username"
                    name="username"
                    type="text"
                    value="${login.username!''}"
                    placeholder="Ex : sophie.martin@entreprise.com"
                    autocomplete="username"
                    autofocus
                  />
                </div>
              </div>

              <div class="tg-auth-field">
                <label for="password">Mot de passe</label>
                <div class="tg-auth-input-wrap tg-auth-input-wrap--password">
                  <span class="tg-auth-input-icon">🔒</span>
                  <input
                    id="password"
                    name="password"
                    type="password"
                    placeholder="Entrez votre mot de passe"
                    autocomplete="current-password"
                  />
                  <button
                    type="button"
                    class="tg-password-toggle"
                    aria-label="Afficher ou masquer le mot de passe"
                    onclick="togglePassword('password', this)"
                  >
                    👁️
                  </button>
                </div>
              </div>

              <div class="tg-auth-links">
                <#if realm.resetPasswordAllowed>
                  <a href="${url.loginResetCredentialsUrl}" class="tg-auth-secondary-link">
                    Mot de passe oublié ?
                  </a>
                </#if>
              </div>

              <button class="tg-auth-primary-btn" type="submit">
                Se connecter
              </button>

              <#if realm.rememberMe && !usernameEditDisabled??>
                <div class="tg-auth-checkbox-row">
                  <label class="tg-auth-checkbox-label" for="rememberMe">
                    <input id="rememberMe" name="rememberMe" type="checkbox" <#if login.rememberMe??>checked</#if> />
                    <span>Se souvenir de moi</span>
                  </label>
                </div>
              </#if>

              <#if realm.registrationAllowed>
                <p class="tg-auth-bottom-text">
                  Pas de compte ?
                  <a href="${url.registrationUrl}" class="tg-auth-inline-link">
                    Créer un compte
                  </a>
                </p>
              </#if>
            </form>
          </div>
        </section>
      </div>
    </main>
  </div>

  <script>
    function togglePassword(inputId, button) {
      const input = document.getElementById(inputId);
      if (!input) return;

      if (input.type === 'password') {
        input.type = 'text';
        button.textContent = '🙈';
      } else {
        input.type = 'password';
        button.textContent = '👁️';
      }
    }
  </script>
</body>
</html>