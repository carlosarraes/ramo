pub mod analysis;
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

    let _cli = cli::Cli::parse();
    Ok(())
}
