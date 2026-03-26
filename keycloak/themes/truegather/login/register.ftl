<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Inscription - TrueGather</title>
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
          <div class="tg-auth-simple-card tg-auth-simple-card--wide">
            <span class="tg-auth-chip">Inscription</span>

            <h1 class="tg-auth-title tg-auth-title--sm">Créer votre compte</h1>

            <p class="tg-auth-description tg-auth-description--sm">
              Créez votre compte sécurisé TrueGather
            </p>

            <#if message?has_content>
              <div class="tg-auth-feedback <#if message.type == 'error'>tg-auth-feedback--error<#else>tg-auth-feedback--success</#if>">
                ${message.summary}
              </div>
            </#if>

            <form id="kc-register-form" class="tg-auth-form" action="${url.registrationAction}" method="post">
              <div class="tg-auth-grid-2">
                <div class="tg-auth-field">
                  <label for="firstName">Prénom</label>
                  <div class="tg-auth-input-wrap">
                    <span class="tg-auth-input-icon">👤</span>
                    <input
                        id="firstName"
                        name="firstName"
                        type="text"
                        value="<#if register?? && register.formData?? && register.formData.firstName??>${register.formData.firstName}</#if>"
                        placeholder="Ex : Sophie"
                        autocomplete="given-name"
                    />
                  </div>
                </div>

                <div class="tg-auth-field">
                  <label for="lastName">Nom</label>
                  <div class="tg-auth-input-wrap">
                    <span class="tg-auth-input-icon">👤</span>
                    <input
                        id="lastName"
                        name="lastName"
                        type="text"
                        value="<#if register?? && register.formData?? && register.formData.lastName??>${register.formData.lastName}</#if>"
                        placeholder="Ex : Martin"
                        autocomplete="family-name"
                    />
                  </div>
                </div>
              </div>

              <div class="tg-auth-field">
                <label for="email">Adresse e-mail</label>
                <div class="tg-auth-input-wrap">
                  <span class="tg-auth-input-icon">✉</span>
                  <input
                    id="email"
                    name="email"
                    type="email"
                    value="<#if register?? && register.formData?? && register.formData.email??>${register.formData.email}</#if>"
                    placeholder="Ex : sophie.martin@entreprise.com"
                    autocomplete="email"
                />
                </div>
              </div>

              <#if !realm.registrationEmailAsUsername>
                <div class="tg-auth-field">
                  <label for="username">Nom d'utilisateur</label>
                  <div class="tg-auth-input-wrap">
                    <span class="tg-auth-input-icon">👤</span>
                    <input
                        id="username"
                        name="username"
                        type="text"
                        value="${(register.formData.username)!''}"
                        placeholder="Choisissez un identifiant"
                        autocomplete="username"
                    />
                  </div>
                </div>
              </#if>

              <div class="tg-auth-field">
                <label for="password">Mot de passe</label>
                <div class="tg-auth-input-wrap tg-auth-input-wrap--password">
                  <span class="tg-auth-input-icon">🔒</span>
                  <input
                    id="password"
                    name="password"
                    type="password"
                    placeholder="Entrez votre mot de passe"
                    autocomplete="new-password"
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
                <p class="tg-auth-hint">
                    Le mot de passe doit contenir au moins 14 caractères, avec majuscule, minuscule,
                    chiffre et caractère spécial.
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
                Créer un compte
              </button>

              <p class="tg-auth-bottom-text">
                Déjà inscrit ?
                <a href="${url.loginUrl}" class="tg-auth-inline-link">
                  Se connecter
                </a>
              </p>
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