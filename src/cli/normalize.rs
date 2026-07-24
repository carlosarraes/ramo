use std::path::{Path, PathBuf};

use super::args::{
    Cli, Command, DaemonCommand, DiffArgs, DifftoolArgs, LayoutArg, MarkupColorArg, MarkupCommand,
    PrArgs, ReviewFlags, SessionCommand as CliSessionCommand, SessionCommentCommand,
    SessionCommentTypeArg, SessionNavigateArgs, SessionReloadArgs, SessionSelectorArgs, ShowArgs,
    SkillCommand, StashCommand, StashShowArgs,
};
use super::{Action, CliError, Invocation};
use crate::core::input::{CommonOptions, LayoutMode, PatchSource, ReviewInput, ReviewOutput};
use crate::markup::{MarkupColor, MarkupRenderOptions};
use crate::session::{
    CommentDirection, CommentListType, CommentRevealMode, DiffSide, SessionCommand, SessionOutput,
    SessionSelector,
};

pub fn normalize(cli: Cli, stdin_is_terminal: bool) -> Result<Invocation, CliError> {
    let output = ReviewOutput {
        markdown_path: cli.output,
        stdout: cli.stdout,
    };

    if cli.input.is_some() && cli.command.is_some() {
        return Err(CliError::ConflictingInput);
    }

    let action = match (cli.command, cli.input) {
        (None, Some(path)) => Action::Review(ReviewInput::Patch {
            source: PatchSource::File(path),
            options: CommonOptions::default(),
        }),
        (None, None) if stdin_is_terminal => Action::Print(super::render_help()),
        (None, None) => Action::Review(ReviewInput::Patch {
            source: PatchSource::Stdin,
            options: CommonOptions::default(),
        }),
        (Some(Command::Diff(args)), None) => Action::Review(normalize_diff(args)?),
        (Some(Command::Pr(args)), None) => Action::Review(normalize_pr(args)),
        (Some(Command::Show(args)), None) => Action::Review(normalize_show(args)),
        (Some(Command::Patch(args)), None) => Action::Review(ReviewInput::Patch {
            source: match args.file {
                Some(path) if path != Path::new("-") => PatchSource::File(path),
                _ => PatchSource::Stdin,
            },
            options: common_options(args.review, false, None),
        }),
        (Some(Command::Pager(args)), None) => Action::Review(ReviewInput::Pager {
            options: common_options(args.review, false, None),
        }),
        (Some(Command::Difftool(args)), None) => Action::Review(normalize_difftool(args)),
        (Some(Command::Stash { command }), None) => match command {
            StashCommand::Show(args) => Action::Review(normalize_stash_show(args)),
        },
        (Some(Command::Install(args)), None) => integration_action(args.target, true)?,
        (Some(Command::Uninstall(args)), None) => integration_action(args.target, false)?,
        (Some(Command::Markup { command }), None) => match command {
            MarkupCommand::Render(args) => Action::MarkupRender(MarkupRenderOptions {
                file: args.file,
                width: args.width,
                theme: args.theme,
                color: match args.color {
                    MarkupColorArg::Auto => MarkupColor::Auto,
                    MarkupColorArg::Always => MarkupColor::Always,
                    MarkupColorArg::Never => MarkupColor::Never,
                },
                json: args.json,
            }),
            MarkupCommand::Guide => Action::MarkupGuide,
        },
        (Some(Command::Session { command }), None) => Action::Session(normalize_session(command)?),
        (Some(Command::Daemon { command } | Command::Mcp { command }), None) => match command {
            DaemonCommand::Serve => Action::DaemonServe,
        },
        (Some(Command::Skill { command }), None) => match command {
            SkillCommand::Path => Action::SkillPath,
        },
        (Some(_), Some(_)) => unreachable!("conflicting input returned above"),
    };

    Ok(Invocation { action, output })
}

fn normalize_pr(args: PrArgs) -> ReviewInput {
    let mut options = common_options(args.review, false, None);
    options.watch = Some(false);
    options.pager = Some(false);
    ReviewInput::PullRequest {
        number: args.number,
        options,
    }
}

