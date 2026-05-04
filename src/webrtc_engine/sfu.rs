//! SFU (Selective Forwarding Unit) — relay WebRTC pour les rooms de groupe (3+).
//!
//! ## Architecture
//!
//! Chaque participant a UNE RTCPeerConnection vers le SFU côté serveur.
//!
//! ```text
//! Client A ──publish──→ SFU ──relay──→ Client B
//!          ←─subscribe──     ←─relay── Client C
//!
//! Client B ──publish──→ SFU ──relay──→ Client A
//! Client C ──publish──→ SFU ──relay──→ Client A, B
//! ```
//!
//! ## Invariants de sécurité
//!
//! - Le SFU NE décode et n'analyse JAMAIS les paquets RTP.
//! - Les flux sont chiffrés SRTP de bout en bout entre clients et SFU.
//! - Les relay tracks sont des pipes opaques (write_rtp = copie brute).
//! - Aucun flux n'est stocké ou enregistré.
//!
//! ## Séquence de signalisation
//!
//! ```text
//! Server → Client : sfu_offer  (SDP de l'offre SFU)
//! Client → Server : sfu_answer (SDP de la réponse)
//! Both directions : sfu_ice_candidate
//! ```

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors, media_engine::MediaEngine, APIBuilder,
    },
    ice_transport::{ice_candidate::RTCIceCandidate, ice_server::RTCIceServer},
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration, sdp::session_description::RTCSessionDescription,
        RTCPeerConnection,
    },
    rtp_transceiver::{
        rtp_codec::RTPCodecType, rtp_transceiver_direction::RTCRtpTransceiverDirection,
        RTCRtpTransceiverInit,
    },
    track::track_local::{track_local_static_rtp::TrackLocalStaticRTP, TrackLocalWriter},
};

use crate::{
    config::TurnConfig,
    error::{AppError, AppResult},
};

// ─── Types publics ────────────────────────────────────────────────────────────

/// État partagé du SFU, protégé par un Mutex tokio.
pub type SfuState = Arc<Mutex<SfuInner>>;

/// Crée un nouvel état SFU vide.
pub fn new_sfu_state() -> SfuState {
    Arc::new(Mutex::new(SfuInner::default()))
}

// ─── État interne ─────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct SfuInner {
    /// rooms actives : room_id → SfuRoom
    pub rooms: HashMap<String, SfuRoom>,
}

/// Une room SFU : contient les participants et leurs relay tracks.
pub struct SfuRoom {
    /// user_id → SfuParticipant (peer connection serveur + canal WS)
    pub participants: HashMap<Uuid, SfuParticipant>,
    /// Relay tracks publiées par chaque participant :
    /// user_id → [audio_relay, video_relay, ...]
    /// Ces tracks sont des TrackLocalStaticRTP que l'on write_rtp() à chaque paquet reçu.
    pub relay_tracks: HashMap<Uuid, Vec<Arc<TrackLocalStaticRTP>>>,
}

impl SfuRoom {
    pub fn new() -> Self {
        Self {
            participants: HashMap::new(),
            relay_tracks: HashMap::new(),
        }
    }
}

/// Un participant côté serveur SFU.
pub struct SfuParticipant {
    /// Peer connection WebRTC côté serveur.
    pub peer_connection: Arc<RTCPeerConnection>,
    /// Canal WebSocket pour envoyer des messages de signalisation à ce client.
    pub ws_tx: mpsc::UnboundedSender<String>,
}

// ─── API publique ─────────────────────────────────────────────────────────────

