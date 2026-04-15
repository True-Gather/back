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

# Session du 14 avril 2026

**Objectif :** Implémentation de la photo de profil (avatar) — persistance PostgreSQL, endpoint REST, composable Vue, et intégration du mode sombre sur le dashboard.

---

## Ce qui a été fait

### 1. Backend — champ `profile_photo_url` et endpoint `PUT /api/v1/auth/avatar`

#### Modèle `User` — `src/models/user.rs`

Le champ `profile_photo_url: Option<String>` a été ajouté à :
- `User` : représentation interne complète d'un utilisateur.
- `UserProfileView` : DTO renvoyé au client via `/me`.

Cela permet de stocker une **data URL base64** (ex. `data:image/png;base64,...`) ou une URL externe pointant vers la photo de profil.

#### Session mémoire — `src/auth/session.rs` + `src/state.rs`

`profile_photo_url` a été ajouté à `AppSession` (la session en mémoire vive). Ainsi, après connexion, la photo de profil est immédiatement disponible sans requête DB supplémentaire pour chaque appel à `/me`.

#### Synchronisation à la connexion — `src/auth/sync.rs`

La fonction `sync_user_from_keycloak` crée ou met à jour le user en base via un **UPSERT** (`ON CONFLICT DO UPDATE`). Elle `RETURNING profile_photo_url` pour récupérer la valeur persistée (qui peut avoir été mise à jour manuellement entre deux connexions). La valeur est ensuite propagée dans l'objet `User` retourné.

```rust
async fn sync_user_to_db(state, keycloak_sub, user) -> AppResult<Option<String>> {
    // INSERT ... ON CONFLICT DO UPDATE SET ... RETURNING profile_photo_url
}
```

#### Endpoint avatar — `src/auth/handlers.rs` + `src/auth/routes.rs`

Nouvel endpoint **`PUT /api/v1/auth/avatar`** :
- Extrait la session courante via le cookie `tg_session`.
- Reçoit `{ "avatar_url": "data:image/png;base64,..." }` (ou `null` pour supprimer).
- Effectue un `UPDATE users SET profile_photo_url = $1 WHERE keycloak_id = $2`.
- Met à jour la session mémoire en temps réel (pas besoin de se reconnecter).

#### Configuration — `src/config.rs` + `src/main.rs`

- Ajout de `DatabaseConfig { url }` dans `AppConfig` pour exposer `APP__DATABASE__URL` proprement.
- Ajout de `issuer_url_internal: Option<String>` dans `KeycloakConfig` : permet de distinguer l'URL publique Keycloak (utilisée par le navigateur) de l'URL interne (utilisée par le backend en Docker, ex. `http://host.docker.internal:...`). Évite les erreurs de résolution DNS dans un environnement containerisé.
- Le pool PostgreSQL (`PgPool`) est maintenant instancié dans `main.rs` et passé à `AppState`.

---

### 2. Frontend — composable `useAvatar` + SettingsModal + thème sombre

#### Composable `useAvatar` — `app/composables/useAvatar.ts`

Gère l'état de l'avatar de façon réactive :

| Fonction | Comportement |
|----------|-------------|
| `loadAvatar()` | Priorité : `profile_photo_url` du backend. Repli : `localStorage` pour éviter un flash au chargement. |
| `saveAvatar(dataUrl)` | Mise à jour locale immédiate + `PUT /api/v1/auth/avatar` en arrière-plan. |

Le cache `localStorage` (clé `tg-avatar`) assure une expérience fluide : la photo s'affiche immédiatement au rechargement de page, avant même la réponse du backend.

#### `app/composables/useAuth.ts`

Ajout du champ `profile_photo_url?: string | null` dans le type `AuthUser` pour correspondre à la nouvelle réponse de `/me`.

#### `app/app.vue`

Au montage de l'application (`onMounted`) :
- Restauration du thème depuis `localStorage` (`tg-theme`).
- Appel à `loadAvatar()` pour pré-charger la photo de profil globalement.