fn normalize_session(command: CliSessionCommand) -> Result<SessionCommand, CliError> {
    Ok(match command {
        CliSessionCommand::List(args) => SessionCommand::List {
            output: session_output(args.json),
        },
        CliSessionCommand::Get(args) => SessionCommand::Get {
            selector: normalize_selector(args.selector)?,
            output: session_output(args.json),
        },
        CliSessionCommand::Context(args) => SessionCommand::Context {
            selector: normalize_selector(args.selector)?,
            output: session_output(args.json),
        },
        CliSessionCommand::Review(args) => SessionCommand::Review {
            selector: normalize_selector(args.selector)?,
            include_patch: args.include_patch,
            include_notes: args.include_notes,
            output: session_output(args.json),
        },
        CliSessionCommand::Navigate(args) => normalize_session_navigate(args)?,
        CliSessionCommand::Reload(args) => normalize_session_reload(args)?,
        CliSessionCommand::Comment { command } => match command {
            SessionCommentCommand::Add(args) => {
                let (side, line) = exclusive_line(args.old_line, args.new_line)?;
                SessionCommand::CommentAdd {
                    selector: normalize_selector(args.selector)?,
                    file_path: args.file,
                    side,
                    line,
                    summary: args.summary,
                    rationale: args.rationale,
                    markup: args.markup,
                    author: args.author,
                    reveal: args.focus,
                    output: session_output(args.json),
                }
            }
            SessionCommentCommand::Apply(args) => {
                if !args.stdin {
                    return Err(session_error(
                        "session comment apply reads a JSON batch only with --stdin",
                    ));
                }
                SessionCommand::CommentApply {
                    selector: normalize_selector(args.selector)?,
                    read_stdin: true,
                    reveal_mode: if args.focus {
                        CommentRevealMode::First
                    } else {
                        CommentRevealMode::None
                    },
                    output: session_output(args.json),
                }
            }
            SessionCommentCommand::List(args) => SessionCommand::CommentList {
                selector: normalize_selector(args.selector)?,
                file_path: args.file,
                note_type: args.note_type.map(|kind| match kind {
                    SessionCommentTypeArg::Live => CommentListType::Live,
                    SessionCommentTypeArg::All => CommentListType::All,
                    SessionCommentTypeArg::Ai => CommentListType::Ai,
                    SessionCommentTypeArg::Agent => CommentListType::Agent,
                    SessionCommentTypeArg::User => CommentListType::User,
                }),
                output: session_output(args.json),
            },
            SessionCommentCommand::Rm(args) => {
                let (selector, comment_id) = if let Some(repo) = args.repo {
                    let [comment_id] = args.targets.as_slice() else {
                        return Err(session_error(
                            "session comment rm with --repo requires exactly one comment id",
                        ));
                    };
                    (
                        normalize_selector(SessionSelectorArgs {
                            session_id: None,
                            repo: Some(repo),
                        })?,
                        comment_id.clone(),
                    )
                } else {
                    let [session_id, comment_id] = args.targets.as_slice() else {
                        return Err(session_error(
                            "session comment rm requires a session id and comment id",
                        ));
                    };
                    (
                        normalize_selector(SessionSelectorArgs {
                            session_id: Some(session_id.clone()),
                            repo: None,
                        })?,
                        comment_id.clone(),
                    )
                };
                SessionCommand::CommentRemove {
                    selector,
                    comment_id,
                    output: session_output(args.json),
                }
            }
            SessionCommentCommand::Clear(args) => {
                if !args.yes {
                    return Err(session_error(
                        "session comment clear is destructive and requires --yes",
                    ));
                }
                SessionCommand::CommentClear {
                    selector: normalize_selector(args.selector)?,
                    file_path: args.file,
                    include_user: args.include_user || args.all,
                    output: session_output(args.json),
                }
            }
        },
    })
}