/// Ajoute un participant à une room SFU.
///
/// Crée une RTCPeerConnection côté serveur, configure le relay des tracks,
/// ajoute les relay tracks déjà existantes (des autres participants),
/// puis envoie une SDP Offer initiale au client.
pub async fn add_participant(
    sfu_state: &SfuState,
    turn: &TurnConfig,
    room_id: &str,
    user_id: Uuid,
    ws_tx: mpsc::UnboundedSender<String>,
    existing_relays: Vec<Arc<TrackLocalStaticRTP>>,
) -> AppResult<()> {
    let pc = create_peer_connection(turn).await?;

    // Transceiver recvonly audio : reçoit le flux micro du client.
    pc.add_transceiver_from_kind(
        RTPCodecType::Audio,
        Some(RTCRtpTransceiverInit {
            direction: RTCRtpTransceiverDirection::Recvonly,
            send_encodings: vec![],
        }),
    )
    .await
    .map_err(|e| AppError::Internal(format!("SFU transceiver audio: {e}")))?;

    // Transceiver recvonly vidéo : reçoit le flux caméra du client.
    pc.add_transceiver_from_kind(
        RTPCodecType::Video,
        Some(RTCRtpTransceiverInit {
            direction: RTCRtpTransceiverDirection::Recvonly,
            send_encodings: vec![],
        }),
    )
    .await
    .map_err(|e| AppError::Internal(format!("SFU transceiver vidéo: {e}")))?;

    // Ajouter les relay tracks des autres participants déjà connectés.
    // Le client recevra immédiatement ces flux dans son on_track dès que le PC est stable.
    for relay in &existing_relays {
        pc.add_track(relay.clone())
            .await
            .map_err(|e| AppError::Internal(format!("SFU add_track: {e}")))?;
    }

    // Setup on_ice_candidate : transférer les ICE candidates SFU → client via WebSocket.
    {
        let ws_ice = ws_tx.clone();
        let rid_ice = room_id.to_string();
        pc.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            let tx = ws_ice.clone();
            let rid = rid_ice.clone();
            Box::pin(async move {
                let Some(c) = candidate else { return };
                let Ok(json) = c.to_json() else { return };
                let msg = serde_json::json!({
                    "type": "sfu_ice_candidate",
                    "room_id": rid,
                    "candidate": json.candidate,
                    "sdp_mid": json.sdp_mid,
                    "sdp_m_line_index": json.sdp_mline_index,
                })
                .to_string();
                tx.send(msg).ok();
            })
        }));
    }

    // Setup on_track : relay des paquets RTP entrants vers les autres participants.
    setup_on_track(pc.clone(), user_id, room_id.to_string(), sfu_state.clone());

    // Enregistrer dans l'état SFU.
    {
        let mut state = sfu_state.lock().await;
        let room = state.rooms.entry(room_id.to_string()).or_insert_with(SfuRoom::new);
        room.participants.insert(
            user_id,
            SfuParticipant {
                peer_connection: pc.clone(),
                ws_tx: ws_tx.clone(),
            },
        );
    }

    // Envoyer l'Offer initiale (déclenche la négociation côté client).
    send_offer(&pc, room_id, &ws_tx).await;

    tracing::info!("[sfu] participant ajouté — user={user_id} room={room_id}");
    Ok(())
}

/// Retire un participant d'une room SFU et ferme sa peer connection.
pub async fn remove_participant(sfu_state: &SfuState, room_id: &str, user_id: Uuid) {
    let pc = {
        let mut state = sfu_state.lock().await;
        if let Some(room) = state.rooms.get_mut(room_id) {
            room.relay_tracks.remove(&user_id);
            let pc = room.participants.remove(&user_id).map(|p| p.peer_connection);
            // Nettoyer la room si elle est vide.
            if room.participants.is_empty() {
                state.rooms.remove(room_id);
            }
            pc
        } else {
            None
        }
    };

    if let Some(pc) = pc {
        let _ = pc.close().await;
        tracing::info!("[sfu] participant retiré — user={user_id} room={room_id}");
    }
}

/// Traite une SDP Answer envoyée par le client (client → serveur).
///
/// Finalise la négociation en appelant set_remote_description sur la peer connection SFU.
pub async fn handle_answer(
    sfu_state: &SfuState,
    room_id: &str,
    user_id: Uuid,
    sdp: &str,
) -> AppResult<()> {
    let pc = get_peer_connection(sfu_state, room_id, user_id).await?;
    let answer = RTCSessionDescription::answer(sdp.to_string())
        .map_err(|e| AppError::BadRequest(format!("SDP SFU answer invalide: {e}")))?;
    pc.set_remote_description(answer)
        .await
        .map_err(|e| AppError::Internal(format!("SFU set_remote_description: {e}")))?;
    Ok(())
}

/// Traite un ICE candidate envoyé par le client (client → serveur).
pub async fn handle_ice_candidate(
    sfu_state: &SfuState,
    room_id: &str,
    user_id: Uuid,
    candidate: &str,
    sdp_mid: Option<String>,
    sdp_mline_index: Option<u16>,
) -> AppResult<()> {
    let pc = match get_peer_connection(sfu_state, room_id, user_id).await {
        Ok(p) => p,
        Err(_) => return Ok(()), // Participant pas encore enregistré, ignorer.
    };

    use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
    pc.add_ice_candidate(RTCIceCandidateInit {
        candidate: candidate.to_string(),
        sdp_mid,
        sdp_mline_index,
        username_fragment: None,
    })
    .await
    .map_err(|e| AppError::Internal(format!("SFU add_ice_candidate: {e}")))?;

    Ok(())
}

/// Retourne toutes les relay tracks publiées dans une room, sauf celles de `exclude_user`.
///
/// Utilisé pour initialiser un nouveau participant avec les flux déjà présents.
pub async fn get_existing_relays(
    sfu_state: &SfuState,
    room_id: &str,
    exclude_user: Uuid,
) -> Vec<Arc<TrackLocalStaticRTP>> {
    let state = sfu_state.lock().await;
    state
        .rooms
        .get(room_id)
        .map(|room| {
            room.relay_tracks
                .iter()
                .filter(|(id, _)| **id != exclude_user)
                .flat_map(|(_, tracks)| tracks.iter().cloned())
                .collect()
        })
        .unwrap_or_default()
}

