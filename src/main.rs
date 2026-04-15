// Point d'entrée binaire du backend.

use std::error::Error;

// Import des fonctions et types exposés par la librairie du projet.
use truegather_backend::{build_app, config::AppConfig, redis::create_pool, state::AppState};

// Macro principale Tokio pour exécuter l'application en asynchrone.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialisation du subscriber de logs.
    init_tracing();

    // Chargement de la configuration depuis l'environnement.
    let config = AppConfig::from_env()?;

    // Création du pool Redis.
    let redis = create_pool(&config.redis.url)?;

    // Création du pool PostgreSQL et exécution des migrations.
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database.url)
        .await?;
    sqlx::migrate!("./migrations").run(&db).await?;

    let state = AppState::new(config.clone(), redis, db)?;
    let app = build_app(state);
    let address = config.server_address();
    tracing::info!("Starting TrueGather backend on {}", address);
    let listener = tokio::net::TcpListener::bind(&address).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// Initialise le système de logs.
fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,truegather_backend=debug,tower_http=info"));

    fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .compact()
        .init();
}
