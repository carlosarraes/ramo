use std::path::{Path, PathBuf};

use crate::core::input::{ReviewInput, VcsId};

use super::detect;
use super::{CommandSpec, VcsAdapter, VcsError, VcsLoadContext, VcsPatch};

pub struct SaplingAdapter;

impl VcsAdapter for SaplingAdapter {
    fn id(&self) -> VcsId {
        VcsId::Sl
    }

    fn detect(&self, cwd: &Path) -> Option<PathBuf> {
        detect::detect_root(cwd, VcsId::Sl)
    }

    fn load(
        &self,
        input: &ReviewInput,
        context: &VcsLoadContext<'_>,
    ) -> Result<VcsPatch, VcsError> {
        if matches!(input, ReviewInput::VcsDiff { staged: true, .. }) {
            return Err(VcsError::User {
                message: "`ramo diff --staged` cannot run because Sapling has no staging area."
                    .into(),
                help: vec!["Remove `--staged`, or set `vcs = \"git\"` in ramo config.".into()],
            });
        }
        if matches!(input, ReviewInput::StashShow { .. }) {
            return Err(VcsError::User {
                message: "Sapling does not support stash show.".into(),
                help: vec!["Use Git VCS mode for `ramo stash show`.".into()],
            });
        }

        let root = run_text(input, context, ["root"])?;
        let repo_root = PathBuf::from(root.trim());
        let repo_name = repo_root
            .file_name()
            .unwrap_or(repo_root.as_os_str())
            .to_string_lossy();
        let (title, args) = match input {
            ReviewInput::VcsDiff {
                range, pathspecs, ..
            } => (
                range.as_ref().map_or_else(
                    || format!("{repo_name} working copy"),
                    |range| format!("{repo_name} {range}"),
                ),
                build_sl_diff_args(range.as_deref(), pathspecs),
            ),
            ReviewInput::Show {
                reference,
                pathspecs,
                ..
            } => (
                format!("{repo_name} show {}", reference.as_deref().unwrap_or(".")),
                build_sl_show_args(reference.as_deref(), pathspecs),
            ),
            _ => {
                return Err(VcsError::UnsupportedOperation {
                    vcs: VcsId::Sl,
                    operation: input.kind(),
                });
            }
        };
        let patch_text = run_text(input, context, args)?;
        let extra_files = load_unknown_files(input, context, &repo_root)?;
        Ok(VcsPatch {
            vcs: VcsId::Sl,
            source_label: repo_root.display().to_string(),
            repo_root,
            title,
            patch_text,
            extra_files,
            source_endpoints: None,
        })
    }
}

pub fn build_sl_diff_args(range: Option<&str>, pathspecs: &[String]) -> Vec<String> {
    let mut args = vec!["diff".into(), "--git".into()];
    if let Some(range) = range {
        args.extend(["-r".into(), range.into()]);
    }
    append_pathspecs(&mut args, pathspecs);
    args
}

pub fn build_sl_show_args(reference: Option<&str>, pathspecs: &[String]) -> Vec<String> {
    let mut args = vec![
        "diff".into(),
        "--git".into(),
        "--change".into(),
        reference.unwrap_or(".").into(),
    ];
    append_pathspecs(&mut args, pathspecs);
    args
}

fn build_status_args(pathspecs: &[String]) -> Vec<String> {
    let mut args = vec![
        "status".into(),
        "--unknown".into(),
        "--print0".into(),
        "--root-relative".into(),
    ];
    append_pathspecs(&mut args, pathspecs);
    args
}

fn append_pathspecs(args: &mut Vec<String>, pathspecs: &[String]) {
    if !pathspecs.is_empty() {
        args.push("--".into());
        args.extend(pathspecs.iter().cloned());
    }
}

