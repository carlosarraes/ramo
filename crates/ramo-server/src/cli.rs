use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "ramo-server",
    version,
    about = "Private local Review Map service"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Serve,
    Setup {
        #[arg(long)]
        dry_run: bool,
    },
    Status,
    Pair,
    Cache {
        #[command(subcommand)]
        command: CacheCommand,
    },
    Benchmark {
        #[command(subcommand)]
        command: BenchmarkCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum CacheCommand {
    List,
    Clear,
}

#[derive(Debug, Subcommand)]
pub enum BenchmarkCommand {
    Init {
        #[arg(long)]
        repo_path: PathBuf,
        #[arg(long = "pr")]
        pull_requests: Vec<u64>,
        #[arg(long, conflicts_with = "pull_requests")]
        recent: Option<usize>,
        #[arg(long)]
        yes: bool,
    },
    Run {
        #[arg(long, default_value = ".ramo-benchmark/manifest.json")]
        manifest: PathBuf,
        #[arg(long)]
        yes: bool,
    },
}
