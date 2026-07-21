use std::fs;
use std::path::{Path, PathBuf};

use crate::core::changeset::stable_file_id;
use crate::core::input::{ReviewInput, VcsId};
use crate::diff::model::{
    DiffFile, FileChangeKind, FileStats, LineType, MovedLineKind, SourceSpec,
};
use crate::diff::parser::parse_unified_diff;

use super::command::CommandSpec;
use super::detect;
use super::{SourceEndpoint, SourceEndpoints, VcsAdapter, VcsError, VcsLoadContext, VcsPatch};

const LARGE_DIFF_FILE_MAX_BYTES: u64 = 1_000_000;
const LARGE_DIFF_FILE_MAX_LINES: usize = 20_000;

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

pub fn parse_git_patch(input: &str) -> Vec<DiffFile> {
    let move_kinds = input
        .lines()
        .filter_map(|line| {
            let clean = strip_ansi_line(line);
            if clean.starts_with("--- ") || clean.starts_with("+++ ") {
                return None;
            }
            matches!(clean.as_bytes().first(), Some(b'+') | Some(b'-'))
                .then(|| moved_line_kind(line, clean.as_bytes()[0]))
        })
        .collect::<Vec<_>>();
    let mut move_kinds = move_kinds.into_iter();
    let mut files = parse_unified_diff(input);
    for line in files
        .iter_mut()
        .flat_map(|file| &mut file.hunks)
        .flat_map(|hunk| &mut hunk.lines)
        .filter(|line| matches!(line.kind, LineType::Addition | LineType::Deletion))
    {
        line.moved = move_kinds.next().flatten();
    }
    files
}

