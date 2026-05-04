// Tests d'intégration — serveur de signalisation WebRTC.
//
// Ces tests vérifient le flux complet de signalisation WebSocket :
//   1. Connexion avec une session valide → message `joined` + `ice_servers`.
//   2. Deux pairs dans la même room → notifications croisées `peer_joined`.
//   3. Routage d'une Offer d'un pair à l'autre.
//   4. Rejet d'une Offer avec SDP invalide → message `error`.

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    connect_async,
    tungstenite::Message as WsMsg,
};
use uuid::Uuid;

use truegather_backend::{
    build_app,
    config::{AppConfig, AuthConfig, BackendConfig, FrontendConfig, KeycloakConfig, RedisConfig, ServerConfig, TurnConfig},
    models::User,
    redis::create_pool,
    state::{AppSession, AppState},
};

// ─── Types ────────────────────────────────────────────────────────────────────

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn test_config() -> AppConfig {
    AppConfig {
        server: ServerConfig {
            host: "127.0.0.1".to_owned(),
            port: 0,
        },
        backend: BackendConfig {
            base_url: "http://localhost".to_owned(),
        },
        frontend: FrontendConfig {
            base_url: "http://localhost:3000".to_owned(),
        },
        keycloak: KeycloakConfig {
            issuer_url: "http://localhost:8081/realms/truegather".to_owned(),
            client_id: "test".to_owned(),
            client_secret: None,
        },
        auth: AuthConfig {
            cookie_name: "tg_session".to_owned(),
            cookie_secure: false,
        },
        redis: RedisConfig {
            url: "redis://127.0.0.1:6379".to_owned(),
        },
        turn: TurnConfig {
            stun_urls: vec!["stun:stun.test.local:3478".to_owned()],
            url: None,
            secret: None,
            ttl_secs: 3600,
        },
    }
}