fn normalize_session_navigate(args: SessionNavigateArgs) -> Result<SessionCommand, CliError> {
    let selector = normalize_selector(args.selector)?;
    let output = session_output(args.json);
    match (args.next_comment, args.prev_comment) {
        (true, true) => Err(session_error(
            "session navigate accepts only one of --next-comment or --prev-comment",
        )),
        (true, false) | (false, true) => {
            if args.file.is_some()
                || args.hunk.is_some()
                || args.old_line.is_some()
                || args.new_line.is_some()
            {
                return Err(session_error(
                    "comment navigation cannot be combined with a file, hunk, or line target",
                ));
            }
            Ok(SessionCommand::Navigate {
                selector,
                file_path: None,
                hunk_number: None,
                side: None,
                line: None,
                comment_direction: Some(if args.next_comment {
                    CommentDirection::Next
                } else {
                    CommentDirection::Prev
                }),
                output,
            })
        }
        (false, false) => {
            let Some(file_path) = args.file else {
                return Err(session_error(
                    "session navigate requires --file for a hunk or line target",
                ));
            };
            let target_count = usize::from(args.hunk.is_some())
                + usize::from(args.old_line.is_some())
                + usize::from(args.new_line.is_some());
            if target_count != 1 {
                return Err(session_error(
                    "session navigate requires exactly one of --hunk, --old-line, or --new-line",
                ));
            }
            let (side, line) = match (args.old_line, args.new_line) {
                (Some(line), None) => (Some(DiffSide::Old), Some(line)),
                (None, Some(line)) => (Some(DiffSide::New), Some(line)),
                _ => (None, None),
            };
            Ok(SessionCommand::Navigate {
                selector,
                file_path: Some(file_path),
                hunk_number: args.hunk,
                side,
                line,
                comment_direction: None,
                output,
            })
        }
    }
}

fn normalize_session_reload(args: SessionReloadArgs) -> Result<SessionCommand, CliError> {
    let selector = normalize_reload_selector(&args)?;
    let mut nested = vec!["ramo".to_owned()];
    nested.extend(args.review_command);
    let next_input = match super::parse_from(nested, true)?.action {
        Action::Review(ReviewInput::Patch {
            source: PatchSource::Stdin,
            ..
        })
        | Action::Review(ReviewInput::Pager { .. }) => {
            return Err(session_error(
                "session reload requires a repeatable review command, not stdin or pager input",
            ));
        }
        Action::Review(input) => input,
        _ => {
            return Err(session_error(
                "session reload accepts only a ramo review command after --",
            ));
        }
    };
    Ok(SessionCommand::Reload {
        selector,
        next_input,
        source_path: args.source.map(absolute_path),
        output: session_output(args.json),
    })
}

fn normalize_reload_selector(args: &SessionReloadArgs) -> Result<SessionSelector, CliError> {
    let count = usize::from(args.session_id.is_some())
        + usize::from(args.repo.is_some())
        + usize::from(args.session_path.is_some());
    if count != 1 {
        return Err(session_error(
            "select exactly one session id, --repo, or --session-path",
        ));
    }
    if let Some(path) = &args.session_path {
        return Ok(SessionSelector {
            session_path: Some(absolute_path(path.clone()).to_string_lossy().into_owned()),
            ..SessionSelector::default()
        });
    }
    normalize_selector(SessionSelectorArgs {
        session_id: args.session_id.clone(),
        repo: args.repo.clone(),
    })
}

fn normalize_selector(args: SessionSelectorArgs) -> Result<SessionSelector, CliError> {
    match (args.session_id, args.repo) {
        (Some(session_id), None) => Ok(SessionSelector {
            session_id: Some(session_id),
            ..SessionSelector::default()
        }),
        (None, Some(repo)) => {
            let repo = absolute_path(repo);
            let repo_root = crate::vcs::detect::select_vcs(&repo, None)
                .map(|detection| detection.repo_root)
                .unwrap_or(repo);
            Ok(SessionSelector {
                repo_root: Some(repo_root.to_string_lossy().into_owned()),
                ..SessionSelector::default()
            })
        }
        (Some(_), Some(_)) => Err(session_error(
            "select a session by id or --repo, but not both",
        )),
        (None, None) => Err(session_error("a session id or --repo is required")),
    }
}

