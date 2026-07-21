use std::fs;
use std::path::{Path, PathBuf};

use similar::TextDiff;

use crate::core::changeset::stable_file_id;
use crate::core::input::{ReviewInput, VcsId};
use crate::diff::model::{DiffFile, FileChangeKind};
use crate::diff::parser::parse_unified_diff;

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
        let repo_root = resolve_repo_root(input, context)?;
        let repo_name = repo_root
            .file_name()
            .unwrap_or(repo_root.as_os_str())
            .to_string_lossy();
        let (title, args) = match input {
            ReviewInput::VcsDiff {
                range,
                staged,
                pathspecs,
                ..
            } => (
                if *staged {
                    format!("{repo_name} staged changes")
                } else if let Some(range) = range {
                    format!("{repo_name} {range}")
                } else {
                    format!("{repo_name} working tree")
                },
                build_git_diff_args(
                    range.as_deref(),
                    *staged,
                    pathspecs,
                    &[],
                    context.config.color_moved,
                ),
            ),
            ReviewInput::Show {
                reference,
                pathspecs,
                ..
            } => (
                format!(
                    "{repo_name} show {}",
                    reference.as_deref().unwrap_or("HEAD")
                ),
                build_git_show_args(reference.as_deref(), pathspecs, context.config.color_moved),
            ),
            ReviewInput::StashShow { reference, .. } => (
                reference.as_ref().map_or_else(
                    || format!("{repo_name} stash"),
                    |reference| format!("{repo_name} stash {reference}"),
                ),
                build_git_stash_args(reference.as_deref(), context.config.color_moved),
            ),
            _ => {
                return Err(VcsError::UnsupportedOperation {
                    vcs: VcsId::Git,
                    operation: input.kind(),
                });
            }
        };

        let patch_output = run_command(
            input,
            context,
            CommandSpec::new(context.git_executable, &repo_root).args(args),
        )?;
        if matches!(input, ReviewInput::StashShow { .. })
            && patch_output.stdout.is_empty()
            && (patch_output.stderr.is_empty()
                || is_missing_stash(&String::from_utf8_lossy(&patch_output.stderr)))
        {
            return Err(missing_stash_error(input));
        }
        let patch_text = decode_stdout(input, patch_output.stdout)?;
        let extra_files = load_untracked_files(input, context, &repo_root)?;

        Ok(VcsPatch {
            vcs: VcsId::Git,
            source_label: repo_root.display().to_string(),
            repo_root,
            title,
            patch_text,
            extra_files,
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
    let output = run_command(input, context, spec)?;
    let root = decode_stdout(input, output.stdout)?;
    Ok(PathBuf::from(root.trim()))
}

fn run_command(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    spec: CommandSpec,
) -> Result<super::CommandOutput, VcsError> {
    context
        .runner
        .run(&spec)
        .map_err(|error| translate_error(input, context.git_executable, error))
}

fn decode_stdout(input: &ReviewInput, stdout: Vec<u8>) -> Result<String, VcsError> {
    String::from_utf8(stdout).map_err(|_| VcsError::User {
        message: format!(
            "Git returned non-UTF-8 output for `{}`.",
            command_label(input)
        ),
        help: vec!["Use UTF-8 file names and Git output, then try again.".into()],
    })
}

fn load_untracked_files(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    repo_root: &Path,
) -> Result<Vec<DiffFile>, VcsError> {
    let ReviewInput::VcsDiff {
        range,
        staged,
        pathspecs,
        options,
    } = input
    else {
        return Ok(Vec::new());
    };
    if *staged
        || options
            .exclude_untracked
            .unwrap_or(context.config.exclude_untracked)
        || !range_includes_worktree(input, context, repo_root, range.as_deref())?
    {
        return Ok(Vec::new());
    }

    let mut args = vec![
        "--no-optional-locks".into(),
        "status".into(),
        "--porcelain=v1".into(),
        "-z".into(),
        "--untracked-files=all".into(),
    ];
    if !pathspecs.is_empty() {
        args.push("--".into());
        args.extend(pathspecs.iter().cloned());
    }
    let output = run_command(
        input,
        context,
        CommandSpec::new(context.git_executable, repo_root).args(args),
    )?;
    let status = decode_stdout(input, output.stdout)?;
    status
        .split('\0')
        .filter_map(|record| record.strip_prefix("?? "))
        .filter(|path| is_reviewable_untracked(repo_root, path))
        .enumerate()
        .map(|(index, path)| build_untracked_file(repo_root, path, index))
        .collect()
}

fn range_includes_worktree(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    repo_root: &Path,
    range: Option<&str>,
) -> Result<bool, VcsError> {
    let Some(range) = range else {
        return Ok(true);
    };
    let output = run_command(
        input,
        context,
        CommandSpec::new(context.git_executable, repo_root).args([
            "rev-parse",
            "--revs-only",
            range,
        ]),
    )?;
    let revisions = decode_stdout(input, output.stdout)?;
    let (positive, negative) = revisions.lines().filter(|line| !line.is_empty()).fold(
        (0, 0),
        |(positive, negative), line| {
            if line.starts_with('^') {
                (positive, negative + 1)
            } else {
                (positive + 1, negative)
            }
        },
    );
    Ok(positive == 1 && negative == 0)
}

fn is_reviewable_untracked(repo_root: &Path, path: &str) -> bool {
    let absolute = repo_root.join(path);
    let Ok(metadata) = fs::symlink_metadata(&absolute) else {
        return true;
    };
    if metadata.is_dir() {
        return false;
    }
    if !metadata.file_type().is_symlink() {
        return true;
    }
    fs::metadata(absolute)
        .map(|target| !target.is_dir())
        .unwrap_or(true)
}

fn build_untracked_file(repo_root: &Path, path: &str, _index: usize) -> Result<DiffFile, VcsError> {
    let absolute = repo_root.join(path);
    let bytes = fs::read(&absolute).map_err(|source| VcsError::User {
        message: format!(
            "failed to read untracked file {}: {source}",
            absolute.display()
        ),
        help: vec!["Retry after the working tree stops changing.".into()],
    })?;
    if bytes.iter().take(8 * 1024).any(|byte| *byte == 0) {
        return Ok(DiffFile {
            id: stable_file_id(path, None),
            path: path.into(),
            previous_path: None,
            patch: format!("Binary file skipped: {path}\n"),
            hunks: Vec::new(),
            change_kind: FileChangeKind::Added,
            is_binary: true,
            is_untracked: true,
            is_too_large: false,
            stats_truncated: false,
            language: None,
        });
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(DiffFile {
            id: stable_file_id(path, None),
            path: path.into(),
            previous_path: None,
            patch: format!("Binary file skipped: {path}\n"),
            hunks: Vec::new(),
            change_kind: FileChangeKind::Added,
            is_binary: true,
            is_untracked: true,
            is_too_large: false,
            stats_truncated: false,
            language: None,
        });
    };
    let body = TextDiff::from_lines("", &text)
        .unified_diff()
        .context_radius(3)
        .header("/dev/null", &format!("b/{path}"))
        .to_string();
    let patch = format!("diff --git a/{path} b/{path}\nnew file mode 100644\n{body}");
    let mut parsed = parse_unified_diff(&patch);
    let mut file = parsed.pop().ok_or_else(|| VcsError::User {
        message: format!("failed to construct a diff for untracked file {path}"),
        help: vec!["Review the file path and try again.".into()],
    })?;
    file.is_untracked = true;
    Ok(file)
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

fn missing_stash_error(input: &ReviewInput) -> VcsError {
    let reference = match input {
        ReviewInput::StashShow { reference, .. } => reference.as_deref(),
        _ => None,
    };
    VcsError::User {
        message: reference.map_or_else(
            || "`pdiff stash show` could not find a stash entry to show.".into(),
            |reference| {
                format!("`pdiff stash show {reference}` could not resolve that stash entry.")
            },
        ),
        help: vec![
            "Create one with `git stash push`, or list entries with `git stash list`.".into(),
        ],
    }
}
