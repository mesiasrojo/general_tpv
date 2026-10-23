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
    let require_auth = std::env::var("TPV_REQUIRE_BACKOFFICE_AUTH")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !require_auth && token.is_none() {
        tracing::warn!(
            "central API abierta en modo local; configura TPV_CENTRAL_TOKEN y TPV_REQUIRE_BACKOFFICE_AUTH=1 en producción"
        );
    }
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
    let state = AppState::with_auth(token, store, require_auth);
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "central TPV server listening");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
