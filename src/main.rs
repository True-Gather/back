use std::error::Error;

use truegather_backend::{
    build_app,
    config::AppConfig,
    state::AppState,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_tracing();

    let config = AppConfig::from_env()?;
    let state = AppState::new(config.clone()).await
        .map_err(|e| -> Box<dyn Error> { e })?;

    sqlx::migrate!("./migrations")
        .run(&state.db)
        .await?;

    let app = build_app(state);
    let address = config.server_address();

    tracing::info!("Starting TrueGather backend on {}", address);

    let listener = tokio::net::TcpListener::bind(&address).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,truegather_backend=debug,tower_http=info"));

    fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .compact()
        .init();
}