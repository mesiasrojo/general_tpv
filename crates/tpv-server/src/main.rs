use std::net::SocketAddr;
use std::sync::Arc;

use tpv_server::{AppState, CentralStore, MemoryStore, PostgresStore, app};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tpv_server=info,tower_http=info".into()),
        )
        .init();

    let address: SocketAddr = std::env::var("TPV_CENTRAL_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    let token = std::env::var("TPV_CENTRAL_TOKEN").ok();
    let store: Arc<dyn CentralStore> = match std::env::var("DATABASE_URL") {
        Ok(database_url) => {
            tracing::info!("using PostgreSQL central store");
            Arc::new(PostgresStore::connect(&database_url).await?)
        }
        Err(_) => {
            tracing::warn!("DATABASE_URL is not set; using in-memory central store");
            Arc::new(MemoryStore::new())
        }
    };
    let state = AppState::with_store(token, store);
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "central TPV server listening");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
