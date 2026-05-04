# Rapport technique — Backend Truegather & WebRTC

**Date :** 2 avril 2026  
**Périmètre :** Backend Rust/Axum — signalisation WebRTC P2P, sécurité TURN, tests

---

## Vue d'ensemble

On a construit de zéro un **serveur de signalisation WebRTC** intégré au backend Rust existant.

Ce serveur ne transporte **jamais** les flux audio/vidéo. Son seul rôle est d'aider deux navigateurs à s'identifier mutuellement et à s'échanger les informations nécessaires pour établir une connexion directe entre eux (peer-to-peer). Une fois la connexion P2P établie, le backend n'est plus impliqué dans la communication.

---

## Concepts clés à comprendre

### WebRTC — comment ça marche en bref

```
Navigateur A                  Backend (toi)              Navigateur B
     |                              |                          |
     |-- WebSocket /ws/signal ----->|                          |
     |                              |<-- WebSocket /ws/signal -|
     |                              |                          |
     |-- Join room "abc" ---------->|                          |
     |<-- joined (liste des pairs) -|                          |
     |                              |<-- Join room "abc" ------|
     |<-- peer_joined (B est là) ---|-- joined (A est là) ---->|
     |                              |                          |
     |-- Offer SDP --------------->|-- Offer SDP (routée) --->|
     |                              |                          |
     |<-- Answer SDP --------------|<-- Answer SDP -----------|
     |                              |                          |
     |-- ICE candidate ----------->|-- ICE candidate -------->|
     |<-- ICE candidate -----------|<-- ICE candidate --------|
     |                              |                          |
     |<======= Connexion P2P directe, backend hors-jeu ======>|
```

- **SDP (Session Description Protocol)** : décrit les capacités du navigateur (codecs audio/vidéo supportés, réseau). L'Offer vient du pair qui initie, l'Answer vient de celui qui répond.
- **ICE (Interactive Connectivity Establishment)** : mécanisme pour trouver le meilleur chemin réseau entre les deux pairs (connexion directe si possible, via TURN si derrière un NAT strict).
- **STUN** : serveur que le navigateur interroge pour connaître son adresse IP publique.
- **TURN** : relais de dernier recours si les navigateurs ne peuvent pas se connecter directement.

---

## Ce qui a été implémenté

### 1. Signalisation WebSocket — `src/ws/mod.rs`

**Route :** `GET /ws/signal`

#### Authentification avant connexion

Avant même d'upgrader la connexion HTTP en WebSocket, le backend vérifie le cookie `tg_session`. Si la session est absente ou invalide → **401** et la connexion est refusée. Aucun WebSocket non authentifié ne peut exister.

```rust
// Extrait le user_id depuis le cookie de session
async fn resolve_user_id(state: &AppState, headers: &HeaderMap) -> Option<Uuid>
```

#### Messages que le client peut envoyer (JSON)

| Type | Champs | Description |
|------|--------|-------------|
| `join` | `room_id` | Rejoindre une room de signalisation |
| `leave` | `room_id` | Quitter la room |
| `offer` | `room_id`, `to` (UUID), `sdp` | Envoyer une SDP Offer à un pair |
| `answer` | `room_id`, `to` (UUID), `sdp` | Envoyer une SDP Answer à un pair |
| `ice_candidate` | `room_id`, `to`, `candidate`, `sdp_mid?`, `sdp_m_line_index?` | Envoyer un candidat ICE |

#### Messages que le serveur envoie (JSON)

| Type | Champs | Quand |
|------|--------|-------|
| `joined` | `room_id`, `user_id`, `peers[]`, `ice_servers[]` | Confirmation d'entrée dans la room |
| `peer_joined` | `room_id`, `user_id` | Un nouveau pair vient d'arriver |
| `peer_left` | `room_id`, `user_id` | Un pair vient de partir |
| `offer` | `room_id`, `from`, `sdp` | Offer reçue et routée |
| `answer` | `room_id`, `from`, `sdp` | Answer reçue et routée |
| `ice_candidate` | `room_id`, `from`, `candidate`, `sdp_mid?`, `sdp_m_line_index?` | Candidat ICE routé |
| `error` | `message` | Erreur (SDP invalide, JSON mal formé, etc.) |

#### Architecture interne des rooms

Chaque utilisateur connecté possède un **canal `mpsc`** (message asynchrone Rust). Les rooms sont une simple `HashMap` en mémoire :

```
SignalingRooms = Arc<RwLock<HashMap<
    room_id: String,           // ex: "salle-de-reunion-42"
    HashMap<
        user_id: Uuid,         // ex: "550e8400-e29b-..."
        sender: mpsc::UnboundedSender<String>  // canal vers son WebSocket
    >
>>>
```

Quand A envoie une Offer à B, le backend trouve le canal de B dans la map et y pousse le JSON. La tâche de B lit ce canal et envoie le message sur son WebSocket. **Aucun broadcast, routage ciblé uniquement.**

#### Validation avant routage

Avant de router une Offer ou une Answer, le SDP est **validé** via `webrtc_engine::validate_sdp()`. Si le SDP est invalide, l'émetteur reçoit immédiatement un message `error` et rien n'est routé.

---

### 2. Persistance Redis — `src/redis/mod.rs`

En plus de la map mémoire (qui contient les canaux actifs), les membres de chaque room sont aussi enregistrés dans Redis. Cela permet :
- De connaître la liste des membres même si un pair vient de se reconnecter.
- D'avoir un historique persistant entre les redémarrages du serveur.

```
Clé Redis : room:<room_id>:members   (type Set)
TTL automatique : 24 heures
```

| Fonction | Description |
|----------|-------------|
| `create_pool(url)` | Crée le pool de connexions Redis |
| `room_add_member(pool, room_id, user_id)` | Ajoute un membre + réinitialise le TTL |
| `room_remove_member(pool, room_id, user_id)` | Retire un membre |
| `room_list_members(pool, room_id)` | Retourne la liste des UUIDs présents |

---

### 3. Moteur WebRTC — `src/webrtc_engine/mod.rs`

Ce module encapsule la crate `webrtc` (webrtc-rs) pour fournir des utilitaires réutilisables.

#### Validation SDP

```rust
pub fn validate_sdp(sdp: &str) -> AppResult<()>
```

Parse le SDP via le parseur webrtc-rs. Si la chaîne n'est pas du SDP valide (RFC 4566), retourne une erreur qui sera renvoyée au client.

#### Validation ICE candidate

