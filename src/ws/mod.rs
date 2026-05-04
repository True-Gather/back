// Serveur de signalisation WebRTC — P2P (≤ 2 participants) + SFU (≥ 3 participants).
//
// ─── Topologies ──────────────────────────────────────────────────────────────
//
//  P2P (mode = "p2p", ≤ 2 participants) :
//    Chaque pair se connecte directement aux autres via des RTCPeerConnections.
//    Le backend relaie uniquement les messages SDP et ICE candidates.
//    Flux média : direct navigateur → navigateur (SRTP).
//
//  SFU (mode = "sfu", ≥ 3 participants) :
//    Chaque pair se connecte au SFU côté serveur (1 RTCPeerConnection par client).
//    Le SFU relaye les paquets RTP entre clients sans les décoder ni les stocker.
//    Flux média : navigateur → SFU → navigateurs (SRTP de bout en bout).
//
// ─── Basculement P2P → SFU ───────────────────────────────────────────────────
//
//  Quand le 3ème participant rejoint une room P2P :
//    1. Backend envoie { type: "mode_switch", mode: "sfu" } à tous.
//    2. Backend crée une RTCPeerConnection SFU pour chaque participant.
//    3. SFU envoie { type: "sfu_offer" } à chaque participant.
//    4. Clients répondent avec { type: "sfu_answer" }.
//    5. Échange ICE via { type: "sfu_ice_candidate" }.
//
// ─── Sécurité ─────────────────────────────────────────────────────────────────
//
//  - Le backend ne lit JAMAIS les flux média (ni en P2P, ni en SFU).
//  - Chiffrement SRTP par défaut (imposé par la spec WebRTC).
//  - Credentials TURN temporels (RFC 5766 / coturn --use-auth-secret).
//  - Rate-limiting WebSocket implicite via MAX_SFU_SIZE.

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
    state::{AppState, RoomMode},
    webrtc_engine,
    webrtc_engine::sfu,
};

// ─── Messages client → serveur ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    // ── Signalisation commune ─────────────────────────────────────────────────

    // Rejoindre une room de signalisation.
    Join { room_id: String },

    // Quitter explicitement une room.
    Leave { room_id: String },

    // ── Signalisation P2P (pair ↔ pair) ──────────────────────────────────────

    // Envoyer une SDP Offer à un pair spécifique (mode P2P).
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

    // Envoyer un candidat ICE à un pair spécifique (mode P2P).
    IceCandidate {
        room_id: String,
        to: Uuid,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_m_line_index: Option<u32>,
    },

    // ── Signalisation SFU (client ↔ serveur SFU) ──────────────────────────────

    // SDP Answer du client vers le SFU (réponse à sfu_offer).
    SfuAnswer { room_id: String, sdp: String },

    // Candidat ICE du client vers le SFU.
    SfuIceCandidate {
        room_id: String,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
    },

    // ── Chat ─────────────────────────────────────────────────────────────────

    // Message de chat : broadcast à tous les membres de la room.
    ChatMessage { room_id: String, text: String },
}