#### `components/barrehorizontale/SettingsModal.vue` — refonte complète en onglets

La modal de paramètres a été entièrement réécrite. Avant : un seul bloc avec une option "Notifications". Après : **4 onglets distincts** :

| Onglet | Contenu |
|--------|---------|
| **Profil** | Upload de la photo de profil (fichier image → base64 via `FileReader`) + champs prénom/nom + bouton "Enregistrer". Utilise `saveAvatar()` du composable `useAvatar`. |
| **Apparence** | Sélecteur de thème visuel (clair / sombre / système) avec prévisualisation miniature. Bouton de déconnexion. |
| **Notifications** | Toggles : rappels de réunion, sons, invitations. |
| **Réunions** | Toggles : micro coupé par défaut, caméra coupée par défaut. |

L'upload avatar côté frontend lit le fichier sélectionné avec `FileReader.readAsDataURL()`, vérifie la taille (alerte si > 2 Mo), et appelle `saveAvatar()` qui persiste en DB via le backend.

#### `components/barrehorizontale/UserMenu.vue` — affichage de l'avatar

Le bouton du menu utilisateur (en haut à droite du header) affiche maintenant la **photo de profil réelle** si elle est disponible, via `useAvatar()`. Si aucune photo : repli sur les initiales comme avant.

```vue
<img v-if="avatarDataUrl" :src="avatarDataUrl" class="avatar-photo" />
<span v-else class="avatar-text">{{ initials }}</span>
```

Les couleurs en dur du dropdown ont également été migrées vers des variables CSS (thème sombre).

#### `app/pages/dashboard.vue` — intégration du mode sombre

Toutes les couleurs en dur (`#14b8a6`, `white`, `#4b5563`, etc.) ont été remplacées par des variables CSS (`var(--accent-text)`, `var(--bg-sidebar)`, `var(--text-secondary)`, etc.). Cela permet au **mode sombre** défini dans `assets/theme.css` de s'appliquer correctement sur l'ensemble du dashboard sans dupliquer les règles CSS.

---

## Pourquoi ces choix

| Choix | Raison |
|-------|--------|
| **Data URL en base64** | Pas de gestion de fichier serveur ni de stockage objet à configurer. Suffisant pour les avatars (petites images). |
| **Double stratégie cache** (mémoire session + localStorage) | Évite les flashs visuels et les requêtes DB inutiles à chaque appel. |
| **RETURNING dans le UPSERT** | Récupère en une seule requête la valeur persistée (incluant les modifications faites hors session, ex. via admin). |
| **`issuer_url_internal`** | Sépare proprement l'URL vue par le navigateur de celle vue par le backend en Docker — problème courant en développement containerisé. |
| **Variables CSS pour le thème** | Permet au mode sombre de s'activer sans dupliquer les styles ni utiliser de classes JavaScript. |

---

## Sécurité

| Point | Détail |
|-------|--------|
| **Authentification requise** | `update_avatar` extrait et valide le cookie `tg_session` avant toute opération. Sans session valide → `401 Unauthorized`. |
| **Isolation par `keycloak_id`** | La requête `UPDATE` cible uniquement le user identifié par son `keycloak_sub` issu de la session — pas d'input utilisateur sur l'identifiant. Aucune possibilité d'écraser la photo d'un autre utilisateur. |
| **Taille de la data URL** | Non limitée côté backend pour l'instant — à surveiller (un avatar en haute résolution en base64 peut peser plusieurs Mo et saturer la DB). Une validation de taille devra être ajoutée. |
| **Pas de validation du type MIME** | Le backend accepte n'importe quelle chaîne comme `avatar_url`. La validation du préfixe `data:image/` devrait être faite côté frontend ET backend pour éviter l'injection de contenu arbitraire. |
| **`issuer_url_internal` non exposé** | Cette URL interne ne transite jamais vers le client — elle est utilisée uniquement dans les appels serveur-à-serveur (Keycloak token + userinfo). |

---

---

# Session du 15 avril 2026

