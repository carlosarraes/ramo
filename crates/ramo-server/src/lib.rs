pub mod analysis;
pub mod api;
pub mod cache;
pub mod cli;
pub mod config;
pub mod error;
pub mod github;
pub mod ollama;

pub use error::ReviewMapFailure;

/// Parses and runs the standalone server command.
pub async fn run() -> Result<(), ReviewMapFailure> {
    use clap::Parser;
    use ramo_core::review_map::{REVIEW_MAP_SCHEMA_VERSION, ReviewMapFailureCode};

    match cli::Cli::parse().command {
        cli::Command::Serve => serve(config::ServerConfig::discover()?).await,
        _ => Err(ReviewMapFailure::new(
            ReviewMapFailureCode::ServerIncompatible,
            format!(
                "This ramo-server {} command is not available yet (schema {})",
                env!("CARGO_PKG_VERSION"),
                REVIEW_MAP_SCHEMA_VERSION
            ),
        )),
    }
}

pub async fn serve(config: config::ServerConfig) -> Result<(), ReviewMapFailure> {
    use std::sync::Arc;
    use std::time::Duration;

    use analysis::{AnalysisCoordinator, CoordinatorConfig};
    use api::{HealthStatus, PairingState, ReviewMapClientTokenStore, ServerState, build_router};
    use cache::{CacheLimits, ReviewMapCache};
    use ramo_core::review_map::ReviewMapFailureCode;

    config.validate()?;
    let cache = ReviewMapCache::new(
        &config.cache_dir,
        CacheLimits {
            max_bytes: 256 * 1024 * 1024,
            max_age: Duration::from_secs(30 * 24 * 60 * 60),
        },
    )?;
    let tokens = ReviewMapClientTokenStore::open(config.state_dir.join("clients.json"))?;
    let coordinator = AnalysisCoordinator::new(
        Arc::new(github::GithubPullRequestProvider::new()),
        Arc::new(ollama::OllamaAnalyzer::new(
            &config.ollama_url,
            &config.model,
            Duration::from_secs(120),
        )),
        cache,
        CoordinatorConfig::default(),
    );
    let state = ServerState {
        coordinator,
        pairing: PairingState::new(tokens.clone()),
        tokens,
        health: HealthStatus::healthy(&config.model),
    };
    let listener = tokio::net::TcpListener::bind(config.bind_address)
        .await
        .map_err(|error| {
            ReviewMapFailure::with_source(
                ReviewMapFailureCode::ServerUnreachable,
                "Could not bind the local Review Map server",
                error,
            )
        })?;
    tracing::info!(bind = %config.bind_address, "local Review Map server started");
    axum::serve(listener, build_router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|error| {
            ReviewMapFailure::with_source(
                ReviewMapFailureCode::ServerUnreachable,
                "The local Review Map server stopped unexpectedly",
                error,
            )
        })
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