```rust
pub fn validate_ice_candidate(candidate: &str) -> AppResult<()>
```

Vérifie qu'un candidat ICE n'est pas vide.

#### Factory RTCPeerConnection

```rust
pub async fn new_peer_connection(turn: &TurnConfig) -> AppResult<RTCPeerConnection>
```

Crée une `RTCPeerConnection` côté serveur avec les codecs par défaut (Opus, VP8, VP9, H264) et les interceptors RTP standard (NACK, RTCP reports).

> Cette fonction est disponible pour usage futur (SFU, enregistrement, etc.). Elle n'est pas appelée dans le flux de signalisation actuel car la signalisation est purement un relais de messages.

---

### 4. Sécurité TURN — credentials temporels (RFC 5766)

#### Le problème

Pour envoyer les serveurs TURN au navigateur, il faut fournir des credentials. Si on utilise un mot de passe permanent :
- Il est visible dans le réseau si HTTPS est mal configuré.
- N'importe qui qui le récupère peut utiliser ton serveur TURN indéfiniment.

#### La solution : credentials temporels

Le mécanisme est standardisé (RFC 5766) et supporté nativement par coturn (`--use-auth-secret`) :

```
username  = "<timestamp_unix_d_expiration>"
password  = base64( HMAC-SHA256( secret, username ) )
```

**Exemple concret :**
```
username = "1743600000"       ← expire le 02/04/2026 à 12h00
password = "Xk9mP2zA/..."   ← HMAC du secret sur ce timestamp
```