fn moved_line_kind(line: &str, sign: u8) -> Option<MovedLineKind> {
    let mut parameters = Vec::new();
    let bytes = line.as_bytes();
    let mut index = 0;
    while index + 2 < bytes.len() {
        if bytes[index] == 0x1b && bytes[index + 1] == b'[' {
            let start = index + 2;
            index = start;
            while index < bytes.len() && !(0x40..=0x7e).contains(&bytes[index]) {
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'm' {
                parameters.extend(
                    String::from_utf8_lossy(&bytes[start..index])
                        .split(';')
                        .filter_map(|value| value.parse::<u16>().ok()),
                );
            }
        }
        index += 1;
    }
    let dimmed = parameters.contains(&2);
    match sign {
        b'-' if parameters.contains(&35) && dimmed => Some(MovedLineKind::OldMovedDimmed),
        b'-' if parameters.contains(&35) => Some(MovedLineKind::OldMoved),
        b'+' if parameters.contains(&36) && dimmed => Some(MovedLineKind::NewMovedDimmed),
        b'+' if parameters.contains(&36) => Some(MovedLineKind::NewMoved),
        _ => None,
    }
}

fn strip_ansi_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' && characters.peek() == Some(&'[') {
            characters.next();
            for character in characters.by_ref() {
                if ('@'..='~').contains(&character) {
                    break;
                }
            }
        } else {
            output.push(character);
        }
    }
    output
}

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
        let mut extra_files = Vec::new();
        let (title, args) = match input {
            ReviewInput::VcsDiff {
                range,
                staged,
                pathspecs,
                ..
            } => {
                let large_files = large_tracked_files(input, context, &repo_root)?;
                let excluded = large_files
                    .iter()
                    .map(|file| file.path.clone())
                    .collect::<Vec<_>>();
                extra_files.extend(large_files.into_iter().map(large_tracked_placeholder));
                (
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
                        &excluded,
                        context.config.color_moved,
                    ),
                )
            }
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
        extra_files.extend(load_untracked_files(input, context, &repo_root)?);
        let source_endpoints = resolve_source_endpoints(input, context, &repo_root)?;

        Ok(VcsPatch {
            vcs: VcsId::Git,
            source_label: repo_root.display().to_string(),
            repo_root,
            title,
            patch_text,
            extra_files,
            source_endpoints,
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

fn build_git_numstat_args(range: Option<&str>, staged: bool, pathspecs: &[String]) -> Vec<String> {
    let mut args = PREFIX_ARGS
        .iter()
        .map(|value| (*value).into())
        .collect::<Vec<_>>();
    args.extend(
        [
            "diff",
            "--no-ext-diff",
            "--find-renames",
            "--no-color",
            "--numstat",
            "-z",
        ]
        .into_iter()
        .map(String::from),
    );
    if staged {
        args.push("--staged".into());
    }
    if let Some(range) = range {
        args.push(range.into());
    }
    append_pathspecs(&mut args, pathspecs, &[]);
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

#[derive(Debug)]
struct LargeTrackedFile {
    path: String,
    stats: FileStats,
}

fn large_tracked_files(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    repo_root: &Path,
) -> Result<Vec<LargeTrackedFile>, VcsError> {
    let ReviewInput::VcsDiff {
        range,
        staged,
        pathspecs,
        ..
    } = input
    else {
        return Ok(Vec::new());
    };
    let output = run_command(
        input,
        context,
        CommandSpec::new(context.git_executable, repo_root).args(build_git_numstat_args(
            range.as_deref(),
            *staged,
            pathspecs,
        )),
    )?;
    let text = decode_stdout(input, output.stdout)?;
    Ok(text
        .split('\0')
        .filter_map(|entry| {
            let mut fields = entry.splitn(3, '\t');
            let additions = fields.next()?.parse::<usize>().ok()?;
            let deletions = fields.next()?.parse::<usize>().ok()?;
            let path = fields.next()?.to_string();
            let changed_lines = additions.saturating_add(deletions);
            let current_size = fs::metadata(repo_root.join(&path))
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            should_skip_large(changed_lines, current_size).then_some(LargeTrackedFile {
                path,
                stats: FileStats {
                    additions,
                    deletions,
                },
            })
        })
        .collect())
}

fn large_tracked_placeholder(file: LargeTrackedFile) -> DiffFile {
    DiffFile {
        id: stable_file_id(&file.path, None),
        path: file.path,
        previous_path: None,
        patch: String::new(),
        hunks: Vec::new(),
        change_kind: FileChangeKind::Modified,
        is_binary: false,
        is_untracked: false,
        is_too_large: true,
        stats_truncated: false,
        language: None,
        stats: file.stats,
        old_source: SourceSpec::None,
        new_source: SourceSpec::None,
    }
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
        .filter(|path| super::untracked::is_reviewable_path(repo_root, path))
        .map(|path| super::untracked::build_filesystem_untracked_file(repo_root, path))
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

fn resolve_source_endpoints(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    repo_root: &Path,
) -> Result<Option<SourceEndpoints>, VcsError> {
    let endpoints = match input {
        ReviewInput::VcsDiff { range, staged, .. } => {
            if *staged {
                let old = if let Some(range) = range {
                    let (positive, negative) =
                        resolve_range_revisions(input, context, repo_root, range)?;
                    if positive.len() == 1 && negative.is_empty() {
                        SourceEndpoint::GitBlob {
                            repo_root: repo_root.into(),
                            reference: positive[0].clone(),
                        }
                    } else {
                        return Ok(None);
                    }
                } else if let Some(head) = try_resolve_commit(input, context, repo_root, "HEAD")? {
                    SourceEndpoint::GitBlob {
                        repo_root: repo_root.into(),
                        reference: head,
                    }
                } else {
                    SourceEndpoint::None
                };
                SourceEndpoints {
                    old,
                    new: SourceEndpoint::GitIndex {
                        repo_root: repo_root.into(),
                    },
                }
            } else if let Some(range) = range {
                if let Some((left, right)) = symmetric_range(range) {
                    let merge_base = run_git_text(
                        input,
                        context,
                        CommandSpec::new(context.git_executable, repo_root).args([
                            "merge-base",
                            left.as_str(),
                            right.as_str(),
                        ]),
                    )?;
                    let right = resolve_commit(input, context, repo_root, &right)?;
                    SourceEndpoints {
                        old: SourceEndpoint::GitBlob {
                            repo_root: repo_root.into(),
                            reference: merge_base.trim().into(),
                        },
                        new: SourceEndpoint::GitBlob {
                            repo_root: repo_root.into(),
                            reference: right,
                        },
                    }
                } else {
                    let (positive, negative) =
                        resolve_range_revisions(input, context, repo_root, range)?;
                    match (positive.as_slice(), negative.as_slice()) {
                        ([old], []) => SourceEndpoints {
                            old: SourceEndpoint::GitBlob {
                                repo_root: repo_root.into(),
                                reference: old.clone(),
                            },
                            new: SourceEndpoint::Worktree {
                                repo_root: repo_root.into(),
                            },
                        },
                        ([new], [old]) => SourceEndpoints {
                            old: SourceEndpoint::GitBlob {
                                repo_root: repo_root.into(),
                                reference: old.clone(),
                            },
                            new: SourceEndpoint::GitBlob {
                                repo_root: repo_root.into(),
                                reference: new.clone(),
                            },
                        },
                        _ => return Ok(None),
                    }
                }
            } else {
                SourceEndpoints {
                    old: SourceEndpoint::GitIndex {
                        repo_root: repo_root.into(),
                    },
                    new: SourceEndpoint::Worktree {
                        repo_root: repo_root.into(),
                    },
                }
            }
        }
        ReviewInput::Show { reference, .. } => {
            let new = resolve_commit(
                input,
                context,
                repo_root,
                reference.as_deref().unwrap_or("HEAD"),
            )?;
            SourceEndpoints {
                old: SourceEndpoint::GitBlob {
                    repo_root: repo_root.into(),
                    reference: format!("{new}^"),
                },
                new: SourceEndpoint::GitBlob {
                    repo_root: repo_root.into(),
                    reference: new,
                },
            }
        }
        ReviewInput::StashShow { reference, .. } => {
            let new = resolve_commit(
                input,
                context,
                repo_root,
                reference.as_deref().unwrap_or("stash@{0}"),
            )?;
            SourceEndpoints {
                old: SourceEndpoint::GitBlob {
                    repo_root: repo_root.into(),
                    reference: format!("{new}^"),
                },
                new: SourceEndpoint::GitBlob {
                    repo_root: repo_root.into(),
                    reference: new,
                },
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(endpoints))
}

fn resolve_commit(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    repo_root: &Path,
    reference: &str,
) -> Result<String, VcsError> {
    run_git_text(
        input,
        context,
        CommandSpec::new(context.git_executable, repo_root).args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ]),
    )
    .map(|value| value.lines().next().unwrap_or_default().trim().into())
}

fn try_resolve_commit(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    repo_root: &Path,
    reference: &str,
) -> Result<Option<String>, VcsError> {
    let object = format!("{reference}^{{commit}}");
    let output = run_command(
        input,
        context,
        CommandSpec::new(context.git_executable, repo_root)
            .args(["rev-parse", "--verify", "--end-of-options", object.as_str()])
            .accepted_exit_codes([0, 1, 128]),
    )?;
    if output.code == 0 {
        decode_stdout(input, output.stdout).map(|value| Some(value.trim().into()))
    } else {
        Ok(None)
    }
}

fn resolve_range_revisions(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    repo_root: &Path,
    range: &str,
) -> Result<(Vec<String>, Vec<String>), VcsError> {
    let revisions = run_git_text(
        input,
        context,
        CommandSpec::new(context.git_executable, repo_root).args([
            "rev-parse",
            "--revs-only",
            range,
        ]),
    )?;
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    for revision in revisions.lines().filter(|line| !line.is_empty()) {
        if let Some(revision) = revision.strip_prefix('^') {
            negative.push(revision.into());
        } else {
            positive.push(revision.into());
        }
    }
    Ok((positive, negative))
}

fn symmetric_range(range: &str) -> Option<(String, String)> {
    if range.contains("....") {
        return None;
    }
    let mut parts = range.split("...");
    let left = parts.next()?;
    let right = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some((
        if left.is_empty() { "HEAD" } else { left }.into(),
        if right.is_empty() { "HEAD" } else { right }.into(),
    ))
}

fn run_git_text(
    input: &ReviewInput,
    context: &VcsLoadContext<'_>,
    spec: CommandSpec,
) -> Result<String, VcsError> {
    let output = run_command(input, context, spec)?;
    decode_stdout(input, output.stdout)
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

fn should_skip_large(lines: usize, bytes: u64) -> bool {
    lines > LARGE_DIFF_FILE_MAX_LINES || bytes > LARGE_DIFF_FILE_MAX_BYTES
}

#[cfg(test)]
mod large_file_tests {
    use super::should_skip_large;

    #[test]
    fn large_file_thresholds_are_exclusive() {
        assert!(!should_skip_large(20_000, 1_000_000));
        assert!(should_skip_large(20_001, 1_000_000));
        assert!(should_skip_large(20_000, 1_000_001));
    }
}
