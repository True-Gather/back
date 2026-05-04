// Module WebRTC engine.
//
// Ce module encapsule webrtc-rs pour fournir :
//   - la configuration ICE/STUN commune à tous les peers,
//   - des credentials TURN temporels (RFC 5766 / coturn --use-auth-secret),
//   - une factory de RTCPeerConnection prête à l'emploi,
//   - des helpers de validation SDP et ICE candidate.

/// SFU (Selective Forwarding Unit) pour les rooms de groupe (3+ participants).
pub mod sfu;
//
// ─── Sécurité TURN ───────────────────────────────────────────────────────────
//
// Les credentials TURN ne sont JAMAIS des mots de passe permanents.
// On utilise le mécanisme de credentials temporels (coturn --use-auth-secret) :
//
//   username  = "<timestamp_expiry>"
//   password  = base64(HMAC-SHA1(secret, username))
//
// Ces credentials sont valides uniquement pendant `turn.ttl_secs` secondes.
// Même interceptés, ils ne servent à rien après expiration.
// Le secret `turn.secret` ne quitte jamais le backend.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors, media_engine::MediaEngine, APIBuilder,
    },
    ice_transport::ice_server::RTCIceServer,
    interceptor::registry::Registry,
    peer_connection::{configuration::RTCConfiguration, RTCPeerConnection},
    sdp::SessionDescription,
};

use crate::{
    config::TurnConfig,
    error::{AppError, AppResult},
};

// ─── Représentation d'un serveur ICE pour le frontend ────────────────────────

// Structure sérialisable envoyée dans le message `joined`.
// Compatible avec le champ `iceServers` de RTCConfiguration côté navigateur.
#[derive(Debug, serde::Serialize, Clone)]
pub struct IceServerInfo {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

// ─── Configuration ICE ────────────────────────────────────────────────────────

// Construit les serveurs STUN depuis la config (aucune URL en dur dans le code).
fn stun_servers(stun_urls: &[String]) -> Vec<IceServerInfo> {
    vec![IceServerInfo {
        urls: stun_urls.to_vec(),
        username: None,
        credential: None,
    }]
}

// Génère des credentials TURN temporels selon RFC 5766.
//
// `username`  = timestamp d'expiration (UNIX)
// `password`  = base64(HMAC-SHA256(secret, username))
//
// Coturn vérifie ces credentials avec --use-auth-secret.
// Ils expirent automatiquement après `ttl_secs` secondes.
fn generate_turn_credentials(secret: &str, ttl_secs: u64) -> AppResult<(String, String)> {
    let expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::Internal(format!("SystemTime error: {e}")))?
        .as_secs()
        + ttl_secs;

    let username = expiry.to_string();

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| AppError::Internal(format!("HMAC init error: {e}")))?;
    mac.update(username.as_bytes());
    let credential = B64.encode(mac.finalize().into_bytes());

    Ok((username, credential))
}

// Retourne la liste des serveurs ICE à envoyer au client lors du `joined`.
//
// - Toujours : STUN depuis la config (APP_TURN__STUN_URLS).
// - Si configuré : TURN avec credentials temporels.
pub fn build_ice_servers(turn: &TurnConfig) -> AppResult<Vec<IceServerInfo>> {
    let mut servers = stun_servers(&turn.stun_urls);

    if let (Some(url), Some(secret)) = (&turn.url, &turn.secret) {
        let (username, credential) = generate_turn_credentials(secret, turn.ttl_secs)?;
        servers.push(IceServerInfo {
            urls: vec![url.clone()],
            username: Some(username),
            credential: Some(credential),
        });
    }

    Ok(servers)
}

// Construit la RTCConfiguration interne webrtc-rs à partir de la config.
pub fn default_rtc_config(turn: &TurnConfig) -> RTCConfiguration {
    RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: turn.stun_urls.clone(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

// ─── Factory RTCPeerConnection ────────────────────────────────────────────────

// Crée un nouveau RTCPeerConnection côté serveur.
//
// Enregistre les codecs audio/vidéo par défaut (Opus, VP8, VP9, H264)
// et les interceptors RTP standard (NACK, RTCP reports, etc.).
pub async fn new_peer_connection(turn: &TurnConfig) -> AppResult<RTCPeerConnection> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .map_err(|e| AppError::Internal(format!("WebRTC codecs: {e}")))?;

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)
        .map_err(|e| AppError::Internal(format!("WebRTC interceptors: {e}")))?;

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    api.new_peer_connection(default_rtc_config(turn))
        .await
        .map_err(|e| AppError::Internal(format!("RTCPeerConnection: {e}")))
}

// ─── Validation SDP ───────────────────────────────────────────────────────────

