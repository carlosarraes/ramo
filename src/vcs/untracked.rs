use std::fs;
use std::io::Read;
use std::path::Path;

use similar::TextDiff;

use crate::core::changeset::stable_file_id;
use crate::diff::model::{DiffFile, FileChangeKind, FileStats, SourceSpec};
use crate::diff::parser::parse_unified_diff;

use super::VcsError;

const LARGE_DIFF_FILE_MAX_BYTES: u64 = 1_000_000;
const LARGE_DIFF_FILE_MAX_LINES: usize = 20_000;
const LARGE_DIFF_FILE_SNIFF_BYTES: usize = 256 * 1024;

pub(crate) fn build_filesystem_untracked_file(
    repo_root: &Path,
    path: &str,
) -> Result<DiffFile, VcsError> {
    let absolute = repo_root.join(path);
    if let Some((stats, stats_truncated)) = inspect_large(&absolute)? {
        return Ok(DiffFile {
            id: stable_file_id(path, None),
            path: path.into(),
            previous_path: None,
            summary: None,
            patch: String::new(),
            hunks: Vec::new(),
            change_kind: FileChangeKind::Added,
            is_binary: false,
            is_untracked: true,
            is_too_large: true,
            stats_truncated,
            language: None,
            stats,
            old_source: SourceSpec::None,
            new_source: SourceSpec::File(absolute),
        });
    }
    let bytes = fs::read(&absolute).map_err(|source| io_error("read", &absolute, source))?;
    if bytes.iter().take(8 * 1024).any(|byte| *byte == 0) {
        return Ok(binary_file(path, absolute));
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(binary_file(path, absolute));
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
    file.old_source = SourceSpec::None;
    file.new_source = SourceSpec::File(absolute);
    Ok(file)
}

pub(crate) fn is_reviewable_path(repo_root: &Path, path: &str) -> bool {
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

fn binary_file(path: &str, absolute: std::path::PathBuf) -> DiffFile {
    DiffFile {
        id: stable_file_id(path, None),
        path: path.into(),
        previous_path: None,
        summary: None,
        patch: format!("Binary file skipped: {path}\n"),
        hunks: Vec::new(),
        change_kind: FileChangeKind::Added,
        is_binary: true,
        is_untracked: true,
        is_too_large: false,
        stats_truncated: false,
        language: None,
        stats: FileStats::default(),
        old_source: SourceSpec::None,
        new_source: SourceSpec::File(absolute),
    }
}

fn inspect_large(path: &Path) -> Result<Option<(FileStats, bool)>, VcsError> {
    let metadata = fs::metadata(path).map_err(|source| io_error("inspect", path, source))?;
    let read_limit = if metadata.len() > LARGE_DIFF_FILE_MAX_BYTES {
        LARGE_DIFF_FILE_MAX_BYTES as usize
    } else {
        LARGE_DIFF_FILE_SNIFF_BYTES
    };
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(read_limit as u64)
                .read_to_end(&mut bytes)
                .map(|_| ())
        })
        .map_err(|source| io_error("inspect", path, source))?;
    let mut lines = bytes.iter().filter(|byte| **byte == b'\n').count();
    if bytes.last().is_some_and(|byte| *byte != b'\n') {
        lines += 1;
    }
    let complete = bytes.len() as u64 >= metadata.len();
    let should_skip =
        metadata.len() > LARGE_DIFF_FILE_MAX_BYTES || lines > LARGE_DIFF_FILE_MAX_LINES;
    Ok(should_skip.then_some((
        FileStats {
            additions: lines,
            deletions: 0,
        },
        !complete,
    )))
}

fn io_error(operation: &str, path: &Path, source: std::io::Error) -> VcsError {
    VcsError::User {
        message: format!(
            "failed to {operation} untracked file {}: {source}",
            path.display()
        ),
        help: vec!["Retry after the working tree stops changing.".into()],
    }
}
