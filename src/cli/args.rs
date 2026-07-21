use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "pdiff",
    version,
    about = "Review-first terminal diff viewer",
    disable_version_flag = true,
    after_help = "Common review options:\n  --mode <MODE>                           layout mode: auto, split, stack\n  --watch                                 auto-reload when the current diff input changes\n  --theme <THEME>                         named theme override\n  --agent-context <PATH>                  JSON sidecar with agent rationale\n  --pager                                 use pager-style chrome and controls\n  --line-numbers / --no-line-numbers      show or hide line numbers\n  --wrap / --no-wrap                      wrap or truncate long diff lines\n  --hunk-headers / --no-hunk-headers      show or hide hunk metadata rows\n  --agent-notes / --no-agent-notes        show or hide agent notes by default\n  --transparent-bg / --no-transparent-bg  use or paint the terminal background\n  --exclude-untracked                     hide untracked working-tree files\n\nRun `pdiff <command> --help` for command-specific syntax."
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
    /// Render or inspect native terminal markup without opening the review UI.
    Markup {
        #[command(subcommand)]
        command: MarkupCommand,
    },
    /// Inspect and control a live pdiff review.
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    /// Run the native live-session broker.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Compatibility alias for the native live-session broker.
    Mcp {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Inspect native pdiff agent assets.
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Serve the loopback live-session API.
    Serve,
}

#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// Materialize and print the embedded pdiff review skill path.
    Path,
}

#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// List live reviews.
    List(SessionListArgs),
    /// Get one live review snapshot.
    Get(SessionSelectedArgs),
    /// Get focused context for one live review.
    Context(SessionSelectedArgs),
    /// Export the structured review for one live review.
    Review(SessionReviewArgs),
    /// Move the selected location in one live review.
    Navigate(SessionNavigateArgs),
    /// Replace the input of one live review.
    Reload(SessionReloadArgs),
    /// Manage live review comments.
    Comment {
        #[command(subcommand)]
        command: SessionCommentCommand,
    },
}

#[derive(Debug, Args)]
pub struct SessionListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct SessionSelectorArgs {
    #[arg(value_name = "SESSION_ID")]
    pub session_id: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub repo: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct SessionSelectedArgs {
    #[command(flatten)]
    pub selector: SessionSelectorArgs,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SessionReviewArgs {
    #[command(flatten)]
    pub selector: SessionSelectorArgs,
    #[arg(long)]
    pub include_patch: bool,
    #[arg(long)]
    pub include_notes: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SessionNavigateArgs {
    #[command(flatten)]
    pub selector: SessionSelectorArgs,
    #[arg(long)]
    pub file: Option<String>,
    #[arg(long)]
    pub hunk: Option<usize>,
    #[arg(long)]
    pub old_line: Option<u32>,
    #[arg(long)]
    pub new_line: Option<u32>,
    #[arg(long)]
    pub next_comment: bool,
    #[arg(long)]
    pub prev_comment: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SessionReloadArgs {
    #[arg(value_name = "SESSION_ID")]
    pub session_id: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub repo: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub session_path: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub source: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
    #[arg(last = true, required = true, value_name = "REVIEW_COMMAND")]
    pub review_command: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum SessionCommentCommand {
    /// Add one live agent comment.
    Add(SessionCommentAddArgs),
    /// Apply a JSON comment batch from standard input.
    Apply(SessionCommentApplyArgs),
    /// List comments.
    List(SessionCommentListArgs),
    /// Remove one live comment by id.
    Rm(SessionCommentRemoveArgs),
    /// Clear live comments after explicit confirmation.
    Clear(SessionCommentClearArgs),
}

#[derive(Debug, Args)]
pub struct SessionCommentAddArgs {
    #[command(flatten)]
    pub selector: SessionSelectorArgs,
    #[arg(long)]
    pub file: String,
    #[arg(long)]
    pub old_line: Option<u32>,
    #[arg(long)]
    pub new_line: Option<u32>,
    #[arg(long)]
    pub summary: String,
    #[arg(long)]
    pub rationale: Option<String>,
    #[arg(long)]
    pub markup: Option<String>,
    #[arg(long)]
    pub author: Option<String>,
    #[arg(long)]
    pub focus: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SessionCommentApplyArgs {
    #[command(flatten)]
    pub selector: SessionSelectorArgs,
    #[arg(long)]
    pub stdin: bool,
    #[arg(long)]
    pub focus: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SessionCommentTypeArg {
    Live,
    All,
    Ai,
    Agent,
    User,
}

#[derive(Debug, Args)]
pub struct SessionCommentListArgs {
    #[command(flatten)]
    pub selector: SessionSelectorArgs,
    #[arg(long)]
    pub file: Option<String>,
    #[arg(long = "type", value_enum)]
    pub note_type: Option<SessionCommentTypeArg>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SessionCommentRemoveArgs {
    #[arg(value_name = "TARGET")]
    pub targets: Vec<String>,
    #[arg(long, value_name = "PATH")]
    pub repo: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SessionCommentClearArgs {
    #[command(flatten)]
    pub selector: SessionSelectorArgs,
    #[arg(long)]
    pub file: Option<String>,
    #[arg(long)]
    pub include_user: bool,
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub yes: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Subcommand)]
pub enum MarkupCommand {
    /// Render an STML file, or '-' for standard input.
    Render(MarkupRenderArgs),
    /// Print the embedded STML authoring guide.
    Guide,
}

#[derive(Debug, Args)]
pub struct MarkupRenderArgs {
    #[arg(value_name = "FILE", default_value = "-")]
    pub file: PathBuf,
    #[arg(long, default_value_t = crate::markup::STML_REFERENCE_WIDTH)]
    pub width: u16,
    #[arg(long)]
    pub theme: Option<String>,
    #[arg(long, value_enum, default_value_t = MarkupColorArg::Auto)]
    pub color: MarkupColorArg,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MarkupColorArg {
    Auto,
    Always,
    Never,
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