fn load_unknown_files(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    repo_root: &Path,
) -> Result<Vec<crate::diff::model::DiffFile>, VcsError> {
    let ReviewInput::VcsDiff {
        staged,
        pathspecs,
        options,
        ..
    } = input
    else {
        return Ok(Vec::new());
    };
    if *staged
        || options
            .exclude_untracked
            .unwrap_or(context.config.exclude_untracked)
    {
        return Ok(Vec::new());
    }
    let status = run_text(input, context, build_status_args(pathspecs))?;
    status
        .split('\0')
        .filter_map(|record| record.strip_prefix("? "))
        .filter(|path| super::untracked::is_reviewable_path(repo_root, path))
        .map(|path| super::untracked::build_filesystem_untracked_file(repo_root, path))
        .collect()
}

fn run_text(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    args: impl IntoIterator<Item = impl Into<String>>,
) -> Result<String, VcsError> {
    let mut full_args = vec!["--noninteractive".into(), "--color".into(), "never".into()];
    full_args.extend(args.into_iter().map(Into::into));
    let output = context
        .runner
        .run(&CommandSpec::new(context.sl_executable, context.cwd).args(full_args))
        .map_err(|error| translate_error(input, context.sl_executable, error))?;
    String::from_utf8(output.stdout).map_err(|_| VcsError::User {
        message: format!(
            "Sapling returned non-UTF-8 output for `{}`.",
            command_label(input)
        ),
        help: vec!["Use UTF-8 paths and Sapling output, then try again.".into()],
    })
}

fn translate_error(input: &ReviewInput, executable: &str, error: VcsError) -> VcsError {
    match error {
        VcsError::Spawn { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            VcsError::User {
                message: format!(
                    "Sapling is required for `{}`, but `{executable}` was not found in PATH.",
                    command_label(input)
                ),
                help: vec!["Install Sapling or set `vcs = \"git\"`, then try again.".into()],
            }
        }
        VcsError::Exit { stderr, .. } if is_missing_repo(&stderr) => VcsError::User {
            message: format!(
                "`{}` must be run inside a Sapling repository.",
                command_label(input)
            ),
            help: vec!["Run the command from a Sapling checkout.".into()],
        },
        VcsError::Exit { stderr, .. } if is_invalid_revset(&stderr) => VcsError::User {
            message: format!(
                "`{}` could not resolve Sapling revset `{}`.",
                command_label(input),
                revset(input)
            ),
            help: vec!["Check the revset and try again.".into()],
        },
        VcsError::Exit { stderr, .. } => VcsError::User {
            message: format!("`{}` failed.", command_label(input)),
            help: vec![first_error_line(&stderr)],
        },
        other => other,
    }
}

fn command_label(input: &ReviewInput) -> String {
    match input {
        ReviewInput::VcsDiff { staged: true, .. } => "ramo diff --staged".into(),
        ReviewInput::VcsDiff {
            range: Some(range), ..
        } => format!("ramo diff {range}"),
        ReviewInput::VcsDiff { .. } => "ramo diff".into(),
        ReviewInput::Show {
            reference: Some(reference),
            ..
        } => format!("ramo show {reference}"),
        ReviewInput::Show { .. } => "ramo show".into(),
        _ => format!("ramo {:?}", input.kind()),
    }
}

fn revset(input: &ReviewInput) -> &str {
    match input {
        ReviewInput::VcsDiff { range, .. } => range.as_deref().unwrap_or("."),
        ReviewInput::Show { reference, .. } => reference.as_deref().unwrap_or("."),
        _ => ".",
    }
}

fn is_missing_repo(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    [
        "is not inside a repository",
        "not in a repository",
        "no repository found",
    ]
    .iter()
    .any(|fragment| stderr.contains(fragment))
}

fn is_invalid_revset(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    [
        "unknown revision",
        "ambiguous identifier",
        "can't find revision",
        "is not a valid revision",
        "revision not found",
        "syntax error in revset",
    ]
    .iter()
    .any(|fragment| stderr.contains(fragment))
}

fn first_error_line(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Sapling command failed.")
        .trim_start_matches("abort:")
        .trim_start_matches("error:")
        .trim()
        .into()
}
