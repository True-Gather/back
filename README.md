# TrueGather Backend

Backend API de TrueGather

---

## 🚀 Stack technique

- Rust (Axum)
- Keycloak (OIDC / OAuth2)
- Docker
- Tokio (async runtime)
- Reqwest (HTTP client)

---

## 📁 Structure du projet

back/
├── src/ # Code backend Rust
├── keycloak/
│ ├── import/ # Realm exporté (auto-import)
│ └── themes/ # Thème custom Keycloak
├── Cargo.toml
├── .env.example
└── README.md

---

## ⚙️ Prérequis

- Rust installé
- Docker installé
- Node (pour le frontend si besoin)

---

## 🔐 Lancer Keycloak

Depuis le dossier racine du projet :

```bash
docker run --name keycloak \
  -p 127.0.0.1:8081:8080 \
  -e KC_BOOTSTRAP_ADMIN_USERNAME=admin \
  -e KC_BOOTSTRAP_ADMIN_PASSWORD=admin \
  -e KC_HOSTNAME=http://localhost:8081 \
  -v $(pwd)/../keycloak-data:/opt/keycloak/data \
  -v $(pwd)/keycloak/import:/opt/keycloak/data/import \
  -v $(pwd)/keycloak/themes:/opt/keycloak/themes \
  quay.io/keycloak/keycloak:26.5.5 \
  start-dev --import-realm

```

  ## Accès admin
  http://localhost:8081/admin

  ## Identifiants : 
  admin / admin

  ## ⚙️ Configuration backend
  ```bash
    cp .env.example .env

  ```
  ## ▶️ Lancer le backend
    ```bash
      cargo run
    
    ```

  ## 🔑 Authentification
  ## Flow utilisé :
  ## Authorization Code Flow + PKCE
  ## Géré via Keycloak
  ## Endpoints principaux :

    GET /api/v1/auth/login
    GET /api/v1/auth/register
    GET /api/v1/auth/callback
    GET /api/v1/auth/me
  
  ## 🧪 Test rapide
  ## Aller sur :
    http://localhost:8080/api/v1/auth/login

  ## Se connecter via Keycloak
  ## Vérifier :
    http://localhost:8080/api/v1/auth/me

  ## 📦 Notes importantes
  ## Le dossier keycloak-data n’est pas versionné
  ## Le realm est partagé via :
    keycloak/import/truegather-realm.json

  ## Le thème custom est dans :
    keycloak/themes/truegather