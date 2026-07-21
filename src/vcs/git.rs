use std::path::{Path, PathBuf};

use crate::core::input::{ReviewInput, VcsId};

use super::command::CommandSpec;
use super::detect;
use super::{VcsAdapter, VcsError, VcsLoadContext, VcsPatch};

const PREFIX_ARGS: &[&str] = &[
    "-c",
    "diff.noprefix=false",
    "-c",
    "diff.mnemonicPrefix=false",
    "-c",
    "diff.srcPrefix=a/",
    "-c",
    "diff.dstPrefix=b/",
];

const MOVED_COLOR_CONFIG: &[&str] = &[
    "-c",
    "color.diff.oldMoved=magenta bold",
    "-c",
    "color.diff.oldMovedAlternative=magenta bold",
    "-c",
    "color.diff.oldMovedDimmed=magenta dim",
    "-c",
    "color.diff.oldMovedAlternativeDimmed=magenta dim",
    "-c",
    "color.diff.newMoved=cyan bold",
    "-c",
    "color.diff.newMovedAlternative=cyan bold",
    "-c",
    "color.diff.newMovedDimmed=cyan dim",
    "-c",
    "color.diff.newMovedAlternativeDimmed=cyan dim",
];

pub struct GitAdapter;

impl VcsAdapter for GitAdapter {
    fn id(&self) -> VcsId {
        VcsId::Git
    }

    fn detect(&self, cwd: &Path) -> Option<PathBuf> {
        detect::detect_root(cwd, VcsId::Git)
    }

    fn load(
        &self,
        input: &ReviewInput,
        context: &VcsLoadContext<'_>,
    ) -> Result<VcsPatch, VcsError> {
        resolve_repo_root(input, context)?;
        Err(VcsError::UnsupportedOperation {
            vcs: VcsId::Git,
            operation: input.kind(),
        })
    }
}

pub fn build_git_diff_args(
    range: Option<&str>,
    staged: bool,
    pathspecs: &[String],
    excluded: &[String],
    color_moved: bool,
) -> Vec<String> {
    let mut args = patch_prefix(color_moved);
    args.extend(
        ["diff", "--no-ext-diff", "--find-renames"]
            .into_iter()
            .map(String::from),
    );
    args.extend(color_args(color_moved));
    if staged {
        args.push("--staged".into());
    }
    if let Some(range) = range {
        args.push(range.into());
    }
    append_pathspecs(&mut args, pathspecs, excluded);
    args
}

pub fn build_git_show_args(
    reference: Option<&str>,
    pathspecs: &[String],
    color_moved: bool,
) -> Vec<String> {
    let mut args = patch_prefix(color_moved);
    args.extend(
        ["show", "--format=", "--no-ext-diff", "--find-renames"]
            .into_iter()
            .map(String::from),
    );
    args.extend(color_args(color_moved));
    if let Some(reference) = reference {
        args.push(reference.into());
    }
    append_pathspecs(&mut args, pathspecs, &[]);
    args
}

pub fn build_git_stash_args(reference: Option<&str>, color_moved: bool) -> Vec<String> {
    let mut args = patch_prefix(color_moved);
    args.extend(
        ["stash", "show", "-p", "--no-ext-diff", "--find-renames"]
            .into_iter()
            .map(String::from),
    );
    args.extend(color_args(color_moved));
    if let Some(reference) = reference {
        args.push(reference.into());
    }
    args
}

fn patch_prefix(color_moved: bool) -> Vec<String> {
    PREFIX_ARGS
        .iter()
        .chain(
            color_moved
                .then_some(MOVED_COLOR_CONFIG)
                .into_iter()
                .flatten(),
        )
        .map(|value| (*value).into())
        .collect()
}

fn color_args(color_moved: bool) -> impl Iterator<Item = String> {
    if color_moved {
        vec!["--color=always".into(), "--color-moved=zebra".into()]
    } else {
        vec!["--no-color".into()]
    }
    .into_iter()
}

fn append_pathspecs(args: &mut Vec<String>, pathspecs: &[String], excluded: &[String]) {
    if pathspecs.is_empty() && excluded.is_empty() {
        return;
    }
    args.push("--".into());
    args.extend(pathspecs.iter().cloned());
    args.extend(excluded.iter().map(|path| format!(":(exclude){path}")));
}

fn resolve_repo_root(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
) -> Result<PathBuf, VcsError> {
    let spec = CommandSpec::new(context.git_executable, context.cwd)
        .args(["rev-parse", "--show-toplevel"]);
    let output = context
        .runner
        .run(&spec)
        .map_err(|error| translate_error(input, context.git_executable, error))?;
    let root = String::from_utf8(output.stdout).map_err(|_| VcsError::User {
        message: "Git returned a repository path that is not valid UTF-8.".into(),
        help: vec!["Run pdiff from a repository with a UTF-8 path.".into()],
    })?;
    Ok(PathBuf::from(root.trim()))
}

fn translate_error(input: &ReviewInput, executable: &str, error: VcsError) -> VcsError {
    match error {
        VcsError::Spawn { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            VcsError::User {
                message: format!(
                    "Git is required for `{}`, but `{executable}` was not found in PATH.",
                    command_label(input)
                ),
                help: vec!["Install Git or make it available on PATH, then try again.".into()],
            }
        }
        VcsError::Exit { stderr, .. } if stderr.contains("not a git repository") => {
            VcsError::User {
                message: format!(
                    "`{}` must be run inside a Git repository.",
                    command_label(input)
                ),
                help: vec!["Run the command from a Git checkout.".into()],
            }
        }
        VcsError::Exit { stderr, .. }
            if matches!(input, ReviewInput::StashShow { .. }) && is_missing_stash(&stderr) =>
        {
            VcsError::User {
                message: "`pdiff stash show` could not find a stash entry to show.".into(),
                help: vec![
                    "Create one with `git stash push`, or pass an explicit stash ref.".into(),
                ],
            }
        }
        VcsError::Exit { stderr, .. } if is_unknown_revision(&stderr) => {
            let (kind, value) = revision_value(input);
            VcsError::User {
                message: format!(
                    "`{}` could not resolve Git {kind} `{value}`.",
                    command_label(input)
                ),
                help: vec![format!("Check the {kind} and try again.")],
            }
        }
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
        ReviewInput::StashShow {
            reference: Some(reference),
            ..
        } => format!("pdiff stash show {reference}"),
        ReviewInput::StashShow { .. } => "pdiff stash show".into(),
        _ => format!("pdiff {:?}", input.kind()),
    }
}

fn revision_value(input: &ReviewInput) -> (&'static str, &str) {
    match input {
        ReviewInput::VcsDiff {
            range: Some(range), ..
        } => ("revision or range", range),
        ReviewInput::Show { reference, .. } => ("ref", reference.as_deref().unwrap_or("HEAD")),
        ReviewInput::StashShow { reference, .. } => {
            ("stash entry", reference.as_deref().unwrap_or("stash@{0}"))
        }
        _ => ("ref", "HEAD"),
    }
}

fn is_unknown_revision(stderr: &str) -> bool {
    [
        "bad revision",
        "unknown revision or path not in the working tree",
        "ambiguous argument",
        "Needed a single revision",
    ]
    .iter()
    .any(|fragment| stderr.contains(fragment))
}

fn is_missing_stash(stderr: &str) -> bool {
    stderr.contains("No stash entries found.") || stderr.contains("log for 'stash' only has")
}