**Objectif :** Remise en route de l'environnement de développement.

---

## Ce qui a été fait

Redémarrage des deux serveurs de développement :

- **Backend** (`truegather-backend`) — Rust/Axum, lancé via `cargo run` depuis `/backend/`.
  - Écoute sur `0.0.0.0:8082`.
  - Migrations SQLx déjà appliquées (message informatif `relation "_sqlx_migrations" already exists` — normal au redémarrage, ce n'est pas une erreur).

- **Frontend** (`front`) — Nuxt 4.3.1 avec Vite 7.3.1, lancé via `npm run dev` depuis `/frontend/`.
  - Accessible sur `http://localhost:3000/`.

## Pourquoi

Simple redémarrage de session de travail — les processus ne persistaient plus depuis la session précédente.

## Points de sécurité à garder en tête

| Point | Détail |
|-------|--------|
| **Mode développement uniquement** | Les deux serveurs tournent en mode `dev` (profil `[unoptimized + debuginfo]` pour Rust, mode dev Nuxt avec HMR). Ils ne doivent **jamais** être exposés sur un réseau public dans cet état. |
| **Backend sur `0.0.0.0`** | Le backend écoute sur toutes les interfaces réseau locales. En production, mettre un reverse proxy (Nginx, Caddy) devant avec TLS. |
| **Pas de HTTPS en dev** | Les cookies de session (`tg_session`) sont transmis sans chiffrement en local. En production, `Secure` + `HttpOnly` + `SameSite=Strict` doivent être activés et HTTPS obligatoire. |
| **Secrets en `.env`** | Les variables sensibles (secrets TURN, clé session, DSN DB) ne doivent pas être commitées dans git. Vérifier que `.env` est bien dans `.gitignore`. |

---

---

# Session du 15 avril 2026 — Suite : Modal Paramètres, changement de mot de passe

**Objectif :** Documenter le fonctionnement complet de la modal `Paramètres` et implémenter le changement de mot de passe.

---

## La modal Paramètres — comment ça fonctionne

### Vue d'ensemble

La modal Paramètres est dans `components/barrehorizontale/SettingsModal.vue`. Elle s'ouvre depuis le menu utilisateur (bouton en haut à droite du header).

Elle est organisée en **5 onglets** :

```
[ Profil ] [ Apparence ] [ Notifications ] [ Réunions ] [ Sécurité ]
```

Le principe est simple : un tableau `tabs` contient la liste des onglets. Une variable réactive `activeTab` trace lequel est actif. Quand on clique sur un onglet, `activeTab` change, et Vue affiche le bon bloc grâce à des `v-if`.

```typescript
const tabs = [
  { id: 'profil',        label: 'Profil' },
  { id: 'apparence',     label: 'Apparence' },
  { id: 'notifications', label: 'Notifications' },
  { id: 'reunions',      label: 'Réunions' },
  { id: 'securite',      label: 'Sécurité' },
]
const activeTab = ref('profil')  // onglet ouvert par défaut
```

---

### Onglet 1 — Profil

**Ce qu'on voit :** cercle avec la photo (ou les initiales), champs Prénom / Nom, bouton Enregistrer.

#### Photo de profil

| Étape | Ce qui se passe |
|-------|----------------|
| Clic sur le cercle | `triggerFileInput()` → déclenche un `<input type="file" accept="image/*">` invisible |
| Sélection du fichier | `onAvatarChange()` vérifie que le fichier fait moins de 2 Mo |
| Lecture du fichier | `FileReader.readAsDataURL(file)` convertit l'image en **data URL base64** (ex. `data:image/png;base64,...`) |
| Sauvegarde | `saveAvatar(dataUrl)` de `useAvatar` : met à jour l'affichage immédiatement + envoie `PUT /api/v1/auth/avatar` au backend + cache dans `localStorage` |
| Suppression | Clic sur "Supprimer" → `removeAvatar()` → `saveAvatar(null)` supprime côté backend et efface le cache local |

Si aucune photo n'est définie, le cercle affiche les **initiales** calculées depuis le nom de l'utilisateur connecté.

#### Champs Prénom / Nom

Liés à la variable réactive `form` via `v-model`. Le bouton "Enregistrer" appelle `saveProfile()`.

> **Note :** `saveProfile()` est actuellement un placeholder (simule un délai de 800ms). L'appel API `PUT /api/v1/users/me` est à implémenter.

---

### Onglet 2 — Apparence

**Ce qu'on voit :** 3 cartes de thème (Clair / Sombre / Système) avec prévisualisation miniature, et un bouton "Se déconnecter".

#### Sélecteur de thème

| Valeur | Comportement |
|--------|-------------|
| `light` | `document.documentElement.setAttribute('data-theme', 'light')` — force le thème clair |
| `dark` | `document.documentElement.setAttribute('data-theme', 'dark')` — force le thème sombre |
| `system` | `document.documentElement.removeAttribute('data-theme')` — laisse le navigateur choisir selon les préférences OS |

La valeur est **persistée dans `localStorage`** (clé `tg-theme`). Au montage de la modal, le thème sauvegardé est relu et appliqué. Au montage global de `app.vue`, le même mécanisme s'applique pour que le thème soit actif dès le chargement de la page.

Le changement de thème est **instantané** — aucun rechargement de page, aucun appel API. Tout se fait via l'attribut `data-theme` sur `<html>`, que les variables CSS de `assets/theme.css` écoutent.

#### Bouton Se déconnecter

Appelle `logout()` de `useAuth`, qui supprime la session côté backend et redirige vers la page de login.

---

### Onglet 3 — Notifications

**Ce qu'on voit :** 3 toggles ON/OFF.

| Toggle | Variable | Par défaut |
|--------|----------|-----------|
| Rappels de réunion | `prefs.meetingReminders` | ON |
| Sons de notification | `prefs.notifSound` | ON |
| Invitations aux réunions | `prefs.meetingInvites` | ON |

Les toggles sont **visuellement réactifs** — cliquer l'un d'eux inverse immédiatement sa valeur (`!prefs.xxx`). Ce sont des préférences locales pour l'instant (non persistées en base).

---

### Onglet 4 — Réunions

**Ce qu'on voit :** 2 toggles ON/OFF.

| Toggle | Variable | Par défaut |
|--------|----------|-----------|
| Micro coupé par défaut | `prefs.mutedByDefault` | OFF |
| Caméra coupée par défaut | `prefs.cameraOffByDefault` | OFF |

Même principe que l'onglet Notifications — réactifs visuellement, à connecter plus tard à la logique WebRTC.

---

### Onglet 5 — Sécurité

**Ce qu'on voit :** 3 champs mot de passe + bouton "Modifier".

#### Flux complet du changement de mot de passe

```
Utilisateur saisit :
  ├── Mot de passe actuel
  ├── Nouveau mot de passe
  └── Confirmation

Frontend valide :
  ├── Champ actuel non vide
  ├── Nouveau mot de passe ≥ 14 caractères
  └── Nouveau = Confirmation  →  sinon : message d'erreur rouge, arrêt

Frontend envoie : PUT /api/v1/auth/password
  Body JSON : { current_password, new_password, confirm_password }

Backend valide à nouveau (sécurité côté serveur) :
  ├── Session valide (cookie tg_session)  →  sinon 401
  ├── new_password = confirm_password     →  sinon 400
  └── new_password ≥ 14 caractères        →  sinon 400

Backend vérifie le mot de passe actuel (ROPC Keycloak) :
  POST /realms/truegather/protocol/openid-connect/token
  grant_type=password, username=email, password=current_password
  →  Si Keycloak refuse : 400 "Mot de passe actuel incorrect"

Backend obtient un token admin (client_credentials) :
  POST /realms/truegather/protocol/openid-connect/token
  grant_type=client_credentials

Backend met à jour le mot de passe (API admin Keycloak) :
  PUT /admin/realms/truegather/users/{keycloak_id}/reset-password
  Body : { type: "password", value: new_password, temporary: false }

Frontend affiche :
  ├── Succès → message vert, champs vidés
  └── Erreur → message rouge avec le détail
```

#### Pourquoi passer par Keycloak et pas par la DB ?

Les mots de passe ne sont **jamais stockés dans notre base PostgreSQL**. Keycloak est le seul gestionnaire d'identité — il détient les hash des mots de passe. Modifier le mot de passe directement en DB ne fonctionnerait pas : il faut forcément passer par l'API Keycloak.

---

## Ce qui est à finir

| Élément | État |
|---------|------|
| `saveProfile()` (Prénom/Nom) | Placeholder — l'API `PUT /api/v1/users/me` est à créer |
| Persistance des préférences (Notifications/Réunions) | Stockées en mémoire uniquement — à sauvegarder en DB |
| Changement de mot de passe | **Fonctionnel côté backend et frontend** |
| Photo de profil | **Fonctionnelle** |
| Thème sombre | **Fonctionnel** |
| **Email de confirmation après changement de mot de passe** | **À faire** — voir ci-dessous |

### Email après changement de mot de passe — à implémenter

Après un changement de mot de passe réussi, l'utilisateur doit recevoir un **email de confirmation de sécurité**. Cela lui permet d'être alerté si le changement est frauduleux (quelqu'un a pris sa session).

**Ce qu'il faut faire :**

#### 1. Implémenter `src/mail/mod.rs`

Le module existe mais est vide. Il faudra choisir une crate d'envoi d'email et implémenter une fonction `send_password_changed_notification` :

```
Expéditeur : noreply@truegather.app
Destinataire : session.email
Sujet : "Votre mot de passe TrueGather a été modifié"
Corps : "Bonjour, votre mot de passe a été modifié le [date] à [heure].
         Si vous n'êtes pas à l'origine de cette modification, contactez-nous."
```

Crates candidates :
- `lettre` (0.11) — la référence en Rust, support SMTP + STARTTLS
- `sendgrid` — si on utilise SendGrid comme provider

Variables `.env` à prévoir :
```
APP__MAIL__SMTP_HOST=smtp.monservice.com
APP__MAIL__SMTP_PORT=587
APP__MAIL__SMTP_USER=...
APP__MAIL__SMTP_PASSWORD=...
APP__MAIL__FROM=noreply@truegather.app
```

#### 2. Appeler `send_password_changed_notification` à la fin de `change_password`

Dans `src/auth/handlers.rs`, après le `reset_resp` Keycloak réussi :

```rust
// Envoi de l'email de confirmation (non bloquant — on ne fait pas échouer
// le changement de mot de passe si l'envoi d'email rate).
if let Err(e) = mail::send_password_changed_notification(&state, &session.email).await {
    tracing::warn!("Failed to send password change notification: {}", e);
}
```

> L'envoi d'email doit être **non bloquant** : si le serveur SMTP est indisponible, le changement de mot de passe a déjà réussi — on ne doit pas retourner une erreur à l'utilisateur pour ça. On log juste un warning.

---

## Sécurité

| Point | Détail |
|-------|--------|
| **Double validation** | Les règles (14 caractères, correspondance) sont vérifiées côté frontend ET backend. Impossible de les contourner en appelant l'API directement. |
| **Vérification du mot de passe actuel** | Via le flow ROPC Keycloak : on ne peut pas changer le mot de passe de quelqu'un sans connaître son mot de passe actuel. |
| **Pas d'accès au hash** | Le backend ne lit jamais le hash du mot de passe. La vérification se fait entièrement via le protocole OAuth2/OIDC de Keycloak. |
| **Token admin éphémère** | Le token admin obtenu pour mettre à jour le mot de passe n'est jamais stocké — il est utilisé dans la même requête et oublié. |
| **Prérequis Keycloak** | Le service account du client `truegather-backend` doit avoir le rôle `manage-users` dans `realm-management` de Keycloak pour que l'étape admin fonctionne. |
