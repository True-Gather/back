# Truegather - Backend

Backend du projet Truegather, construit en **Rust** avec le framework **Axum**.

---

## Prérequis

| Outil | Rôle | Installation |
|-------|------|-------------|
| **Rust + Cargo** | Compilateur et gestionnaire de paquets | [rustup.rs](https://rustup.rs/) |
| **Docker + Docker Compose** | Infrastructure locale (Keycloak, Redis) | [docs.docker.com](https://docs.docker.com/get-docker/) |
| **Git** | Gestion de version | Fourni par votre OS |

Rust 1.85+ est requis (édition 2024). Vérifiez votre version :
```bash
rustc --version   # doit afficher 1.85 ou supérieur
cargo --version
```

Pour mettre à jour Rust :
```bash
rustup update stable
```

---

## Architecture des sources

```
src/
├── main.rs              Point d'entrée, initialisation du serveur
├── lib.rs               Router global (build_app)
├── config.rs            Configuration centralisée (AppConfig)
├── state.rs             État partagé injecté dans les handlers (AppState)
├── error.rs             Types d'erreurs applicatives
├── api/                 Routes et handlers REST généraux
├── auth/                OIDC, session, middleware d'authentification
├── models/              Modèles de données (User, Meeting…)
├── redis/               Pool Redis + helpers rooms de signalisation
├── webrtc_engine/       Config ICE/TURN, validation SDP, credentials temporels
└── ws/                  Serveur de signalisation WebRTC P2P (WebSocket)

tests/
└── signaling.rs         Tests d'intégration du flux de signalisation WebSocket
```

---

## Mise en place de l'environnement

### 1. Variables d'environnement

Copiez le fichier d'exemple et ajustez les valeurs :
```bash
cp .env.example .env
```

Le fichier `.env` complet pour le développement local :
```env
# Serveur
APP_SERVER__HOST=0.0.0.0
APP_SERVER__PORT=8080

# URLs
APP_BACKEND__BASE_URL=http://localhost:8080
APP_FRONTEND__BASE_URL=http://localhost:3000

# Keycloak
APP_KEYCLOAK__ISSUER_URL=http://localhost:8081/realms/truegather
APP_KEYCLOAK__CLIENT_ID=truegather-backend

# Auth
APP_AUTH__COOKIE_NAME=tg_session
APP_AUTH__COOKIE_SECURE=false

# Redis
APP_REDIS__URL=redis://127.0.0.1:6379

# TURN / ICE (STUN public par défaut, TURN optionnel)
APP_TURN__STUN_URLS=["stun:stun.l.google.com:19302","stun:stun1.l.google.com:19302"]
# APP_TURN__URL=turn:votre-serveur-turn:3478
# APP_TURN__SECRET=votre-secret-turn
# APP_TURN__TTL_SECS=3600

# Logs (format : crate=niveau)
RUST_LOG=info,truegather_backend=debug,tower_http=info
```

> Les variables utilisent le préfixe `APP_` et le séparateur `__` (double underscore) entre les niveaux.
> Exemple : `APP_KEYCLOAK__ISSUER_URL` correspond à `config.keycloak.issuer_url`.

### 2. Infrastructure locale avec Docker Compose

Lance Keycloak et Redis (nécessaires pour faire tourner le serveur) :
```bash
docker compose up -d
```

Attendez que Keycloak soit prêt (environ 30 secondes) :
```bash
docker compose logs -f keycloak
# Ctrl+C quand vous voyez "Started" dans les logs
```

Vérifiez que Redis répond :
```bash
docker compose exec redis redis-cli ping
# Doit afficher : PONG
```

---

## Lancer le serveur

### Mode développement (compilation + exécution en une commande)
```bash
cargo run
```

Le serveur démarre sur `http://0.0.0.0:8080` par défaut (configurable via `APP_SERVER__PORT`).

Lors du premier lancement, la compilation peut prendre plusieurs minutes à cause des dépendances
(webrtc, openidconnect, etc.). Les lancements suivants sont quasi-instantanés.

### Avec des logs verbeux
```bash
RUST_LOG=debug cargo run
```

### Exécuter le binaire directement (après `cargo build`)
```bash
./target/debug/truegather-backend
```

---

## Compilation

### Vérifier que le code compile (sans produire de binaire)
```bash
cargo check
```

### Compilation en mode développement
```bash
cargo build
# Le binaire est produit dans : target/debug/truegather-backend
```

### Compilation en mode production (optimisée)
```bash
cargo build --release
# Le binaire est produit dans : target/release/truegather-backend
```

### Linter
```bash
cargo clippy
```

### Formater le code
```bash
cargo fmt
```

---

## Tests

L'infrastructure Docker doit tourner pour les tests d'intégration (Redis requis).

### Tous les tests (unitaires + intégration)
```bash
cargo test
```

### Afficher les `println!` / logs dans les tests
```bash
cargo test -- --nocapture
```

### Tests unitaires uniquement (sans Redis)
Ces tests vérifient la logique interne de `webrtc_engine` : validation SDP, candidats ICE,
génération des credentials TURN temporels.
```bash
cargo test --lib
```

### Tests d'intégration uniquement (WebSocket de signalisation)
Ces tests démarrent un vrai serveur Axum sur un port aléatoire et vérifient le flux complet
de signalisation WebRTC (join, peer_joined, offer, erreur SDP invalide).
**Redis doit être accessible sur `127.0.0.1:6379`.**
```bash
cargo test --test signaling
```

### Un test précis
```bash
cargo test nom_du_test
# Exemple :
cargo test sdp_valide_accepte
cargo test deux_pairs_room_notifications_croisees -- --nocapture
```

### Liste de tous les tests disponibles
```bash
cargo test -- --list
```

### Résumé des tests existants

| Suite | Fichier | Tests |
|-------|---------|-------|
| `webrtc_engine` (unitaires) | `src/webrtc_engine/mod.rs` | `sdp_valide_accepte`, `sdp_vide_refuse`, `sdp_corrompu_refuse`, `ice_candidate_valide_accepte`, `ice_candidate_vide_refuse`, `turn_credentials_format`, `turn_credentials_different_a_chaque_appel`, `turn_credentials_different_si_secret_different`, `build_ice_servers_sans_turn`, `build_ice_servers_avec_turn` |
| `signaling` (intégration) | `tests/signaling.rs` | `join_room_recoit_joined_avec_ice_servers`, `deux_pairs_room_notifications_croisees`, `offer_valide_routee_vers_pair_cible`, `offer_sdp_invalide_renvoie_erreur_a_lemeteur` |

---

## Docker et Production

### Construire l'image de production
```bash
docker build -t truegather-backend .
```

### Lancer le conteneur backend via Docker Compose
```bash
# Arrêter le `cargo run` en cours si besoin (Ctrl+C), puis :
docker compose up -d backend
```

Le `Dockerfile` utilise un build multi-étapes :
1. **Builder** : image Rust officielle pour compiler le binaire.
2. **Runtime** : image Debian minimale + binaire uniquement — image finale légère et sécurisée.

---

## CI / CD

Le fichier `.github/workflows/ci-cd.yml` déclenche automatiquement sur chaque push `main` :
1. `cargo fmt --check` — vérifie le formatage
2. `cargo clippy` — linter
3. `cargo test` — tous les tests
4. Build + push de l'image Docker sur DockerHub (si tout est vert)

---

## Commandes du quotidien

```bash
cargo check                    # Vérifie la compilation (rapide)
cargo clippy                   # Linter
cargo fmt                      # Formater le code
cargo test                     # Tous les tests
cargo test --lib               # Tests unitaires uniquement (sans Redis)
cargo test -- --nocapture      # Tests avec affichage des logs
docker compose up -d           # Démarrer l'infrastructure
docker compose down            # Arrêter l'infrastructure
docker compose logs -f         # Suivre les logs des conteneurs
```

| Outil | Rôle | Installation |
|-------|------|-------------|
| **Rust + Cargo** | Compilateur et gestionnaire de paquets | [rustup.rs](https://rustup.rs/) |
| **Docker + Docker Compose** | Infrastructure locale (Keycloak, Redis) | [docs.docker.com](https://docs.docker.com/get-docker/) |
| **Git** | Gestion de version | Fourni par votre OS |

Rust 1.85+ est requis (édition 2024). Vérifiez votre version :
```bash
rustc --version   # doit afficher 1.85 ou supérieur
cargo --version
```

Pour mettre à jour Rust :
```bash
rustup update stable
```

---

## Mise en place de l'environnement

### 1. Variables d'environnement

Copiez le fichier d'exemple et ajustez les valeurs :
```bash
cp .env.example .env
```

Le fichier `.env` complet pour le développement local :
```env
# Serveur
APP_SERVER__HOST=0.0.0.0
APP_SERVER__PORT=8080

# URLs
APP_BACKEND__BASE_URL=http://localhost:8080
APP_FRONTEND__BASE_URL=http://localhost:3000

# Keycloak
APP_KEYCLOAK__ISSUER_URL=http://localhost:8081/realms/truegather
APP_KEYCLOAK__CLIENT_ID=truegather-backend

# Auth
APP_AUTH__COOKIE_NAME=tg_session
APP_AUTH__COOKIE_SECURE=false

# Redis
APP_REDIS__URL=redis://127.0.0.1:6379

# TURN / ICE (STUN public par défaut, TURN optionnel)
APP_TURN__STUN_URLS=["stun:stun.l.google.com:19302","stun:stun1.l.google.com:19302"]
# APP_TURN__URL=turn:votre-serveur-turn:3478
# APP_TURN__SECRET=votre-secret-turn
# APP_TURN__TTL_SECS=3600

# Logs (format : crate=niveau)
RUST_LOG=info,truegather_backend=debug,tower_http=info
```

> Les variables utilisent le préfixe `APP_` et le séparateur `__` (double underscore) entre les niveaux.
> Exemple : `APP_KEYCLOAK__ISSUER_URL` correspond à `config.keycloak.issuer_url`.

### 2. Infrastructure locale avec Docker Compose

Lance Keycloak et Redis (nécessaires pour faire tourner le serveur) :
```bash
docker compose up -d
```

Attendez que Keycloak soit prêt (environ 30 secondes) :
```bash
docker compose logs -f keycloak
# Ctrl+C quand vous voyez "Started" dans les logs
```

Vérifiez que Redis répond :
```bash
docker compose exec redis redis-cli ping
# Doit afficher : PONG
```

---

## Compilation

### Vérifier que le code compile (sans produire de binaire)
```bash
cargo check
```

### Compilation en mode développement
```bash
cargo build
# Le binaire est produit dans : target/debug/truegather-backend
```

### Compilation en mode production (optimisée)
```bash
cargo build --release
# Le binaire est produit dans : target/release/truegather-backend
```

### Linter
```bash
cargo clippy
```

### Formater le code
```bash
cargo fmt
```

---

## Lancer le serveur

### Mode développement (compilation + exécution en une commande)
```bash
cargo run
```

Le serveur démarre sur `http://0.0.0.0:8080` par défaut (configurable via `APP_SERVER__PORT`).

Lors du premier lancement, la compilation peut prendre plusieurs minutes à cause des dépendances
(webrtc, openidconnect, etc.). Les lancements suivants sont quasi-instantanés.

### Avec des logs verbeux
```bash
RUST_LOG=debug cargo run
```

### Exécuter le binaire directement (après `cargo build`)
```bash
./target/debug/truegather-backend
```

---

## Tests

L'infrastructure Docker doit tourner pour les tests d'intégration (Redis requis).

### Tous les tests (unitaires + intégration)
```bash
cargo test
```

### Afficher les `println!` / logs dans les tests
```bash
cargo test -- --nocapture
```

### Tests unitaires uniquement (sans Redis)
Ces tests vérifient la logique interne de `webrtc_engine` : validation SDP, candidats ICE,
génération des credentials TURN temporels.
```bash
cargo test --lib
```

### Tests d'intégration uniquement (WebSocket de signalisation)
Ces tests démarrent un vrai serveur Axum sur un port aléatoire et vérifient le flux complet
de signalisation WebRTC (join, peer_joined, offer, erreur SDP invalide).
**Redis doit être accessible sur `127.0.0.1:6379`.**
```bash
cargo test --test signaling
```

### Un test précis
```bash
cargo test nom_du_test
# Exemple :
cargo test sdp_valide_accepte
cargo test deux_pairs_room_notifications_croisees -- --nocapture
```

### Liste de tous les tests disponibles
```bash
cargo test -- --list
```

### Résumé des tests existants

| Suite | Fichier | Tests |
|-------|---------|-------|
| `webrtc_engine` (unitaires) | `src/webrtc_engine/mod.rs` | `sdp_valide_accepte`, `sdp_vide_refuse`, `sdp_corrompu_refuse`, `ice_candidate_valide_accepte`, `ice_candidate_vide_refuse`, `turn_credentials_format`, `turn_credentials_different_a_chaque_appel`, `turn_credentials_different_si_secret_different`, `build_ice_servers_sans_turn`, `build_ice_servers_avec_turn` |
| `signaling` (intégration) | `tests/signaling.rs` | `join_room_recoit_joined_avec_ice_servers`, `deux_pairs_room_notifications_croisees`, `offer_valide_routee_vers_pair_cible`, `offer_sdp_invalide_renvoie_erreur_a_lemeteur` |

---

## Docker et Production

### Construire l'image de production
```bash
docker build -t truegather-backend .
```

### Lancer le conteneur backend via Docker Compose
```bash
# Arrêter le `cargo run` en cours si besoin (Ctrl+C), puis :
docker compose up -d backend
```

Le `Dockerfile` utilise un build multi-étapes :
1. **Builder** : image Rust officielle pour compiler le binaire.
2. **Runtime** : image Debian minimale + binaire uniquement — image finale légère et sécurisée.

---

## Architecture des sources

```
src/
├── main.rs              Point d'entrée, initialisation du serveur
├── lib.rs               Router global (build_app)
├── config.rs            Configuration centralisée (AppConfig)
├── state.rs             État partagé injecté dans les handlers (AppState)
├── error.rs             Types d'erreurs applicatives
├── api/                 Routes et handlers REST généraux
├── auth/                OIDC, session, middleware d'authentification
├── models/              Modèles de données (User, Meeting…)
├── redis/               Pool Redis + helpers rooms de signalisation
├── webrtc_engine/       Config ICE/TURN, validation SDP, credentials temporels
└── ws/                  Serveur de signalisation WebRTC P2P (WebSocket)

tests/
└── signaling.rs         Tests d'intégration du flux de signalisation WebSocket
```

---

## CI / CD

Le fichier `.github/workflows/ci-cd.yml` déclenche automatiquement sur chaque push `main` :
1. `cargo fmt --check` — vérifie le formatage
2. `cargo clippy` — linter
3. `cargo test` — tous les tests
4. Build + push de l'image Docker sur DockerHub (si tout est vert)

---

## Commandes du quotidien

```bash
cargo check                    # Vérifie la compilation (rapide)
cargo clippy                   # Linter
cargo fmt                      # Formater le code
cargo test                     # Tous les tests
cargo test --lib               # Tests unitaires uniquement (sans Redis)
cargo test -- --nocapture      # Tests avec affichage des logs
docker compose up -d           # Démarrer l'infrastructure
docker compose down            # Arrêter l'infrastructure
docker compose logs -f         # Suivre les logs des conteneurs
```
