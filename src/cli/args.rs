use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "pdiff",
    version,
    about = "Review-first terminal diff viewer",
    disable_version_flag = true
)]
pub struct Cli {
    #[arg(short = 'v', long = "version", action = ArgAction::Version)]
    pub version: Option<bool>,

    #[arg(short, long)]
    pub input: Option<PathBuf>,

    #[arg(short, long)]
    pub output: Option<PathBuf>,

    #[arg(long)]
    pub stdout: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Review working-tree changes, a revision range, or two files.
    Diff(DiffArgs),
    /// Review the latest commit or a given revision.
    Show(ShowArgs),
    /// Review a patch file or patch text from stdin.
    Patch(PatchArgs),
    /// Act as a general VCS pager with diff detection.
    Pager(PagerArgs),
    /// Review a file pair provided by a VCS difftool.
    Difftool(DifftoolArgs),
    /// Review a Git stash.
    Stash {
        #[command(subcommand)]
        command: StashCommand,
    },
    /// Install a pdiff integration.
    Install(IntegrationArgs),
    /// Uninstall a pdiff integration.
    Uninstall(IntegrationArgs),
}

#[derive(Debug, Subcommand)]
pub enum StashCommand {
    /// Review a stash entry.
    Show(StashShowArgs),
}

#[derive(Debug, Clone, Default, Args)]
pub struct ReviewFlags {
    #[arg(long, value_enum)]
    pub mode: Option<LayoutArg>,
    #[arg(long)]
    pub theme: Option<String>,
    #[arg(long)]
    pub agent_context: Option<PathBuf>,
    #[arg(long)]
    pub pager: bool,
    #[arg(long, overrides_with = "no_line_numbers")]
    pub line_numbers: bool,
    #[arg(long, overrides_with = "line_numbers")]
    pub no_line_numbers: bool,
    #[arg(long, overrides_with = "no_wrap")]
    pub wrap: bool,
    #[arg(long, overrides_with = "wrap")]
    pub no_wrap: bool,
    #[arg(long, overrides_with = "no_hunk_headers")]
    pub hunk_headers: bool,
    #[arg(long, overrides_with = "hunk_headers")]
    pub no_hunk_headers: bool,
    #[arg(long, overrides_with = "no_agent_notes")]
    pub agent_notes: bool,
    #[arg(long, overrides_with = "agent_notes")]
    pub no_agent_notes: bool,
    #[arg(long, overrides_with = "no_transparent_bg")]
    pub transparent_bg: bool,
    #[arg(long, overrides_with = "transparent_bg")]
    pub no_transparent_bg: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LayoutArg {
    Auto,
    Split,
    Stack,
}

#[derive(Debug, Args)]
pub struct DiffArgs {
    #[command(flatten)]
    pub review: ReviewFlags,
    #[arg(long)]
    pub watch: bool,
    #[arg(long)]
    pub staged: bool,
    #[arg(long)]
    pub cached: bool,
    #[arg(long, overrides_with = "no_exclude_untracked")]
    pub exclude_untracked: bool,
    #[arg(long, overrides_with = "exclude_untracked")]
    pub no_exclude_untracked: bool,
    #[arg(value_name = "TARGET")]
    pub targets: Vec<String>,
    #[arg(last = true, value_name = "PATHSPEC")]
    pub pathspecs: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ShowArgs {
    #[command(flatten)]
    pub review: ReviewFlags,
    #[arg(long)]
    pub watch: bool,
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
    #[arg(last = true, value_name = "PATHSPEC")]
    pub pathspecs: Vec<String>,
}

#[derive(Debug, Args)]
pub struct StashShowArgs {
    #[command(flatten)]
    pub review: ReviewFlags,
    #[arg(value_name = "REF")]
    pub reference: Option<String>,
}

#[derive(Debug, Args)]
pub struct PatchArgs {
    #[command(flatten)]
    pub review: ReviewFlags,
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct PagerArgs {
    #[command(flatten)]
    pub review: ReviewFlags,
}

#[derive(Debug, Args)]
pub struct DifftoolArgs {
    #[command(flatten)]
    pub review: ReviewFlags,
    #[arg(long)]
    pub watch: bool,
    pub left: PathBuf,
    pub right: PathBuf,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct IntegrationArgs {
    pub target: String,
}