// Valide une chaîne SDP.
//
// Retourne une erreur si le SDP est mal formé.
pub fn validate_sdp(sdp: &str) -> AppResult<()> {
    use std::io::Cursor;

    let mut reader = Cursor::new(sdp.as_bytes());
    SessionDescription::unmarshal(&mut reader)
        .map(|_| ())
        .map_err(|e| AppError::BadRequest(format!("SDP invalide : {e}")))
}

// ─── Validation ICE candidate ─────────────────────────────────────────────────

// Valide qu'un candidat ICE n'est pas vide.
pub fn validate_ice_candidate(candidate: &str) -> AppResult<()> {
    if candidate.trim().is_empty() {
        return Err(AppError::BadRequest("ICE candidate vide".to_string()));
    }
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── SDP valide ────────────────────────────────────────────────────────────

    // SDP Offer minimal conforme RFC 4566.
    const VALID_SDP: &str = "\
v=0\r\n\
o=- 0 0 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
a=group:BUNDLE 0\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
c=IN IP4 0.0.0.0\r\n\
a=rtpmap:111 opus/48000/2\r\n\
a=mid:0\r\n\
";

    #[test]
    fn sdp_valide_accepte() {
        assert!(validate_sdp(VALID_SDP).is_ok());
    }

    #[test]
    fn sdp_vide_refuse() {
        assert!(validate_sdp("").is_err());
    }

    #[test]
    fn sdp_corrompu_refuse() {
        // Champ obligatoire `o=` absent → parser doit rejeter.
        let bad = "v=0\r\ns=-\r\nt=0 0\r\n";
        assert!(validate_sdp(bad).is_err());
    }

    // ── ICE candidate ─────────────────────────────────────────────────────────

    #[test]
    fn ice_candidate_valide_accepte() {
        let c = "candidate:1 1 UDP 2130706431 192.168.1.1 54321 typ host";
        assert!(validate_ice_candidate(c).is_ok());
    }

    #[test]
    fn ice_candidate_vide_refuse() {
        assert!(validate_ice_candidate("").is_err());
        assert!(validate_ice_candidate("   ").is_err());
    }

    // ── TURN credentials ──────────────────────────────────────────────────────

    #[test]
    fn turn_credentials_format() {
        let (username, credential) = generate_turn_credentials("secret_test", 3600).unwrap();

        // username doit être un timestamp numérique.
        assert!(username.parse::<u64>().is_ok(), "username doit être un u64");

        // credential doit être du base64 valide.
        use base64::{Engine, engine::general_purpose::STANDARD as B64};
        assert!(B64.decode(&credential).is_ok(), "credential doit être du base64");
    }

    #[test]
    fn turn_credentials_different_a_chaque_appel() {
        // Les credentials doivent être différents si le TTL diffère.
        let (u1, c1) = generate_turn_credentials("secret", 3600).unwrap();
        let (u2, c2) = generate_turn_credentials("secret", 7200).unwrap();
        assert_ne!(u1, u2);
        assert_ne!(c1, c2);
    }

    #[test]
    fn turn_credentials_different_si_secret_different() {
        let (_, c1) = generate_turn_credentials("secret_a", 3600).unwrap();
        let (_, c2) = generate_turn_credentials("secret_b", 3600).unwrap();
        // Même TTL mais secrets différents → credentials différents.
        assert_ne!(c1, c2);
    }

    // ── build_ice_servers ────────────────────────────────────────────────────

    #[test]
    fn build_ice_servers_sans_turn() {
        let turn = TurnConfig {
            stun_urls: vec!["stun:stun.exemple.com:3478".to_owned()],
            url: None,
            secret: None,
            ttl_secs: 3600,
        };
        let servers = build_ice_servers(&turn).unwrap();
        assert_eq!(servers.len(), 1);
        assert!(servers[0].username.is_none());
        assert!(servers[0].credential.is_none());
    }

    #[test]
    fn build_ice_servers_avec_turn() {
        let turn = TurnConfig {
            stun_urls: vec!["stun:stun.exemple.com:3478".to_owned()],
            url: Some("turn:turn.exemple.com:3478".to_owned()),
            secret: Some("mon_secret".to_owned()),
            ttl_secs: 3600,
        };
        let servers = build_ice_servers(&turn).unwrap();
        // STUN + TURN = 2 entrées.
        assert_eq!(servers.len(), 2);
        // Le serveur TURN a des credentials.
        let turn_server = &servers[1];
        assert!(turn_server.username.is_some());
        assert!(turn_server.credential.is_some());
        // Le secret ne doit PAS apparaître en clair dans les credentials.
        assert_ne!(turn_server.credential.as_deref().unwrap(), "mon_secret");
    }
}

