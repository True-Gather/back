// Module de signalisation WebRTC peer-to-peer.
//
// Ce serveur de signalisation ne transporte jamais les flux média.
// Il sert uniquement à échanger les messages SDP (Offer / Answer)
// et les candidats ICE entre les pairs d'une même room.
//
// ─── Flux général ────────────────────────────────────────────────────────────
//
//  1. Le client ouvre une connexion WebSocket sur /ws/signal
//     avec son cookie de session `tg_session`.
//  2. Le backend vérifie la session en mémoire → extrait user_id.
//     Si la session est invalide, la connexion est rejetée (401).
//  3. Le client envoie { "type": "join", "room_id": "..." }.
//     Le backend :
//       - enregistre le pair dans la map mémoire (SignalingRooms),
//       - enregistre le pair dans Redis (room:<id>:members),
//       - renvoie { "type": "joined", "peers": [...] },
//       - notifie les autres pairs avec { "type": "peer_joined" }.
//  4. Les messages Offer / Answer / IceCandidate sont routés
//     vers le pair `to` via son canal mpsc.
//  5. À la déconnexion (ou suite à un Leave), le pair est retiré
//     de la room et les autres sont notifiés.

use axum::{
    Router,
    extract::{State, WebSocketUpgrade, ws::Message},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    auth::session::extract_session_id_from_headers,
    redis,
    state::AppState,
    webrtc_engine,
};

// ─── Messages client → serveur ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    // Rejoindre une room de signalisation.
    Join { room_id: String },

    // Quitter explicitement une room.
    Leave { room_id: String },

    // Envoyer une SDP Offer à un pair spécifique.
    Offer {
        room_id: String,
        to: Uuid,
        sdp: String,
    },

    // Envoyer une SDP Answer à un pair spécifique.
    Answer {
        room_id: String,
        to: Uuid,
        sdp: String,
    },

    // Envoyer un candidat ICE à un pair spécifique.
    IceCandidate {
        room_id: String,
        to: Uuid,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_m_line_index: Option<u32>,
    },
}

// ─── Messages serveur → client ───────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMessage {
    // Confirmation d'entrée dans une room + liste des pairs déjà connectés.
    // `ice_servers` contient les URLs STUN/TURN à configurer côté client.
    Joined {
        room_id: String,
        user_id: Uuid,
        peers: Vec<Uuid>,
        ice_servers: Vec<webrtc_engine::IceServerInfo>,
    },

    // Un nouveau pair a rejoint la room.
    PeerJoined { room_id: String, user_id: Uuid },

    // Un pair a quitté la room.
    PeerLeft { room_id: String, user_id: Uuid },

    // SDP Offer reçue d'un pair.
    Offer {
        room_id: String,
        from: Uuid,
        sdp: String,
    },

    // SDP Answer reçue d'un pair.
    Answer {
        room_id: String,
        from: Uuid,
        sdp: String,
    },

    // Candidat ICE reçu d'un pair.
    IceCandidate {
        room_id: String,
        from: Uuid,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_m_line_index: Option<u32>,
    },

    // Erreur renvoyée au client.
    Error { message: String },
}

// ─── Route ───────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new().route("/ws/signal", get(ws_handler))
}

// ─── Handler d'upgrade WebSocket ─────────────────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Extraction de la session depuis le cookie HTTP.
    let user_id = resolve_user_id(&state, &headers).await;

    match user_id {
        Some(uid) => ws
            .on_upgrade(move |socket| handle_socket(socket, state, uid))
            .into_response(),
        None => (StatusCode::UNAUTHORIZED, "Session invalide ou absente").into_response(),
    }
}

// Résout le user_id à partir du cookie de session présent dans les headers.
async fn resolve_user_id(state: &AppState, headers: &HeaderMap) -> Option<Uuid> {
    let session_id =
        extract_session_id_from_headers(headers, &state.config.auth.cookie_name)?;
    let sessions = state.sessions.read().await;
    sessions.get(&session_id).map(|s| s.user_id)
}

// ─── Boucle principale par connexion ─────────────────────────────────────────

