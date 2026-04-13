<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Nouveau mot de passe - TrueGather</title>
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
            <span class="tg-auth-chip">Nouveau mot de passe</span>

            <h1 class="tg-auth-title tg-auth-title--sm">Créer un nouveau mot de passe</h1>

            <p class="tg-auth-description tg-auth-description--sm">
              Définissez un nouveau mot de passe pour retrouver l’accès à votre espace.
            </p>

            <#if message?has_content>
              <div class="tg-auth-feedback <#if message.type == 'error'>tg-auth-feedback--error<#else>tg-auth-feedback--success</#if>">
                ${message.summary}
              </div>
            </#if>

            <form id="kc-passwd-update-form" class="tg-auth-form" action="${url.loginAction}" method="post">
              <div class="tg-auth-field">
                <label for="password-new">Mot de passe</label>
                <div class="tg-auth-input-wrap tg-auth-input-wrap--password">
                  <span class="tg-auth-input-icon">🔒</span>
                  <input
                    id="password-new"
                    name="password-new"
                    type="password"
                    placeholder="Entrez votre mot de passe"
                    autocomplete="new-password"
                  />
                  <button
                    type="button"
                    class="tg-password-toggle"
                    aria-label="Afficher ou masquer le mot de passe"
                    onclick="togglePassword('password-new', this)"
                  >
                    👁️
                  </button>
                </div>
                <p class="tg-auth-hint">
                  Le mot de passe doit contenir au moins 14 caractères, avec majuscule,
                  minuscule, chiffre et caractère spécial.
                </p>
              </div>

              <div class="tg-auth-field">
                <label for="password-confirm">Confirmer le mot de passe</label>
                <div class="tg-auth-input-wrap tg-auth-input-wrap--password">
                  <span class="tg-auth-input-icon">🔐</span>
                  <input
                    id="password-confirm"
                    name="password-confirm"
                    type="password"
                    placeholder="Entrez le mot de passe à nouveau"
                    autocomplete="new-password"
                  />
                  <button
                    type="button"
                    class="tg-password-toggle"
                    aria-label="Afficher ou masquer le mot de passe"
                    onclick="togglePassword('password-confirm', this)"
                  >
                    👁️
                  </button>
                </div>
              </div>

              <button class="tg-auth-primary-btn" type="submit">
                Confirmer
              </button>
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