- Ces credentials **expirent automatiquement** après `ttl_secs` secondes (défaut : 1 heure).
- Même interceptés, ils sont inutilisables après expiration.
- Le **secret ne quitte jamais le backend** — seuls les credentials dérivés sont envoyés au client.
- Chaque appel génère des credentials différents (le timestamp d'expiration change).

```rust
fn generate_turn_credentials(secret: &str, ttl_secs: u64) -> AppResult<(String, String)>
```

#### Configuration dans `.env`

```env
APP_TURN__STUN_URLS=["stun:stun.l.google.com:19302"]   # toujours activé
APP_TURN__URL=turn:mon-serveur-turn:3478               # optionnel
APP_TURN__SECRET=mon-secret-coturn                    # optionnel
APP_TURN__TTL_SECS=3600                               # optionnel, défaut 1h
```

Si `APP_TURN__URL` et `APP_TURN__SECRET` ne sont pas définis, seuls les STUN sont envoyés.

#### Structure envoyée au navigateur lors du `joined`

```json
{
  "type": "joined",
  "ice_servers": [
    {
      "urls": ["stun:stun.l.google.com:19302"]
    },
    {
      "urls": ["turn:mon-serveur:3478"],
      "username": "1743600000",
      "credential": "Xk9mP2zA..."
    }
  ],
  "peers": ["550e8400-..."],
  "user_id": "...",
  "room_id": "..."
}
```

Le navigateur passe directement cet objet à `new RTCPeerConnection({ iceServers: ... })`.

---

### 5. Configuration centralisée — `src/config.rs`

Toutes les valeurs configurables passent par des variables d'environnement. **Zéro valeur en dur dans le code.**

```rust
pub struct TurnConfig {
    pub stun_urls: Vec<String>,      // APP_TURN__STUN_URLS
    pub url: Option<String>,         // APP_TURN__URL
    pub secret: Option<String>,      // APP_TURN__SECRET
    pub ttl_secs: u64,              // APP_TURN__TTL_SECS
}
```

La convention : préfixe `APP_` + double underscore `__` entre les niveaux de config.

---

### 6. Tests

#### Tests unitaires — `src/webrtc_engine/mod.rs`

10 tests, exécutables sans Redis ni Docker :

| Test | Ce qu'il vérifie |
|------|-----------------|
| `sdp_valide_accepte` | Un SDP Offer conforme RFC 4566 est accepté |
| `sdp_vide_refuse` | Un SDP vide est rejeté |
| `sdp_corrompu_refuse` | Du texte aléatoire est rejeté comme SDP |
| `ice_candidate_valide_accepte` | Un candidat ICE non-vide est accepté |
| `ice_candidate_vide_refuse` | Un candidat vide est rejeté |
| `turn_credentials_format` | Le username est un timestamp, le credential est en base64 |
| `turn_credentials_different_a_chaque_appel` | Deux appels génèrent des credentials différents |
| `turn_credentials_different_si_secret_different` | Deux secrets → deux credentials différents |
| `build_ice_servers_sans_turn` | Sans TURN configuré, seul STUN est retourné |
| `build_ice_servers_avec_turn` | Avec TURN configuré, STUN + TURN sont retournés |

```bash
cargo test --lib
```

#### Tests d'intégration — `tests/signaling.rs`

4 tests qui démarrent un vrai serveur Axum + Redis :

| Test | Ce qu'il vérifie |
|------|-----------------|
| `join_room_recoit_joined_avec_ice_servers` | Connexion WS → `joined` avec `ice_servers` non vide |
| `deux_pairs_room_notifications_croisees` | A rejoint → B rejoint → A reçoit `peer_joined`, B voit A dans `peers` |
| `offer_valide_routee_vers_pair_cible` | Offer SDP valide envoyée par A → reçue par B avec `from = user_a` |
| `offer_sdp_invalide_renvoie_erreur_a_lemeteur` | SDP invalide → A reçoit `error`, B ne reçoit rien |

**Comment ça fonctionne techniquement :**
1. `start_test_server()` : lie un serveur Axum sur le port `0` (le système attribue un port libre).
2. `create_test_session()` : injecte directement une `AppSession` dans la `HashMap` mémoire du state — pas besoin de passer par Keycloak.
3. `ws_connect()` : utilise `tokio-tungstenite` pour se connecter en WebSocket avec le cookie injecté dans le header HTTP.

Si Redis n'est pas disponible, les tests s'ignorent eux-mêmes sans planter.

```bash
cargo test --test signaling
```

---

## Dépendances ajoutées

| Crate | Version | Rôle |
|-------|---------|------|
| `axum` (feature `ws`) | 0.8 | WebSocket natif dans Axum |
| `deadpool-redis` | 0.18 | Pool de connexions Redis async |
| `futures-util` | 0.3 | `SinkExt`/`StreamExt` pour la boucle WebSocket |
| `webrtc` | 0.13 | SDP parsing, RTCPeerConnection, codecs, DTLS, SRTP |
| `hmac` | 0.12 | HMAC-SHA256 pour les credentials TURN |
| `sha2` | 0.10 | Algorithme SHA-256 |
| `base64` | 0.22 | Encodage du credential TURN |
| `tokio-tungstenite` | 0.28 | Client WebSocket pour les tests d'intégration (dev uniquement) |

---

## Fichiers créés ou modifiés

| Fichier | Statut | Résumé |
|---------|--------|--------|
| `src/ws/mod.rs` | Créé | Serveur de signalisation complet |
| `src/webrtc_engine/mod.rs` | Créé | Engine WebRTC + tests unitaires |
| `src/redis/mod.rs` | Créé | Pool Redis + helpers rooms |
| `src/config.rs` | Modifié | Ajout `RedisConfig`, `TurnConfig` |
| `src/state.rs` | Modifié | Ajout `redis: RedisPool`, `signaling_rooms: SignalingRooms` |
| `src/main.rs` | Modifié | Initialisation du pool Redis au démarrage |
| `src/lib.rs` | Modifié | `pub mod webrtc_engine` + `.merge(ws::router())` |
| `Cargo.toml` | Modifié | Nouvelles dépendances + `[dev-dependencies]` |
| `.env` | Modifié | Variables `APP_REDIS__URL`, `APP_TURN__*` |
| `tests/signaling.rs` | Créé | Tests d'intégration WebSocket |
| `README.md` | Modifié | Documentation complète dev/test/déploiement |

---

## Résultat final

```
cargo test

running 10 tests (unitaires)
test webrtc_engine::tests::build_ice_servers_avec_turn ... ok
test webrtc_engine::tests::build_ice_servers_sans_turn ... ok
test webrtc_engine::tests::ice_candidate_valide_accepte ... ok
test webrtc_engine::tests::ice_candidate_vide_refuse ... ok
test webrtc_engine::tests::sdp_corrompu_refuse ... ok
test webrtc_engine::tests::sdp_vide_refuse ... ok
test webrtc_engine::tests::sdp_valide_accepte ... ok
test webrtc_engine::tests::turn_credentials_different_a_chaque_appel ... ok
test webrtc_engine::tests::turn_credentials_different_si_secret_different ... ok
test webrtc_engine::tests::turn_credentials_format ... ok
test result: ok. 10 passed; 0 failed

running 4 tests (intégration)
test join_room_recoit_joined_avec_ice_servers ... ok
test deux_pairs_room_notifications_croisees ... ok
test offer_valide_routee_vers_pair_cible ... ok
test offer_sdp_invalide_renvoie_erreur_a_lemeteur ... ok
test result: ok. 4 passed; 0 failed
```

**14/14 tests passent.**

---

## Ce qui reste à faire côté backend (futures étapes)

- **Intégration frontend** : le frontend Nuxt doit se connecter à `/ws/signal`, envoyer un `join`, et utiliser le `ice_servers` reçu pour construire sa `RTCPeerConnection`.
- **TURN serveur** : déployer une instance coturn en production et renseigner `APP_TURN__URL` + `APP_TURN__SECRET`.
- **Rooms persistantes** : actuellement une room n'existe que le temps que des pairs y sont connectés. On pourrait lier les rooms aux meetings de la base de données.
- **Limites** : pas de limite sur le nombre de pairs par room pour l'instant.

---

---

# Version vulgarisée — pour expliquer à un novice

Cette section reprend exactement les mêmes concepts, mais sans jargon technique ni code Rust.

---

## C'est quoi WebRTC en une phrase ?

WebRTC est une technologie qui permet à **deux navigateurs de se parler directement**, sans passer par un serveur intermédiaire pour la vidéo et l'audio. Comme un appel téléphonique direct entre deux personnes, plutôt que de tout faire passer par un central.

---

## Le rôle du backend : le standardiste

Imagine que tu veux appeler un ami, mais tu n'as pas son numéro. Tu appelles un **standardiste** (le backend) qui dit à ton ami "hé, quelqu'un veut te parler". Ton ami répond au standardiste "ok, voilà comment me joindre directement". Le standardiste te transmet cette info.

À partir de là, **tu appelles directement ton ami** — le standardiste raccroche et n'entend plus rien.

C'est exactement ce que fait notre backend :
- Il met en relation deux navigateurs.
- Il transmet les informations de connexion entre eux.
- Une fois la connexion établie, il n'intervient plus du tout dans la vidéo/audio.

---

## Les concepts un par un

### SDP — la carte de visite technique

Quand tu veux commencer un appel vidéo avec quelqu'un, ton navigateur prépare une **carte de visite technique** qui dit :

> "Bonjour, je m'appelle Chrome. Je suis capable de faire de la vidéo en H264 et VP8, de l'audio en Opus. Voilà mon adresse réseau."

C'est ce qu'on appelle un **SDP** (Session Description Protocol). Il y en a deux :
- L'**Offer** (offre) : envoyée par celui qui initie l'appel — "voilà ce que je sais faire"
- L'**Answer** (réponse) : envoyée par celui qui reçoit — "ok, voilà ce que moi je sais faire en retour"

Le backend ne comprend pas ce que ça veut dire — il se contente de **vérifier que c'est bien formaté** et de **le transmettre** à l'autre navigateur.

### ICE — le GPS qui cherche le meilleur chemin

Une fois que les deux navigateurs savent qu'ils veulent se connecter, il faut trouver **comment** se rejoindre sur le réseau. C'est le rôle d'**ICE**.

ICE essaie plusieurs chemins dans l'ordre :
1. Connexion directe (le plus rapide, si les deux sont sur le même réseau)
2. Via l'adresse IP publique (si derrière une box internet normale)
3. Via un serveur relais TURN (en dernier recours, si derrière un pare-feu strict)

Les **candidats ICE** sont les différentes adresses réseau que chaque navigateur propose à l'autre pour tenter une connexion. Le backend les transmet de l'un à l'autre.

### STUN — "Quelle est mon adresse ?"

Pour que ton navigateur puisse proposer son adresse IP publique comme candidat ICE, il doit d'abord **connaître cette adresse**. En effet, ta box internet cache ton adresse réelle.

Un **serveur STUN** est un service ultra-simple qui répond à la question : "Quelle IP tu vois quand je t'envoie ce message ?"

> Ton navigateur → serveur STUN : "Coucou"  
> Serveur STUN → ton navigateur : "Je te vois depuis 90.45.123.67:54321"

C'est tout. Le navigateur inclut ensuite cette adresse dans ses candidats ICE.

### TURN — le relais de dernier recours

Certaines entreprises ou réseaux ont des **pare-feux très stricts** qui bloquent toute connexion directe entre deux ordinateurs extérieurs. Dans ce cas, il faut passer par un **relais** : c'est le serveur TURN.

> Navigateur A → serveur TURN → Navigateur B

Le TURN voit tout le trafic vidéo/audio, contrairement au STUN. C'est pourquoi il faut des **credentials** pour y accéder (sinon n'importe qui pourrait utiliser ton serveur comme relais et saturer ta bande passante).