async fn handle_socket(
    socket: axum::extract::ws::WebSocket,
    state: AppState,
    user_id: Uuid,
) {
    // Séparation du WebSocket en un émetteur et un récepteur.
    let (mut ws_sink, mut ws_stream) = socket.split();

    // Canal mpsc : les messages routés par d'autres pairs arrivent ici
    // pour être ensuite envoyés sur le WebSocket de cet utilisateur.
    let (peer_tx, mut peer_rx) = mpsc::unbounded_channel::<String>();

    // Tâche de transfert : canal mpsc → WebSocket sink.
    let forward_task = tokio::spawn(async move {
        while let Some(json) = peer_rx.recv().await {
            if ws_sink
                .send(Message::Text(json.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Room courante de cet utilisateur (un seul à la fois).
    let mut current_room: Option<String> = None;

    // Boucle de lecture des messages WebSocket entrants.
    while let Some(Ok(raw)) = ws_stream.next().await {
        match raw {
            Message::Text(text) => {
                handle_client_message(
                    text.as_str(),
                    &state,
                    user_id,
                    &peer_tx,
                    &mut current_room,
                )
                .await;
            }
            // Ping automatiquement géré par axum ; on ignore les autres frames.
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Nettoyage à la déconnexion.
    if let Some(room_id) = current_room.take() {
        leave_room(&state, &room_id, user_id).await;
    }

    // Arrêt propre de la tâche de transfert.
    forward_task.abort();
}

// ─── Dispatch des messages clients ───────────────────────────────────────────

async fn handle_client_message(
    raw: &str,
    state: &AppState,
    user_id: Uuid,
    peer_tx: &mpsc::UnboundedSender<String>,
    current_room: &mut Option<String>,
) {
    // Désérialisation du message JSON.
    let msg = match serde_json::from_str::<ClientMessage>(raw) {
        Ok(m) => m,
        Err(_) => {
            let _ = peer_tx.send(
                serde_json::to_string(&ServerMessage::Error {
                    message: "Message JSON invalide".to_string(),
                })
                .unwrap_or_default(),
            );
            return;
        }
    };

    match msg {
        ClientMessage::Join { room_id } => {
            // Quitter l'éventuelle room précédente avant d'en rejoindre une.
            if let Some(prev) = current_room.take() {
                leave_room(state, &prev, user_id).await;
            }
            join_room(state, &room_id, user_id, peer_tx.clone()).await;
            *current_room = Some(room_id);
        }

        ClientMessage::Leave { room_id } => {
            leave_room(state, &room_id, user_id).await;
            if current_room.as_deref() == Some(&room_id) {
                *current_room = None;
            }
        }

        ClientMessage::Offer { room_id, to, sdp } => {
            // Validation SDP avant de router.
            if let Err(e) = webrtc_engine::validate_sdp(&sdp) {
                let _ = peer_tx.send(
                    serde_json::to_string(&ServerMessage::Error {
                        message: e.to_string(),
                    })
                    .unwrap_or_default(),
                );
                return;
            }
            route_to_peer(
                state,
                &room_id,
                to,
                &ServerMessage::Offer {
                    room_id: room_id.clone(),
                    from: user_id,
                    sdp,
                },
            )
            .await;
        }

        ClientMessage::Answer { room_id, to, sdp } => {
            // Validation SDP avant de router.
            if let Err(e) = webrtc_engine::validate_sdp(&sdp) {
                let _ = peer_tx.send(
                    serde_json::to_string(&ServerMessage::Error {
                        message: e.to_string(),
                    })
                    .unwrap_or_default(),
                );
                return;
            }
            route_to_peer(
                state,
                &room_id,
                to,
                &ServerMessage::Answer {
                    room_id: room_id.clone(),
                    from: user_id,
                    sdp,
                },
            )
            .await;
        }

        ClientMessage::IceCandidate {
            room_id,
            to,
            candidate,
            sdp_mid,
            sdp_m_line_index,
        } => {
            // Validation du candidat ICE avant de router.
            if let Err(e) = webrtc_engine::validate_ice_candidate(&candidate) {
                let _ = peer_tx.send(
                    serde_json::to_string(&ServerMessage::Error {
                        message: e.to_string(),
                    })
                    .unwrap_or_default(),
                );
                return;
            }
            route_to_peer(
                state,
                &room_id,
                to,
                &ServerMessage::IceCandidate {
                    room_id: room_id.clone(),
                    from: user_id,
                    candidate,
                    sdp_mid,
                    sdp_m_line_index,
                },
            )
            .await;
        }
    }
}

// ─── Gestion des rooms ────────────────────────────────────────────────────────

// Ajoute un pair dans une room : enregistrement mémoire + Redis,
// puis envoi des notifications.
async fn join_room(
    state: &AppState,
    room_id: &str,
    user_id: Uuid,
    sender: mpsc::UnboundedSender<String>,
) {
    // 1. Récupérer les pairs déjà présents AVANT d'ajouter le nouvel entrant.
    let existing_peers: Vec<Uuid> = {
        let rooms = state.signaling_rooms.read().await;
        rooms
            .get(room_id)
            .map(|members| members.keys().copied().collect())
            .unwrap_or_default()
    };

    // 2. Enregistrer dans la map mémoire.
    {
        let mut rooms = state.signaling_rooms.write().await;
        rooms
            .entry(room_id.to_string())
            .or_default()
            .insert(user_id, sender.clone());
    }

    // 3. Enregistrer dans Redis (best-effort : erreur non fatale).
    if let Err(e) = redis::room_add_member(&state.redis, room_id, user_id).await {
        tracing::warn!("Redis room_add_member failed: {e}");
    }

    // 4. Confirmer l'entrée au pair qui vient de rejoindre.
    // On inclut les serveurs ICE (STUN/TURN) pour que le frontend
    // configure son RTCPeerConnection avec les mêmes serveurs.
    // Les credentials TURN sont temporels : générés à la volée, invalides après TTL.
    let ice_servers = webrtc_engine::build_ice_servers(&state.config.turn)
        .unwrap_or_else(|e| {
            tracing::warn!("build_ice_servers failed: {e}");
            webrtc_engine::build_ice_servers(&Default::default()).unwrap_or_default()
        });
    let joined_msg = serde_json::to_string(&ServerMessage::Joined {
        room_id: room_id.to_string(),
        user_id,
        peers: existing_peers.clone(),
        ice_servers,
    })
    .unwrap_or_default();
    let _ = sender.send(joined_msg);

    // 5. Notifier les pairs déjà présents.
    let peer_joined_msg = serde_json::to_string(&ServerMessage::PeerJoined {
        room_id: room_id.to_string(),
        user_id,
    })
    .unwrap_or_default();

    let rooms = state.signaling_rooms.read().await;
    if let Some(members) = rooms.get(room_id) {
        for peer_id in &existing_peers {
            if let Some(tx) = members.get(peer_id) {
                let _ = tx.send(peer_joined_msg.clone());
            }
        }
    }
}

// Retire un pair d'une room : nettoyage mémoire + Redis,
// puis notification des pairs restants.
async fn leave_room(state: &AppState, room_id: &str, user_id: Uuid) {
    // 1. Supprimer de la map mémoire.
    {
        let mut rooms = state.signaling_rooms.write().await;
        if let Some(members) = rooms.get_mut(room_id) {
            members.remove(&user_id);
            // Supprimer la room si elle est vide.
            if members.is_empty() {
                rooms.remove(room_id);
            }
        }
    }

    // 2. Supprimer de Redis (best-effort).
    if let Err(e) = redis::room_remove_member(&state.redis, room_id, user_id).await {
        tracing::warn!("Redis room_remove_member failed: {e}");
    }

    // 3. Notifier les pairs restants.
    let peer_left_msg = serde_json::to_string(&ServerMessage::PeerLeft {
        room_id: room_id.to_string(),
        user_id,
    })
    .unwrap_or_default();

    let rooms = state.signaling_rooms.read().await;
    if let Some(members) = rooms.get(room_id) {
        for tx in members.values() {
            let _ = tx.send(peer_left_msg.clone());
        }
    }
}

// ─── Routage d'un message vers un pair ───────────────────────────────────────

// Envoie un message sérialisé au canal mpsc d'un pair cible dans une room.
async fn route_to_peer(state: &AppState, room_id: &str, to: Uuid, msg: &ServerMessage) {
    let json = match serde_json::to_string(msg) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Serialization error when routing to peer: {e}");
            return;
        }
    };

    let rooms = state.signaling_rooms.read().await;
    let Some(members) = rooms.get(room_id) else {
        tracing::debug!("route_to_peer: room {room_id} not found");
        return;
    };
    let Some(tx) = members.get(&to) else {
        tracing::debug!("route_to_peer: peer {to} not found in room {room_id}");
        return;
    };

    if tx.send(json).is_err() {
        tracing::debug!("route_to_peer: peer {to} channel closed");
    }
}
