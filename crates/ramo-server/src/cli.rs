use clap::{Parser, Subcommand};

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
}

#[derive(Debug, Subcommand)]
pub enum CacheCommand {
    List,
    Clear,
}