---

## La sécurité des credentials TURN — sans le jargon

### Le problème avec un mot de passe fixe

Imagine que tu donnes le même mot de passe WiFi à tout le monde pour toujours. Si quelqu'un le capture une fois, il peut l'utiliser indéfiniment.

C'est le problème avec des credentials TURN permanents.

### La solution : un ticket à usage limité dans le temps

À la place, le backend génère un **ticket qui expire** :

```
ticket_valable_jusqu_au = "02/04/2026 à 13h00"
code_d_accès = signature_secrète(ticket_valable_jusqu_au)
```

Le serveur TURN vérifie :
1. Est-ce que le ticket n'est pas expiré ?
2. Est-ce que la signature correspond bien à notre secret partagé ?

Si quelqu'un intercepte ce ticket, il ne peut l'utiliser que pendant 1 heure maximum. Après, il est inutile.

Le **secret** qui sert à signer les tickets ne quitte **jamais** le backend. Seuls les tickets dérivés sont envoyés aux navigateurs.

---

## Le flux complet, étape par étape

Voici ce qui se passe quand Alice et Bob veulent faire un appel vidéo sur Truegather :

**Étape 1 — Connexion au standardiste**
```
Alice ouvre la page de meeting.
Son navigateur se connecte au backend via WebSocket (connexion permanente).
Bob fait pareil.
```

**Étape 2 — Rejoindre la salle**
```
Alice envoie : { "type": "join", "room_id": "meeting-42" }
Le backend répond : "Tu es dans la salle. Voici les serveurs STUN/TURN à utiliser."
Bob envoie la même chose.
Le backend dit à Alice : "Bob vient d'arriver."
Le backend dit à Bob : "Alice était déjà là."
```

**Étape 3 — Échange des cartes de visite (SDP)**
```
Le navigateur d'Alice prépare son Offer SDP.
Alice l'envoie au backend en disant : "Transmets ça à Bob."
Le backend vérifie que c'est bien du SDP valide, puis le transmet à Bob.
Bob répond avec son Answer SDP, transmis à Alice par le backend.
```

**Étape 4 — Négociation du chemin réseau (ICE)**
```
Les deux navigateurs testent différents chemins réseau.
À chaque candidat trouvé, ils l'envoient au backend qui le transmet à l'autre.
Les navigateurs essaient les chemins dans l'ordre jusqu'à trouver le meilleur.
```

**Étape 5 — Connexion directe établie**
```
Un chemin fonctionne. Les deux navigateurs sont maintenant connectés directement.
La vidéo et l'audio circulent directement entre Alice et Bob.
Le backend ne voit plus rien — il reste juste disponible pour les messages de contrôle.
```

---

## La room — comment c'est organisé côté backend

Imagine un tableau blanc dans une salle de réunion virtuelle :

```
Salle "meeting-42"
├── Alice  →  [boîte aux lettres privée d'Alice]
└── Bob    →  [boîte aux lettres privée de Bob]
```

Quand le backend doit envoyer un message à Bob, il le dépose dans **la boîte aux lettres de Bob**. Bob lit sa boîte et envoie le message sur son WebSocket.

Ça permet de router n'importe quel message vers n'importe quel pair sans que les messages se mélangent.

---

## Pourquoi Redis ?

La map des rooms existe en mémoire vive du serveur (ultra-rapide). Redis sert de **sauvegarde** :
- Si le serveur redémarre, il peut retrouver qui était dans quelle salle.
- Les données Redis s'effacent automatiquement après 24h d'inactivité.

---

## Les tests — à quoi ça sert ?

On a écrit 14 tests automatiques pour s'assurer que tout fonctionne correctement.

**Les tests unitaires (10)** vérifient des petites fonctions isolées :
- "Est-ce que le serveur détecte bien qu'un SDP est invalide ?"
- "Est-ce que les tickets TURN sont bien différents à chaque génération ?"

**Les tests d'intégration (4)** simulent un vrai scénario :
- Un faux Alice et un faux Bob se connectent à un vrai serveur de test.
- On vérifie que les messages sont bien transmis dans le bon sens.
- On vérifie qu'un SDP invalide est bien rejeté avec un message d'erreur.

Ces tests tournent automatiquement à chaque modification du code, ce qui garantit qu'on n'a pas cassé quelque chose sans s'en rendre compte.

---

---

# Sprint 2 mai 2026 — Vérification email, changement de mot de passe, corrections auth

**Branches :** `TR-127` (back) · `TR-128` (front) · `TR-88` (back) · `TR-129` (front) · `TR-118` (front paramètres)

---

## Fonctionnalités implémentées

### 1. Vérification d'email à l'inscription (TR-127 / TR-128)

Lors d'une première inscription via Keycloak, le backend génère un token cryptographique, en stocke le hash dans Redis (TTL 15 min) et envoie un email de vérification via l'API Brevo.

**Flow complet :**
1. L'utilisateur clique "Créer un compte" → redirigé vers Keycloak
2. Après inscription, le callback OIDC détecte `is_registration && is_new`
3. Un token (UUID v4) est généré côté backend — le hash SHA-256 est stocké dans Redis, **jamais le token brut**
4. Un email est envoyé avec le lien `https://backend/api/v1/auth/verify-email?token=<token>`
5. L'utilisateur est redirigé vers `/auth/verify-email?pending=1` (page d'attente)
6. En cliquant le lien de l'email, le backend vérifie le hash, marque `email_verified = true` et redirige vers `/auth/verify-email?verified=1`
7. La page frontend passe automatiquement en état "vérifié" via BroadcastChannel (même navigateur) ou localStorage (autre onglet/fenêtre)

**Sécurité :**
- Le token n'est stocké que sous forme de hash — une fuite Redis n'expose pas le token
- TTL 15 minutes → le lien expire rapidement
- Token à usage unique (consommé à la vérification via `DEL` Redis)