// ─── Messages serveur → client ───────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMessage {
    // ── Signalisation commune ─────────────────────────────────────────────────

    // Confirmation d'entrée dans une room + liste des pairs déjà connectés.
    // `mode` indique si la room est en mode P2P ou SFU.
    // `ice_servers` contient les URLs STUN/TURN à configurer côté client.
    Joined {
        room_id: String,
        user_id: Uuid,
        peers: Vec<Uuid>,
        ice_servers: Vec<webrtc_engine::IceServerInfo>,
        mode: String,
    },

    // Un nouveau pair a rejoint la room.
    PeerJoined { room_id: String, user_id: Uuid },

    // Un pair a quitté la room.
    PeerLeft { room_id: String, user_id: Uuid },

    // La room bascule de P2P vers SFU (3ème participant).
    // Les clients doivent fermer leurs RTCPeerConnections P2P
    // et attendre un `sfu_offer` du serveur.
    ModeSwitch { room_id: String, mode: String, peers: Vec<Uuid> },

    // ── Signalisation P2P (pair ↔ pair) ──────────────────────────────────────

    // SDP Offer reçue d'un pair (mode P2P).
    Offer {
        room_id: String,
        from: Uuid,
        sdp: String,
    },

    // SDP Answer reçue d'un pair (mode P2P).
    Answer {
        room_id: String,
        from: Uuid,
        sdp: String,
    },

    // Candidat ICE reçu d'un pair (mode P2P).
    IceCandidate {
        room_id: String,
        from: Uuid,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_m_line_index: Option<u32>,
    },

    // ── Chat ─────────────────────────────────────────────────────────────────

    // Message de chat diffusé à tous les membres de la room.
    ChatBroadcast { room_id: String, from: Uuid, text: String },

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

        // ── Signalisation SFU (client → serveur) ─────────────────────────────

        ClientMessage::SfuAnswer { room_id, sdp } => {
            if let Err(e) = webrtc_engine::validate_sdp(&sdp) {
                let _ = peer_tx.send(
                    serde_json::to_string(&ServerMessage::Error {
                        message: e.to_string(),
                    })
                    .unwrap_or_default(),
                );
                return;
            }
            if let Err(e) = sfu::handle_answer(&state.sfu_state, &room_id, user_id, &sdp).await {
                tracing::warn!("[ws] sfu_answer error: {e}");
            }
        }

        ClientMessage::SfuIceCandidate {
            room_id,
            candidate,
            sdp_mid,
            sdp_mline_index,
        } => {
            if let Err(e) = webrtc_engine::validate_ice_candidate(&candidate) {
                let _ = peer_tx.send(
                    serde_json::to_string(&ServerMessage::Error {
                        message: e.to_string(),
                    })
                    .unwrap_or_default(),
                );
                return;
            }
            if let Err(e) = sfu::handle_ice_candidate(
                &state.sfu_state,
                &room_id,
                user_id,
                &candidate,
                sdp_mid,
                sdp_mline_index,
            )
            .await
            {
                tracing::warn!("[ws] sfu_ice_candidate error: {e}");
            }
        }

        // ── Chat ─────────────────────────────────────────────────────────────

        ClientMessage::ChatMessage { room_id, text } => {
            const MAX_CHAT_LEN: usize = 2_000;
            if text.len() > MAX_CHAT_LEN {
                let _ = peer_tx.send(
                    serde_json::to_string(&ServerMessage::Error {
                        message: format!("Message trop long (max {MAX_CHAT_LEN} caractères)."),
                    })
                    .unwrap_or_default(),
                );
                return;
            }
            broadcast_to_room(
                state,
                &room_id,
                &ServerMessage::ChatBroadcast {
                    room_id: room_id.clone(),
                    from: user_id,
                    text,
                },
            )
            .await;
        }
    }
}

// ─── Seuils de topologie ──────────────────────────────────────────────────────

/// Nombre maximum de participants en mode P2P mesh.
/// Au-delà, la room bascule automatiquement en mode SFU.
const MAX_P2P_SIZE: usize = 2;

/// Nombre maximum de participants en mode SFU.
const MAX_SFU_SIZE: usize = 10;

