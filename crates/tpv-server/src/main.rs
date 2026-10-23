use std::net::SocketAddr;

use tpv_server::{AppState, app};

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
    let state = AppState::new(token);
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "central TPV server listening");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
