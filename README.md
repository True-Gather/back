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
- Node 
---

## Lancement du projet :

## Enlever le fichier compose.yaml du dossier back et le mettre a la racine du projet

  docker compose up --build

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

  