async fn join_room(
    state: &AppState,
    room_id: &str,
    user_id: Uuid,
    sender: mpsc::UnboundedSender<String>,
) {
    // 1. Lire les pairs déjà présents ET le mode de la room.
    let (existing_peers, current_mode) = {
        let rooms = state.signaling_rooms.read().await;
        let peers: Vec<Uuid> = rooms
            .get(room_id)
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default();
        let modes = state.room_modes.read().await;
        let mode = modes.get(room_id).cloned();
        (peers, mode)
    };

    let total_after = existing_peers.len() + 1;

    // 2. Vérifier la limite de taille.
    let max_size = match &current_mode {
        Some(RoomMode::Sfu) | None => MAX_SFU_SIZE,
        Some(RoomMode::P2p) => MAX_P2P_SIZE,
    };

    if existing_peers.len() >= max_size {
        let _ = sender.send(
            serde_json::to_string(&ServerMessage::Error {
                message: format!("Room pleine : maximum {max_size} participants."),
            })
            .unwrap_or_default(),
        );
        return;
    }

    // 3. Enregistrer dans la map mémoire.
    {
        let mut rooms = state.signaling_rooms.write().await;
        rooms
            .entry(room_id.to_string())
            .or_default()
            .insert(user_id, sender.clone());
    }

    // 4. Enregistrer dans Redis (best-effort).
    if let Err(e) = redis::room_add_member(&state.redis, room_id, user_id).await {
        tracing::warn!("Redis room_add_member failed: {e}");
    }

    // 5. Construire les ICE servers une seule fois.
    let ice_servers = webrtc_engine::build_ice_servers(&state.config.turn).unwrap_or_else(|e| {
        tracing::warn!("build_ice_servers failed: {e}");
        webrtc_engine::build_ice_servers(&Default::default()).unwrap_or_default()
    });

    // ─── Décision P2P vs SFU ─────────────────────────────────────────────────

    let switching_to_sfu = current_mode.is_none() && total_after > MAX_P2P_SIZE;
    let already_sfu = current_mode == Some(RoomMode::Sfu);
    let new_mode_str = if switching_to_sfu || already_sfu { "sfu" } else { "p2p" };

    // 6a. La room bascule de P2P vers SFU (3ème participant).
    if switching_to_sfu {
        // Mettre à jour le mode.
        {
            let mut modes = state.room_modes.write().await;
            modes.insert(room_id.to_string(), RoomMode::Sfu);
        }

        // Collecter TOUS les participants (existants + nouveau).
        let all_peers: Vec<Uuid> = {
            let rooms = state.signaling_rooms.read().await;
            rooms
                .get(room_id)
                .map(|m| m.keys().copied().collect())
                .unwrap_or_default()
        };

        // Envoyer mode_switch aux pairs EXISTANTS (pas au nouveau, il reçoit "joined" directement).
        let mode_switch_msg = serde_json::to_string(&ServerMessage::ModeSwitch {
            room_id: room_id.to_string(),
            mode: "sfu".to_string(),
            peers: all_peers.clone(),
        })
        .unwrap_or_default();

        {
            let rooms = state.signaling_rooms.read().await;
            if let Some(members) = rooms.get(room_id) {
                for &pid in &existing_peers {
                    if let Some(tx) = members.get(&pid) {
                        let _ = tx.send(mode_switch_msg.clone());
                    }
                }
            }
        }

        // Initialiser le SFU pour chaque participant (existants + nouveau).
        for &pid in &all_peers {
            let ws_tx = {
                let rooms = state.signaling_rooms.read().await;
                rooms
                    .get(room_id)
                    .and_then(|m| m.get(&pid))
                    .cloned()
            };
            let Some(ws_tx) = ws_tx else { continue };

            // Un nouveau participant ne reçoit pas de relay tracks des autres encore
            // (ils n'ont pas encore envoyé leurs flux). Pour les participants existants,
            // les tracks arriveront via on_track quand les clients reconnectent au SFU.
            let existing_relays = sfu::get_existing_relays(&state.sfu_state, room_id, pid).await;

            if let Err(e) = sfu::add_participant(
                &state.sfu_state,
                &state.config.turn,
                room_id,
                pid,
                ws_tx,
                existing_relays,
            )
            .await
            {
                tracing::warn!("[ws] sfu add_participant failed for {pid}: {e}");
            }
        }

        // Confirmer l'entrée au nouveau participant (avec mode = "sfu").
        let joined_msg = serde_json::to_string(&ServerMessage::Joined {
            room_id: room_id.to_string(),
            user_id,
            peers: existing_peers.clone(),
            ice_servers,
            mode: "sfu".to_string(),
        })
        .unwrap_or_default();
        let _ = sender.send(joined_msg);

        return;
    }

    // 6b. La room est déjà en mode SFU : ajouter le nouveau participant au SFU.
    if already_sfu {
        let existing_relays =
            sfu::get_existing_relays(&state.sfu_state, room_id, user_id).await;

        if let Err(e) = sfu::add_participant(
            &state.sfu_state,
            &state.config.turn,
            room_id,
            user_id,
            sender.clone(),
            existing_relays,
        )
        .await
        {
            tracing::warn!("[ws] sfu add_participant (existing sfu room) failed: {e}");
        }

        let joined_msg = serde_json::to_string(&ServerMessage::Joined {
            room_id: room_id.to_string(),
            user_id,
            peers: existing_peers.clone(),
            ice_servers,
            mode: "sfu".to_string(),
        })
        .unwrap_or_default();
        let _ = sender.send(joined_msg);

        // Notifier les pairs existants (ils verront le flux arriver via on_track SFU).
        let peer_joined = serde_json::to_string(&ServerMessage::PeerJoined {
            room_id: room_id.to_string(),
            user_id,
        })
        .unwrap_or_default();
        let rooms = state.signaling_rooms.read().await;
        if let Some(members) = rooms.get(room_id) {
            for &pid in &existing_peers {
                if let Some(tx) = members.get(&pid) {
                    let _ = tx.send(peer_joined.clone());
                }
            }
        }

        return;
    }

    // 6c. Mode P2P (≤ 2 participants) : flux de signalisation standard.

    // Mettre le mode à P2P si la room vient d'être créée.
    if current_mode.is_none() {
        let mut modes = state.room_modes.write().await;
        modes.insert(room_id.to_string(), RoomMode::P2p);
    }

    // Confirmer l'entrée (mode = "p2p").
    let joined_msg = serde_json::to_string(&ServerMessage::Joined {
        room_id: room_id.to_string(),
        user_id,
        peers: existing_peers.clone(),
        ice_servers,
        mode: new_mode_str.to_string(),
    })
    .unwrap_or_default();
    let _ = sender.send(joined_msg);

    // Notifier les pairs existants.
    let peer_joined = serde_json::to_string(&ServerMessage::PeerJoined {
        room_id: room_id.to_string(),
        user_id,
    })
    .unwrap_or_default();
    let rooms = state.signaling_rooms.read().await;
    if let Some(members) = rooms.get(room_id) {
        for &pid in &existing_peers {
            if let Some(tx) = members.get(&pid) {
                let _ = tx.send(peer_joined.clone());
            }
        }
    }
}

