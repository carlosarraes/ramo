use std::path::{Path, PathBuf};

use crate::core::input::{ReviewInput, VcsId};

use super::detect;
use super::{CommandSpec, VcsAdapter, VcsError, VcsLoadContext, VcsPatch};

pub struct JjAdapter;

impl VcsAdapter for JjAdapter {
    fn id(&self) -> VcsId {
        VcsId::Jj
    }

    fn detect(&self, cwd: &Path) -> Option<PathBuf> {
        detect::detect_root(cwd, VcsId::Jj)
    }

    fn load(
        &self,
        input: &ReviewInput,
        context: &VcsLoadContext<'_>,
    ) -> Result<VcsPatch, VcsError> {
        if matches!(input, ReviewInput::VcsDiff { staged: true, .. }) {
            return Err(VcsError::User {
                message: "`pdiff diff --staged` cannot run because Jujutsu has no staging area."
                    .into(),
                help: vec!["Remove `--staged`, or set `vcs = \"git\"` in pdiff config.".into()],
            });
        }
        if matches!(input, ReviewInput::StashShow { .. }) {
            return Err(VcsError::User {
                message: "Jujutsu does not support stash show.".into(),
                help: vec!["Use Git VCS mode for `pdiff stash show`.".into()],
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
                build_jj_diff_args(range.as_deref(), pathspecs),
            ),
            ReviewInput::Show {
                reference,
                pathspecs,
                ..
            } => (
                format!("{repo_name} show {}", reference.as_deref().unwrap_or("@")),
                build_jj_show_args(reference.as_deref(), pathspecs),
            ),
            _ => {
                return Err(VcsError::UnsupportedOperation {
                    vcs: VcsId::Jj,
                    operation: input.kind(),
                });
            }
        };
        let patch_text = run_text(input, context, args)?;
        Ok(VcsPatch {
            vcs: VcsId::Jj,
            source_label: repo_root.display().to_string(),
            repo_root,
            title,
            patch_text,
            extra_files: Vec::new(),
            source_endpoints: None,
        })
    }
}

pub fn build_jj_diff_args(range: Option<&str>, pathspecs: &[String]) -> Vec<String> {
    let mut args = vec!["diff".into(), "--git".into()];
    if let Some(range) = range {
        args.extend(["-r".into(), range.into()]);
    }
    append_pathspecs(&mut args, pathspecs);
    args
}

pub fn build_jj_show_args(reference: Option<&str>, pathspecs: &[String]) -> Vec<String> {
    let mut args = vec![
        "diff".into(),
        "--git".into(),
        "-r".into(),
        reference.unwrap_or("@").into(),
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

fn run_text(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    args: impl IntoIterator<Item = impl Into<String>>,
) -> Result<String, VcsError> {
    let mut full_args = vec!["--no-pager".into(), "--color".into(), "never".into()];
    full_args.extend(args.into_iter().map(Into::into));
    let output = context
        .runner
        .run(&CommandSpec::new(context.jj_executable, context.cwd).args(full_args))
        .map_err(|error| translate_error(input, context.jj_executable, error))?;
    String::from_utf8(output.stdout).map_err(|_| VcsError::User {
        message: format!(
            "Jujutsu returned non-UTF-8 output for `{}`.",
            command_label(input)
        ),
        help: vec!["Use UTF-8 paths and Jujutsu output, then try again.".into()],
    })
}

fn translate_error(input: &ReviewInput, executable: &str, error: VcsError) -> VcsError {
    match error {
        VcsError::Spawn { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            VcsError::User {
                message: format!(
                    "Jujutsu is required for `{}`, but `{executable}` was not found in PATH.",
                    command_label(input)
                ),
                help: vec!["Install Jujutsu or set `vcs = \"git\"`, then try again.".into()],
            }
        }
        VcsError::Exit { stderr, .. } if is_missing_repo(&stderr) => VcsError::User {
            message: format!(
                "`{}` must be run inside a Jujutsu repository.",
                command_label(input)
            ),
            help: vec!["Run the command from a Jujutsu workspace.".into()],
        },
        VcsError::Exit { stderr, .. } if is_invalid_revset(&stderr) => VcsError::User {
            message: format!(
                "`{}` could not resolve Jujutsu revset `{}`.",
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
        ReviewInput::VcsDiff { staged: true, .. } => "pdiff diff --staged".into(),
        ReviewInput::VcsDiff {
            range: Some(range), ..
        } => format!("pdiff diff {range}"),
        ReviewInput::VcsDiff { .. } => "pdiff diff".into(),
        ReviewInput::Show {
            reference: Some(reference),
            ..
        } => format!("pdiff show {reference}"),
        ReviewInput::Show { .. } => "pdiff show".into(),
        _ => format!("pdiff {:?}", input.kind()),
    }
}

fn revset(input: &ReviewInput) -> &str {
    match input {
        ReviewInput::VcsDiff { range, .. } => range.as_deref().unwrap_or("@"),
        ReviewInput::Show { reference, .. } => reference.as_deref().unwrap_or("@"),
        _ => "@",
    }
}

fn is_missing_repo(stderr: &str) -> bool {
    stderr.contains("There is no jj repo in") || stderr.contains("not in a workspace")
}

fn is_invalid_revset(stderr: &str) -> bool {
    [
        "Failed to parse revset",
        "Revision not found",
        "No such revision",
        "doesn't exist",
        "is ambiguous",
        "Revset expression resolved to no revisions",
    ]
    .iter()
    .any(|fragment| stderr.contains(fragment))
}

fn first_error_line(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Jujutsu command failed.")
        .trim_start_matches("Error:")
        .trim()
        .into()
}
