pub mod analysis;
pub mod api;
pub mod benchmark;
pub mod cache;
pub mod cli;
pub mod config;
pub mod error;
pub mod github;
pub mod ollama;
pub mod setup;

pub use error::ReviewMapFailure;

/// Parses and runs the standalone server command.
pub async fn run() -> Result<(), ReviewMapFailure> {
    use clap::Parser;

    match cli::Cli::parse().command {
        cli::Command::Serve => serve(config::ServerConfig::discover()?).await,
        cli::Command::Setup { dry_run } => {
            let plan = setup::current_plan()?;
            println!(
                "{}",
                setup::apply_setup(&setup::SystemEnvironment, &plan, dry_run)?
            );
            Ok(())
        }
        cli::Command::Status => {
            println!("{}", setup::current_status()?);
            Ok(())
        }
        cli::Command::Pair => {
            println!("{}", setup::issue_pairing_code()?);
            Ok(())
        }
        cli::Command::Cache { command } => {
            let config = config::ServerConfig::discover()?;
            let cache = cache::ReviewMapCache::new(
                config.cache_dir,
                cache::CacheLimits {
                    max_bytes: u64::MAX,
                    max_age: std::time::Duration::MAX,
                },
            )?;
            match command {
                cli::CacheCommand::List => {
                    let count = cache.list()?.len();
                    let noun = if count == 1 { "entry" } else { "entries" };
                    println!("{count} cached Review Map {noun}");
                }
                cli::CacheCommand::Clear => {
                    let removed = cache.clear()?;
                    println!("Removed {removed} cached Review Map entries");
                }
            }
            Ok(())
        }
        cli::Command::Benchmark { command } => benchmark::run_command(command).await,
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
            Duration::from_secs(90),
        )),
        cache,
        CoordinatorConfig::default(),
    );
    let state = ServerState {
        coordinator,
        pairing: PairingState::open(tokens.clone(), config.state_dir.join("pairing.json")),
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