// Retire un pair d'une room : nettoyage mémoire + SFU + Redis,
// puis notification des pairs restants.
async fn leave_room(state: &AppState, room_id: &str, user_id: Uuid) {
    // 1. Supprimer de la map mémoire. Vérifier si la room devient vide.
    let room_empty = {
        let mut rooms = state.signaling_rooms.write().await;
        if let Some(members) = rooms.get_mut(room_id) {
            members.remove(&user_id);
            if members.is_empty() {
                rooms.remove(room_id);
                true
            } else {
                false
            }
        } else {
            true
        }
    };

    // 2. Nettoyage SFU (best-effort).
    sfu::remove_participant(&state.sfu_state, room_id, user_id).await;

    // 3. Si la room est vide, supprimer son mode.
    if room_empty {
        let mut modes = state.room_modes.write().await;
        modes.remove(room_id);
    }

    // 4. Supprimer de Redis (best-effort).
    if let Err(e) = redis::room_remove_member(&state.redis, room_id, user_id).await {
        tracing::warn!("Redis room_remove_member failed: {e}");
    }

    // 5. Notifier les pairs restants.
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

// Diffuse un message sérialisé à tous les membres d'une room.
async fn broadcast_to_room(state: &AppState, room_id: &str, msg: &ServerMessage) {
    let json = match serde_json::to_string(msg) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Serialization error in broadcast_to_room: {e}");
            return;
        }
    };

    let rooms = state.signaling_rooms.read().await;
    let Some(members) = rooms.get(room_id) else {
        return;
    };
    for tx in members.values() {
        let _ = tx.send(json.clone());
    }
}

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