fn exclusive_line(
    old_line: Option<u32>,
    new_line: Option<u32>,
) -> Result<(DiffSide, u32), CliError> {
    match (old_line, new_line) {
        (Some(line), None) => Ok((DiffSide::Old, line)),
        (None, Some(line)) => Ok((DiffSide::New, line)),
        _ => Err(session_error(
            "specify exactly one of --old-line or --new-line",
        )),
    }
}

fn session_output(json: bool) -> SessionOutput {
    if json {
        SessionOutput::Json
    } else {
        SessionOutput::Text
    }
}

fn session_error(message: impl Into<String>) -> CliError {
    CliError::Session(message.into())
}

fn absolute_path(path: PathBuf) -> PathBuf {
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    };
    std::fs::canonicalize(&path).unwrap_or(path)
}

fn normalize_diff(args: DiffArgs) -> Result<ReviewInput, CliError> {
    let staged = args.staged || args.cached;
    let exclude_untracked = bool_pair(args.exclude_untracked, args.no_exclude_untracked);
    let options = common_options(args.review, args.watch, exclude_untracked);
    match args.targets.as_slice() {
        [] => Ok(ReviewInput::VcsDiff {
            range: None,
            staged,
            pathspecs: args.pathspecs,
            options,
        }),
        [range] => Ok(ReviewInput::VcsDiff {
            range: Some(range.clone()),
            staged,
            pathspecs: args.pathspecs,
            options,
        }),
        [left, right] if args.pathspecs.is_empty() && are_files(left, right) && !staged => {
            Ok(ReviewInput::FilePair {
                left: PathBuf::from(left),
                right: PathBuf::from(right),
                display_path: None,
                options,
            })
        }
        targets => Err(CliError::InvalidDiffTargets(targets.to_vec())),
    }
}

fn normalize_show(args: ShowArgs) -> ReviewInput {
    ReviewInput::Show {
        reference: args.reference,
        pathspecs: args.pathspecs,
        options: common_options(args.review, args.watch, None),
    }
}

fn normalize_stash_show(args: StashShowArgs) -> ReviewInput {
    ReviewInput::StashShow {
        reference: args.reference,
        options: common_options(args.review, false, None),
    }
}

fn normalize_difftool(args: DifftoolArgs) -> ReviewInput {
    ReviewInput::FilePair {
        left: args.left,
        right: args.right,
        display_path: args.path,
        options: common_options(args.review, args.watch, None),
    }
}

fn integration_action(target: String, install: bool) -> Result<Action, CliError> {
    if target != "pi" {
        return Err(CliError::UnsupportedIntegration(target));
    }
    Ok(if install {
        Action::InstallPi
    } else {
        Action::UninstallPi
    })
}

fn common_options(
    flags: ReviewFlags,
    watch: bool,
    exclude_untracked: Option<bool>,
) -> CommonOptions {
    CommonOptions {
        mode: flags.mode.map(|mode| match mode {
            LayoutArg::Auto => LayoutMode::Auto,
            LayoutArg::Split => LayoutMode::Split,
            LayoutArg::Stack => LayoutMode::Stack,
        }),
        theme: flags.theme,
        agent_context: flags.agent_context,
        pager: flags.pager.then_some(true),
        watch: watch.then_some(true),
        exclude_untracked,
        line_numbers: bool_pair(flags.line_numbers, flags.no_line_numbers),
        wrap_lines: bool_pair(flags.wrap, flags.no_wrap),
        hunk_headers: bool_pair(flags.hunk_headers, flags.no_hunk_headers),
        agent_notes: bool_pair(flags.agent_notes, flags.no_agent_notes),
        transparent_background: bool_pair(flags.transparent_bg, flags.no_transparent_bg),
    }
}

fn bool_pair(enabled: bool, disabled: bool) -> Option<bool> {
    if enabled {
        Some(true)
    } else if disabled {
        Some(false)
    } else {
        None
    }
}

fn are_files(left: &str, right: &str) -> bool {
    [left, right].iter().all(|path| Path::new(path).is_file())
}