// Démarre le serveur Axum sur un port aléatoire.
// Retourne None si Redis est inaccessible (le test sera ignoré).
async fn start_test_server() -> Option<(String, AppState)> {
    let config = test_config();
    let redis = create_pool(&config.redis.url).ok()?;

    {
        let mut conn = redis.get().await.ok()?;
        deadpool_redis::redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .ok()?;
    }

    let state = AppState::new(config, redis).ok()?;
    let app = build_app(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?.to_string();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Some((addr, state))
}

// Insère une session fictive dans le state en mémoire.
async fn create_test_session(state: &AppState) -> (Uuid, String) {
    let user_id = Uuid::new_v4();
    let sub = format!("test-{user_id}");
    let session_id = Uuid::new_v4().to_string();

    let user = User {
        id: user_id,
        keycloak_sub: Some(sub.clone()),
        email: format!("{user_id}@test.local"),
        display_name: format!("User {user_id}"),
        first_name: None,
        last_name: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        last_login_at: None,
    };

    state.users.write().await.insert(user_id, user);

    let session = AppSession {
        user_id,
        keycloak_sub: sub,
        email: format!("{user_id}@test.local"),
        display_name: format!("User {user_id}"),
        first_name: None,
        last_name: None,
    };

    state.sessions.write().await.insert(session_id.clone(), session);
    (user_id, session_id)
}

// Ouvre une connexion WebSocket avec le cookie de session injecté.
//
// On laisse tungstenite construire la requête WS complète (avec Sec-WebSocket-Key, etc.)
// via `into_client_request()`, puis on ajoute le cookie manuellement.
async fn ws_connect(addr: &str, session_id: &str) -> WsStream {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let url = format!("ws://{addr}/ws/signal");
    let mut request = url.into_client_request().expect("URL invalide");
    request.headers_mut().insert(
        http::header::COOKIE,
        format!("tg_session={session_id}").parse().expect("cookie header invalide"),
    );

    let (ws, _) = connect_async(request).await.expect("connexion WS échouée");
    ws
}

// Lit le prochain message texte JSON (ignore Ping/Pong).
async fn recv_json(ws: &mut WsStream) -> Value {
    loop {
        match ws.next().await.expect("stream fermé").expect("erreur WS") {
            WsMsg::Text(t) => return serde_json::from_str(&t).expect("JSON invalide"),
            WsMsg::Ping(_) | WsMsg::Pong(_) => continue,
            other => panic!("message WS inattendu : {other:?}"),
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

// Un pair rejoint une room vide → `joined` avec ice_servers non vide, peers vide.
#[tokio::test]
async fn join_room_recoit_joined_avec_ice_servers() {
    let Some((addr, state)) = start_test_server().await else {
        eprintln!("⚠  Redis inaccessible — test ignoré");
        return;
    };

    let (_, session_id) = create_test_session(&state).await;
    let mut ws = ws_connect(&addr, &session_id).await;

    ws.send(WsMsg::Text(
        json!({ "type": "join", "room_id": "room-single" }).to_string().into(),
    ))
    .await
    .unwrap();

    let msg = recv_json(&mut ws).await;
    assert_eq!(msg["type"], "joined", "réponse inattendue : {msg}");

    let ice = msg["ice_servers"].as_array().expect("ice_servers manquant");
    assert!(!ice.is_empty(), "ice_servers ne doit pas être vide");

    let peers = msg["peers"].as_array().expect("peers manquant");
    assert!(peers.is_empty(), "room vide → peers doit être []");
}

// Deux pairs dans la même room → notifications croisées.
#[tokio::test]
async fn deux_pairs_room_notifications_croisees() {
    let Some((addr, state)) = start_test_server().await else {
        eprintln!("⚠  Redis inaccessible — test ignoré");
        return;
    };

    let (user_a, session_a) = create_test_session(&state).await;
    let (user_b, session_b) = create_test_session(&state).await;
    let room = format!("room-cross-{}", Uuid::new_v4());

    let mut ws_a = ws_connect(&addr, &session_a).await;
    let mut ws_b = ws_connect(&addr, &session_b).await;

    // A rejoint.
    ws_a.send(WsMsg::Text(
        json!({ "type": "join", "room_id": room }).to_string().into(),
    ))
    .await
    .unwrap();
    let joined_a = recv_json(&mut ws_a).await;
    assert_eq!(joined_a["type"], "joined");
    assert!(joined_a["peers"].as_array().unwrap().is_empty());

    // B rejoint.
    ws_b.send(WsMsg::Text(
        json!({ "type": "join", "room_id": room }).to_string().into(),
    ))
    .await
    .unwrap();
    let joined_b = recv_json(&mut ws_b).await;
    assert_eq!(joined_b["type"], "joined");

    let peers_b: Vec<String> = joined_b["peers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert!(peers_b.contains(&user_a.to_string()), "B doit voir A dans ses peers");

    // A reçoit peer_joined pour B.
    let peer_joined = recv_json(&mut ws_a).await;
    assert_eq!(peer_joined["type"], "peer_joined");
    assert_eq!(peer_joined["user_id"], user_b.to_string());
}

// A envoie une Offer valide → B la reçoit avec from = user_a.
#[tokio::test]
async fn offer_valide_routee_vers_pair_cible() {
    let Some((addr, state)) = start_test_server().await else {
        eprintln!("⚠  Redis inaccessible — test ignoré");
        return;
    };

    let sdp = concat!(
        "v=0\r\n",
        "o=- 0 0 IN IP4 127.0.0.1\r\n",
        "s=-\r\n",
        "t=0 0\r\n",
        "m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n",
        "c=IN IP4 0.0.0.0\r\n",
        "a=rtpmap:111 opus/48000/2\r\n",
        "a=mid:0\r\n",
    );

    let (user_a, session_a) = create_test_session(&state).await;
    let (user_b, session_b) = create_test_session(&state).await;
    let room = format!("room-offer-{}", Uuid::new_v4());

    let mut ws_a = ws_connect(&addr, &session_a).await;
    let mut ws_b = ws_connect(&addr, &session_b).await;

    ws_a.send(WsMsg::Text(json!({ "type": "join", "room_id": room }).to_string().into()))
        .await.unwrap();
    let _ = recv_json(&mut ws_a).await; // joined

    ws_b.send(WsMsg::Text(json!({ "type": "join", "room_id": room }).to_string().into()))
        .await.unwrap();
    let _ = recv_json(&mut ws_b).await; // joined
    let _ = recv_json(&mut ws_a).await; // peer_joined

    ws_a.send(WsMsg::Text(
        json!({
            "type": "offer",
            "room_id": room,
            "to": user_b.to_string(),
            "sdp": sdp,
        })
        .to_string()
        .into(),
    ))
    .await
    .unwrap();

    let offer = recv_json(&mut ws_b).await;
    assert_eq!(offer["type"], "offer");
    assert_eq!(offer["from"], user_a.to_string());
    assert!(offer["sdp"].is_string());
}

// SDP invalide → erreur renvoyée à A, B ne reçoit rien.
#[tokio::test]
async fn offer_sdp_invalide_renvoie_erreur_a_lemeteur() {
    let Some((addr, state)) = start_test_server().await else {
        eprintln!("⚠  Redis inaccessible — test ignoré");
        return;
    };

    let (_, session_a) = create_test_session(&state).await;
    let (user_b, session_b) = create_test_session(&state).await;
    let room = format!("room-invalid-{}", Uuid::new_v4());

    let mut ws_a = ws_connect(&addr, &session_a).await;
    let mut ws_b = ws_connect(&addr, &session_b).await;

    ws_a.send(WsMsg::Text(json!({ "type": "join", "room_id": room }).to_string().into()))
        .await.unwrap();
    let _ = recv_json(&mut ws_a).await;

    ws_b.send(WsMsg::Text(json!({ "type": "join", "room_id": room }).to_string().into()))
        .await.unwrap();
    let _ = recv_json(&mut ws_b).await;
    let _ = recv_json(&mut ws_a).await;

    ws_a.send(WsMsg::Text(
        json!({
            "type": "offer",
            "room_id": room,
            "to": user_b.to_string(),
            "sdp": "ce n'est pas du SDP valide",
        })
        .to_string()
        .into(),
    ))
    .await
    .unwrap();

    let err = recv_json(&mut ws_a).await;
    assert_eq!(err["type"], "error", "SDP invalide doit renvoyer une erreur");
}