// ─── Helpers privés ───────────────────────────────────────────────────────────

/// Crée une RTCPeerConnection côté serveur avec les codecs et interceptors par défaut.
async fn create_peer_connection(turn: &TurnConfig) -> AppResult<Arc<RTCPeerConnection>> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .map_err(|e| AppError::Internal(format!("SFU codecs: {e}")))?;

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)
        .map_err(|e| AppError::Internal(format!("SFU interceptors: {e}")))?;

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: turn.stun_urls.clone(),
            ..Default::default()
        }],
        ..Default::default()
    };

    api.new_peer_connection(config)
        .await
        .map(Arc::new)
        .map_err(|e| AppError::Internal(format!("SFU new_peer_connection: {e}")))
}

/// Récupère la peer connection SFU d'un participant, ou retourne une erreur.
async fn get_peer_connection(
    sfu_state: &SfuState,
    room_id: &str,
    user_id: Uuid,
) -> AppResult<Arc<RTCPeerConnection>> {
    let state = sfu_state.lock().await;
    state
        .rooms
        .get(room_id)
        .and_then(|r| r.participants.get(&user_id))
        .map(|p| p.peer_connection.clone())
        .ok_or_else(|| AppError::NotFound("SFU participant introuvable".to_string()))
}

/// Configure le callback on_track pour relayer les paquets RTP vers les autres participants.
///
/// Quand un paquet RTP arrive du client :
///   1. Une TrackLocalStaticRTP est créée (pipe RTP opaque).
///   2. Cette relay track est ajoutée aux peer connections de tous les autres participants.
///   3. Une nouvelle Offer est envoyée à chaque participant mis à jour (renegotiation).
///   4. La boucle de forwarding copie chaque paquet reçu dans la relay track.
fn setup_on_track(
    pc: Arc<RTCPeerConnection>,
    publisher_id: Uuid,
    room_id: String,
    sfu_state: SfuState,
) {
    pc.on_track(Box::new(move |remote_track, _, _| {
        let room_id = room_id.clone();
        let sfu_state = sfu_state.clone();

        Box::pin(async move {
            // Créer une relay track opaque (même codec, stream_id = publisher_id pour identification).
            let codec_cap = remote_track.codec().capability.clone();
            let relay = Arc::new(TrackLocalStaticRTP::new(
                codec_cap,
                remote_track.id().to_string(),
                // Utiliser le user_id comme stream_id pour que le frontend puisse
                // identifier de quel participant vient ce flux.
                publisher_id.to_string(),
            ));

            // ── Phase 1 : stockage de la relay track + collecte des abonnés ──
            // Lock court : on lit et écrit l'état en une seule section critique.
            let subscribers: Vec<(Arc<RTCPeerConnection>, mpsc::UnboundedSender<String>)> = {
                let mut state = sfu_state.lock().await;
                if let Some(room) = state.rooms.get_mut(&room_id) {
                    room.relay_tracks
                        .entry(publisher_id)
                        .or_default()
                        .push(relay.clone());
                    room.participants
                        .iter()
                        .filter(|(id, _)| **id != publisher_id)
                        .map(|(_, p)| (p.peer_connection.clone(), p.ws_tx.clone()))
                        .collect()
                } else {
                    vec![]
                }
            }; // ← lock libéré ici, AVANT le work async

            // ── Phase 2 : ajouter la relay track aux abonnés + renegotiation ──
            // Chaque renegotiation est dans sa propre tâche pour éviter les blocages.
            for (sub_pc, sub_tx) in subscribers {
                let relay_clone = relay.clone();
                let room_id_clone = room_id.clone();
                tokio::spawn(async move {
                    if sub_pc.add_track(relay_clone).await.is_ok() {
                        send_offer(&sub_pc, &room_id_clone, &sub_tx).await;
                    }
                });
            }

            // ── Phase 3 : boucle de forwarding RTP ──
            // Aucun lock détenu. Copie brute des paquets RTP (pas de décodage).
            loop {
                match remote_track.read_rtp().await {
                    Ok((packet, _)) => {
                        if relay.write_rtp(&packet).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            tracing::debug!(
                "[sfu] relay terminée — publisher={publisher_id} room={room_id}"
            );
        })
    }));
}

/// Crée une SDP Offer côté serveur et l'envoie au client via WebSocket.
async fn send_offer(
    pc: &RTCPeerConnection,
    room_id: &str,
    ws_tx: &mpsc::UnboundedSender<String>,
) {
    match pc.create_offer(None).await {
        Ok(offer) => {
            if pc.set_local_description(offer.clone()).await.is_ok() {
                let msg = serde_json::json!({
                    "type": "sfu_offer",
                    "room_id": room_id,
                    "sdp": offer.sdp,
                })
                .to_string();
                ws_tx.send(msg).ok();
            }
        }
        Err(e) => tracing::warn!("[sfu] create_offer échoué: {e}"),
    }
}
