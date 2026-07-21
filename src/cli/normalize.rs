use std::path::{Path, PathBuf};

use super::args::{
    Cli, Command, DiffArgs, DifftoolArgs, LayoutArg, ReviewFlags, ShowArgs, StashCommand,
    StashShowArgs,
};
use super::{Action, CliError, Invocation};
use crate::core::input::{CommonOptions, LayoutMode, PatchSource, ReviewInput, ReviewOutput};

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
        (Some(_), Some(_)) => unreachable!("conflicting input returned above"),
    };

    Ok(Invocation { action, output })
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