**Nouveaux fichiers :**
- `src/auth/email_verification.rs` — génération, hachage, stockage et consommation du token Redis
- `migrations/0002_email_verification.sql` — colonne `email_verified` en base
- `migrations/0003_sessions_and_user_uuid.sql` — UUID stable pour les sessions
- `app/pages/auth/verify-email.vue` — page frontend multi-états (pending / verified / error)

---

### 2. Changement de mot de passe avec confirmation email (TR-88 / TR-129)

L'onglet "Sécurité" du modal Paramètres permet de changer son mot de passe. Après succès, un email de confirmation est envoyé automatiquement.

**Flow :**
1. L'utilisateur saisit le nouveau mot de passe (14 caractères min) et sa confirmation
2. Le backend valide le payload, extrait la session, obtient un token admin Keycloak via `client_credentials`
3. L'API admin Keycloak met à jour le mot de passe via `PUT /admin/realms/truegather/users/{id}/reset-password`
4. Un email de confirmation est envoyé via Brevo (non bloquant — un échec d'envoi ne compromet pas le changement)
5. Le frontend affiche le message de succès avec mention de l'email

**Choix de conception — suppression de la vérification de l'ancien mot de passe :**
La vérification initiale utilisait le flow ROPC (Resource Owner Password Credentials), nécessitant l'activation de "Direct Access Grants" dans Keycloak. Ce flow est déconseillé (RFC 9700) car il expose le mot de passe au backend. On l'a remplacé par un accès direct via l'API admin (`client_credentials`), plus sécurisé et sans configuration Keycloak supplémentaire.

---

## Corrections de bugs

### Bug 1 — Port 8080 vs 8082
**Symptôme :** login, logout et email de vérification ne fonctionnaient plus.  
**Cause :** le `.env` backend déclare `APP__SERVER__PORT=8082` mais le frontend avait encore `http://localhost:8080` en dur dans 4 fichiers.  
**Correction :** mise à jour du port dans `nuxt.config.ts`, `useAuth.ts`, `index.vue`, `dashboard.vue`.

---

### Bug 2 — Route logout GET vs POST
**Symptôme :** le clic sur "Se déconnecter" (qui utilise `window.location.href`) provoquait une erreur 405 Method Not Allowed.  
**Cause :** la route `/api/v1/auth/logout` était déclarée `post` dans Axum, mais `window.location.href` génère une navigation GET.  
**Correction :** changement en `get` dans `src/auth/routes.rs`.

---

### Bug 3 — Reconnexion silencieuse après redémarrage backend
**Symptôme :** après un redémarrage du backend (sessions mémoire perdues), le clic "Se connecter" reconnectait l'utilisateur directement via la session SSO Keycloak sans lui demander ses identifiants.  
**Cause :** le flow OIDC ne forçait pas la saisie des identifiants.  
**Correction :** ajout de `&prompt=login` dans l'URL d'autorisation OIDC pour le flow login (`src/auth/oidc.rs`).

---

### Bug 4 — CORS bloquant les requêtes PUT
**Symptôme :** le changement de mot de passe retournait une erreur sans message (preflight OPTIONS bloqué).  
**Cause :** la couche CORS Axum n'autorisait que `GET`, `POST` et `OPTIONS` — la méthode `PUT` était absente.  
**Correction :** ajout de `Method::PUT` et `Method::DELETE` dans `build_cors_layer()` (`src/lib.rs`).

---

### Bug 5 — `client_secret` absent de l'échange de code OIDC
**Symptôme :** après activation de "Client authentication" dans Keycloak (nécessaire pour le flow `client_credentials`), la connexion retournait une erreur 401 "Invalid client or Invalid client credentials".  
**Cause :** le client Keycloak est devenu confidentiel mais le backend n'envoyait pas le `client_secret` dans le form d'échange de code OIDC.  
**Correction :** ajout conditionnel du `client_secret` dans `exchange_code_for_tokens()` si la config le fournit.

---

### Bug 6 — Service account sans permission `manage-users`
**Symptôme :** le changement de mot de passe retournait une erreur 403 Forbidden de Keycloak.  
**Cause :** le service account `truegather-backend` n'avait pas le rôle `manage-users` du client `realm-management`.  
**Correction (Keycloak):** Clients → `truegather-backend` → Service accounts roles → Assign role → Filter by clients → `realm-management` → `manage-users`.

---

## Difficultés rencontrées

### Client Keycloak — public vs confidentiel
La complexité principale de cette session a été la configuration Keycloak. Le client était initialement "public" (sans secret), ce qui suffisait pour le flow PKCE. En activant "Client authentication" pour débloquer le flow `client_credentials` (nécessaire au changement de mot de passe), plusieurs effets en cascade ont cassé la connexion :
- L'échange de code OIDC requiert désormais le `client_secret`
- Le suffixe `APP__KEYCLOAK__CLIENT_SECRET` était absent du `.env`
- Le service account n'avait pas les permissions admin

Chaque erreur a nécessité un aller-retour entre le backend, le `.env` et la console Keycloak.

### Sessions en mémoire et tests locaux
Les sessions étant stockées uniquement en mémoire (non persistées), tout redémarrage du backend invalide les sessions existantes. Cela complique les tests locaux car il faut se reconnecter à chaque `cargo run`. Une migration vers Redis ou PostgreSQL pour la persistence des sessions est prévue.

### Envoi d'email Brevo — débogage difficile
L'envoi d'email étant non bloquant (best-effort), un échec n'est visible que dans les logs du backend. Sans accès facile aux logs en cours d'exécution, il était difficile de distinguer un problème de clé API d'un problème de template. La solution a été de tester l'appel Brevo directement via `curl` pour isoler le problème.

---

## Configuration Keycloak requise (récapitulatif)

Pour que le système fonctionne complètement en local :

| Paramètre | Valeur |
|-----------|--------|
| Client authentication | ON |
| Service account roles | Activé |
| Rôle service account | `realm-management` > `manage-users` |
| Variable `.env` | `APP__KEYCLOAK__CLIENT_SECRET=<secret>` |

---

## Fichiers modifiés dans ce sprint

| Fichier | Branche | Résumé |
|---------|---------|--------|
| `src/auth/email_verification.rs` | TR-127 | Nouveau — token Redis pour vérification email |
| `src/auth/handlers.rs` | TR-127 / TR-88 | Vérification email + changement mdp sans ROPC |
| `src/auth/dto.rs` | TR-88 | Suppression `current_password` |
| `src/auth/oidc.rs` | TR-127 / TR-88 | `prompt=login` + `client_secret` dans échange OIDC |
| `src/auth/routes.rs` | TR-127 | Logout GET au lieu de POST |
| `src/auth/sync.rs` | TR-127 | Sync utilisateur Keycloak → local |
| `src/lib.rs` | TR-88 | CORS — ajout PUT et DELETE |
| `src/mail/service.rs` | TR-127 | Envoi email vérification + confirmation mdp |
| `src/mail/templates.rs` | TR-127 | Templates HTML email |
| `src/models/user.rs` | TR-127 | Champ `email_verified` |
| `migrations/0002_email_verification.sql` | TR-127 | Colonne email_verified |
| `migrations/0003_sessions_and_user_uuid.sql` | TR-127 | UUID sessions |
| `nuxt.config.ts` | TR-128 | Port 8082 |
| `app/composables/useAuth.ts` | TR-128 | Port 8082 + logout amélioré |
| `app/pages/auth/verify-email.vue` | TR-128 | Nouveau — page vérification email |
| `app/pages/index.vue` | TR-128 | Port 8082 |
| `app/pages/dashboard.vue` | TR-128 | Port 8082 |
| `components/barrehorizontale/SettingsModal.vue` | TR-129 | Formulaire mdp sans ancien mdp, message email |

---

## Sprint 2 mai 2026 — Visio P2P Frontend (TR-101 front)

### Objectif

Intégrer le serveur de signalisation WebRTC (déjà opérationnel côté backend TR-101) avec un frontend Nuxt permettant à 3 clients ou plus de se voir en temps réel via des connexions WebRTC P2P directes.

### Architecture frontend

```
Navigateur A (localhost:3000/visio/ma-room)
  └── useWebRTC()
        └── useSignaling()  →  ws://localhost:8082/ws/signal
              ├── onJoined   → crée RTCPeerConnection pour chaque pair existant
              ├── onPeerJoined  → crée Offer vers le nouveau pair
              ├── onOffer    → répond Answer
              ├── onAnswer   → finalise connexion
              └── onIceCandidate → addIceCandidate (avec tampon)
```

### Topologie pour 3 clients

```
A rejoint (seul)
B rejoint → serveur envoie peer_joined(B) à A
            → A crée Offer vers B
            → B reçoit joined(peers=[A]), crée RTCPeerConnection sans Offer
C rejoint → serveur envoie peer_joined(C) à A et B
            → A crée Offer vers C
            → B crée Offer vers C
            → C reçoit joined(peers=[A, B]), crée 2 RTCPeerConnections sans Offer
```

Résultat : 3 connexions P2P (A↔B, A↔C, B↔C) sans aucun flux passant par le serveur.

### Problème résolu — Offer collision

**Avant :** les deux handlers `onJoined` et `onPeerJoined` créaient chacun une Offer, provoquant une collision SDP (les deux pairs se retrouvaient à simultanément en état `have-local-offer`).

**Fix :** seuls les pairs **existants** (qui reçoivent `peer_joined`) créent l'Offer. Le **nouveau pair** (qui reçoit `joined`) crée uniquement la `RTCPeerConnection` et attend les Offers.

### Problème résolu — ICE candidates prématurés

Les ICE candidates peuvent arriver avant que `setRemoteDescription` soit appelé. Un buffer `iceCandidateBuffer: Map<string, RTCIceCandidateInit[]>` met en tampon ces candidates et les rejoue juste après `setRemoteDescription`.

### Fichiers créés

| Fichier | Description |
|---------|-------------|
| `app/composables/useSignaling.ts` | WebSocket de signalisation (connect, send, handlers) |
| `app/composables/useWebRTC.ts` | Orchestration RTCPeerConnection, mesh topology |
| `app/composables/useAvatar.ts` | Gestion avatar local (localStorage) |
| `app/pages/visio/[roomId].vue` | Page de visio plein écran, layout: false |
| `components/visio/RemoteVideoTile.vue` | Tuile vidéo distante (watch stream → srcObject) |

### Fonctionnement de la page `/visio/:roomId`

1. `onMounted` demande accès caméra/micro (`getUserMedia`)
2. Connexion WebSocket → `joinRoom(roomId)` → cookie `tg_session` envoyé automatiquement
3. Grille CSS Grid avec `repeat(var(--cols), 1fr)` adaptive : 1 col seul, 2 cols à 2-4 participants, 3 cols à 5+
4. Bouton "Copier le lien" pour partager la room
5. Mute/unmute micro et caméra via `track.enabled`
6. "Quitter" → `leaveRoom()` + `router.push('/dashboard')`

---

---

# Sprint 3 mai 2026 — Architecture hybride P2P + SFU

## Objectif

Faire évoluer la visio d'une simple topologie **P2P mesh** vers une **architecture hybride** :

- **2 participants → P2P** : connexion directe navigateur-à-navigateur, le serveur ne touche jamais aux flux.
- **3+ participants → SFU** : chaque client envoie son flux **une seule fois** au SFU côté serveur, qui le redistribue aux autres. Le serveur ne décode ni ne stocke jamais les paquets.

Le basculement est **automatique et transparent** : le 3ème participant déclenchant le switch reçoit un message `mode_switch` et tous les clients passent en mode SFU sans action manuelle.

---

## Pourquoi P2P mesh ne passe pas à l'échelle

En topologie mesh, chaque participant doit établir une connexion directe avec **chacun des autres**. Le nombre de connexions croît en $\frac{N(N-1)}{2}$ :

| Participants | Connexions P2P | Upload par client |
|:---:|:---:|:---:|
| 2 | 1 | 1× son flux |
| 4 | **6** | **3×** son flux |
| 6 | **15** | **5×** son flux |
| 10 | **45** | **9×** son flux |

À 4 participants, chaque navigateur doit encoder et uploader sa vidéo **3 fois en parallèle**. La batterie, le CPU et la bande passante s'effondrent.

Avec un SFU, chaque client **encode et upload une seule fois**, quelle que soit la taille de la room.

---

## Architecture SFU implémentée

```
                    ┌─────────────────────────────┐
                    │      Backend Rust (SFU)     │
                    │                             │
  Client A ──pub──→ │  RTCPeerConnection (A)      │
           ←─sub──  │  relay track A → B, C       │
                    │                             │
  Client B ──pub──→ │  RTCPeerConnection (B)      │
           ←─sub──  │  relay track B → A, C       │
                    │                             │
  Client C ──pub──→ │  RTCPeerConnection (C)      │
           ←─sub──  │  relay track C → A, B       │
                    │                             │
                    │ NE DÉCODE PAS les paquets   │
                    │ NE STOCKE PAS les flux      │
                    └─────────────────────────────┘
```

Chaque client a une **unique RTCPeerConnection** vers le SFU (au lieu de N-1 en mesh). Le SFU crée une `TrackLocalStaticRTP` par flux entrant et la relie aux peer connections des autres participants via `write_rtp()` — copie brute d'octets, aucun décodage.

---

## Signalisation hybride — nouveaux messages

### Messages client → serveur

| Type | Champs | Mode | Description |
|------|--------|------|-------------|
| `join` | `room_id` | les deux | Rejoindre une room |
| `leave` | `room_id` | les deux | Quitter |
| `offer` | `room_id`, `to`, `sdp` | P2P | Offer vers un pair |
| `answer` | `room_id`, `to`, `sdp` | P2P | Answer vers un pair |
| `ice_candidate` | `room_id`, `to`, `candidate`, ... | P2P | ICE vers un pair |
| `sfu_answer` | `room_id`, `sdp` | SFU | Answer vers le SFU (réponse à `sfu_offer`) |
| `sfu_ice_candidate` | `room_id`, `candidate`, ... | SFU | ICE vers le SFU |

### Messages serveur → client

| Type | Champs | Mode | Description |
|------|--------|------|-------------|
| `joined` | `room_id`, `user_id`, `peers`, `ice_servers`, **`mode`** | les deux | Confirmation + mode actuel |
| `peer_joined` | `room_id`, `user_id` | les deux | Nouveau participant |
| `peer_left` | `room_id`, `user_id` | les deux | Départ d'un participant |
| **`mode_switch`** | `room_id`, **`mode: "sfu"`**, `peers` | transition | Bascule P2P → SFU |
| `offer` | `room_id`, `from`, `sdp` | P2P | Offer routée |
| `answer` | `room_id`, `from`, `sdp` | P2P | Answer routée |
| `ice_candidate` | `room_id`, `from`, `candidate`, ... | P2P | ICE routé |
| **`sfu_offer`** | `room_id`, `sdp` | SFU | Offer du SFU vers le client |
| **`sfu_ice_candidate`** | `room_id`, `candidate`, ... | SFU | ICE du SFU vers le client |

---

## Flux complet — cas P2P (2 utilisateurs)

```
Alice rejoint                 Backend                  Bob rejoint
    │                            │                         │
    │── join(room="abc") ────────▶│                         │
    │◀─ joined(peers=[], mode="p2p") ────────────────────  │
    │                            │◀── join(room="abc") ────│
    │◀─ peer_joined(Bob) ────────│─── joined(peers=[Alice], mode="p2p") ──▶│
    │                            │                         │
    │── offer(to=Bob, sdp) ─────▶│── offer(from=Alice) ──▶│
    │◀─ answer(from=Bob, sdp) ───│◀─ answer(to=Alice) ────│
    │── ice ─────────────────────│──────────────────────── ▶│
    │◀─ ice ─────────────────────│◀────────────────────────│
    │                            │                         │
    │◀══════════════ Connexion P2P directe ════════════════▶│
         (le backend ne voit plus les flux média)
```

## Flux complet — basculement vers SFU (3ème participant)

```
Alice & Bob en P2P      Backend                Charlie rejoint
    │                      │                         │
    │                      │◀── join(room="abc") ────│
    │                      │
    │  [Backend détecte 3 participants → switch SFU]
    │                      │
    │◀─ mode_switch(sfu) ──│─── joined(mode="sfu") ─▶│
    │  [Alice ferme ses connexions P2P]
    │                      │
    │◀─ sfu_offer(sdp) ────│──────── sfu_offer(sdp) ─▶│
    │── sfu_answer(sdp) ──▶│◀────── sfu_answer(sdp) ──│
    │◀─▶ sfu_ice ──────────│◀──────▶ sfu_ice ──────────│
    │                      │                         │
    │◀══ flux de Bob ═══SFU══ flux de Charlie ════════▶│
    │◀══ flux de Charlie ══SFU══ flux de Alice ═══════▶│
            (le SFU relaye les paquets RTP bruts)
```

---

## Implémentation backend — `src/webrtc_engine/sfu.rs`

### `SfuState` et structure des rooms

```rust
pub struct SfuInner {
    pub rooms: HashMap<String, SfuRoom>,
}

pub struct SfuRoom {
    pub participants: HashMap<Uuid, SfuParticipant>,
    // relay tracks publiées par chaque participant
    pub relay_tracks: HashMap<Uuid, Vec<Arc<TrackLocalStaticRTP>>>,
}
```

Un `SfuParticipant` contient la `RTCPeerConnection` côté serveur et le canal WebSocket pour lui envoyer des messages (candidats ICE, offers).

### `add_participant()` — connexion d'un client au SFU

Quand un client passe en mode SFU, le backend :

1. Crée une `RTCPeerConnection` serveur avec deux transceivers `recvonly` (audio + vidéo)
2. Ajoute les **relay tracks déjà existantes** des autres participants (le nouveau reçoit leurs flux immédiatement)
3. Configure `on_ice_candidate` pour transmettre les candidats ICE au client via WebSocket
4. Configure `on_track` (voir ci-dessous)
5. Envoie une SDP Offer au client → déclenche la négociation

### `setup_on_track()` — le cœur du SFU

C'est ici que le relais se produit. Quand un paquet RTP arrive d'un client :

```
1. Création d'un TrackLocalStaticRTP opaque
   (même codec que la source, identifié par publisher_id comme stream_id)

2. Phase d'état (lock court) :
   - Stockage de la relay track dans SfuRoom.relay_tracks[publisher_id]
   - Collecte de toutes les peer connections des autres participants

3. Renegotiation avec chaque abonné (hors lock) :
   foreach subscriber {
     subscriber.pc.add_track(relay_track)
     send_offer(subscriber)   ← le subscriber reçoit le nouveau flux
   }

4. Boucle de forwarding RTP (infinie, hors lock) :
   loop {
     packet = remote_track.read_rtp()
     relay_track.write_rtp(packet)   ← copie brute, 0 décodage
   }
```

**Invariant de sécurité :** le `write_rtp` copie des octets opaques. Le SFU ne désencapsule jamais le contenu SRTP, il ne voit pas les données audio/vidéo.

### `ws/mod.rs` — logique de bascule

```rust
const MAX_P2P_SIZE: usize = 2;   // ≤ 2 → P2P mesh
const MAX_SFU_SIZE: usize = 10;  // ≤ 10 → SFU

// Lors d'un join, si on dépasse MAX_P2P_SIZE :
if switching_to_sfu {
    // 1. Notifier tous les existants → mode_switch(sfu)
    // 2. Pour chaque participant (existants + nouveau) → add_participant() SFU
    // 3. Confirmer l'entrée au nouveau → joined(mode="sfu")
}
```

Le mode de chaque room est stocké dans `state.room_modes: Arc<RwLock<HashMap<String, RoomMode>>>`. Lors d'un `leave_room`, si la room devient vide le mode est effacé.

---

## Implémentation frontend

### `useWebRTC.ts` — gestion du mode dynamique

```typescript
const mode = ref<'p2p' | 'sfu'>('p2p')
let sfuConnection: RTCPeerConnection | null = null
const sfuIceCandidateBuffer: RTCIceCandidateInit[] = []
```

**Handler `onModeSwitch`** (P2P → SFU) :
1. Ferme toutes les `RTCPeerConnection` P2P
2. Vide le buffer ICE P2P
3. Réinitialise `remotePeers` (les flux SFU rempliront via `ontrack`)
4. Passe `mode.value = 'sfu'`

Le serveur envoie ensuite un `sfu_offer` → le handler `onSfuOffer` crée la connexion SFU, répond avec `sfu_answer`.

**Identification des flux SFU :** le SFU utilise le `user_id` du publisher comme `stream.id` de la `TrackLocalStaticRTP`. Côté client, `pc.ontrack` peut lire `streams[0].id` pour associer le flux au bon pair.

### `useSignaling.ts` — nouveaux types

```typescript
// Nouveaux messages entrants
| { type: 'mode_switch'; room_id: string; mode: 'p2p' | 'sfu'; peers: string[] }
| { type: 'sfu_offer'; room_id: string; sdp: string }
| { type: 'sfu_ice_candidate'; room_id: string; candidate: string; ... }

// Nouveaux messages sortants
| { type: 'sfu_answer'; room_id: string; sdp: string }
| { type: 'sfu_ice_candidate'; room_id: string; candidate: string; ... }
```

### `/visio/[roomId].vue` — badge mode

Le header de la page affiche maintenant un badge indiquant le mode actuel :

- Badge **P2P** (cyan) : ≤ 2 participants, connexion directe
- Badge **SFU** (violet) : ≥ 3 participants, relais serveur

---

## État partagé — `src/state.rs`

Deux nouveaux champs dans `AppState` :

```rust
pub room_modes: RoomModes,
// Arc<RwLock<HashMap<String, RoomMode>>>
// RoomMode::P2p | RoomMode::Sfu

pub sfu_state: SfuState,
// Arc<Mutex<SfuInner>>
```

---

## Sécurité et contraintes

| Contrainte | Implémentation |
|-----------|----------------|
| Le backend ne lit jamais les flux | `write_rtp` = copie d'octets opaques, pas de décodage |
| Chiffrement SRTP obligatoire | Imposé par la spec WebRTC (DTLS-SRTP) |
| Limite P2P | `MAX_P2P_SIZE = 2` → bascule automatique à 3 |
| Limite SFU | `MAX_SFU_SIZE = 10` → erreur au-delà |
| Nettoyage à la déconnexion | `leave_room` → `sfu::remove_participant()` + fermeture PC |
| Credentials TURN temporels | Inchangé — RFC 5766, expire après TTL |

---

## Fichiers modifiés dans ce sprint

| Fichier | Résumé |
|---------|--------|
| `src/webrtc_engine/sfu.rs` | **Nouveau** — SFU complet : state, add/remove participant, on_track relay, forwarding RTP |
| `src/webrtc_engine/mod.rs` | Ajout `pub mod sfu` |
| `src/ws/mod.rs` | Nouveaux messages SFU/P2P, logique de bascule, `leave_room` nettoyage SFU |
| `src/state.rs` | `RoomMode`, `RoomModes`, `SfuState` dans `AppState` |
| `app/composables/useSignaling.ts` | Types `mode_switch`, `sfu_offer`, `sfu_ice_candidate` + méthodes `sendSfuAnswer`, `sendSfuIceCandidate` |
| `app/composables/useWebRTC.ts` | `mode` ref, `sfuConnection`, handlers `onModeSwitch`/`onSfuOffer`/`onSfuIceCandidate`, `leaveRoom` propre |
| `app/pages/visio/[roomId].vue` | Badge P2P/SFU dans le header |

---

## Évolutions futures

- **TURN dédié** : déployer coturn pour les environnements NAT stricts (entreprises, 4G).
- **Retour SFU → P2P** : si un participant quitte et qu'on redescend à 2, repasser en mesh (économie de ressources serveur).
- **Scalabilité horizontale** : externaliser le `SfuState` dans Redis ou utiliser un SFU dédié (LiveKit, Mediasoup) pour plusieurs instances backend.
- **E2EE** : Insertable Streams (WebRTC Encoded Transform) pour chiffrement de bout en bout même via le SFU.
- **Load balancing** : distribuer les rooms SFU sur plusieurs serveurs via un routeur de sessions Redis.

---

## Résumé vulgarisé — SFU pour un novice

### Le problème du groupe

Imagine 4 amis en appel vidéo. En P2P, chacun doit envoyer sa vidéo **aux 3 autres en même temps**. C'est comme si tu devais envoyer la même lettre à 3 personnes différentes — tu la réécris 3 fois.

Avec un SFU, c'est comme envoyer la lettre à **un bureau de poste central** qui fait les copies. Toi tu envoies une seule lettre, le bureau en fait 3 copies et les distribue.

### Ce que fait le SFU de Truegather

- Il reçoit le flux vidéo de chaque participant **une seule fois**.
- Il le "copie" (sans lire le contenu, comme un photocopieur qui ne lit pas le texte) et l'envoie à tous les autres.
- Le contenu reste **chiffré du début à la fin** — le SFU ne peut pas décoder ce qu'il transporte.

### Pourquoi le basculement est automatique

- **1 ou 2 participants** : pas besoin d'un intermédiaire, connexion directe (plus rapide, moins de latence).
- **3 participants et plus** : le serveur détecte le 3ème arrivant, envoie un signal à tout le monde, et chaque client reconnecte via le SFU. Du point de vue de l'utilisateur, rien ne change — l'image continue d'apparaître.


