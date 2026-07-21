use std::fs;
use std::path::{Path, PathBuf};

use similar::TextDiff;

use crate::core::changeset::{Changeset, stable_file_id};
use crate::diff::model::{DiffFile, FileChangeKind};
use crate::diff::parser::parse_unified_diff;

use super::{LoadError, LoadedReview, ReloadPlan};

pub(super) fn load(
    left: &Path,
    right: &Path,
    display_path: Option<&Path>,
) -> Result<LoadedReview, LoadError> {
    let left_bytes = read(left)?;
    let right_bytes = read(right)?;
    let display = display_path
        .map(Path::to_path_buf)
        .or_else(|| right.file_name().map(PathBuf::from))
        .unwrap_or_else(|| right.to_path_buf());
    let display_text = display.to_string_lossy().replace('\\', "/");
    let source_label = if display_path.is_some() {
        "vcs difftool"
    } else {
        "file compare"
    };
    let title = format!(
        "{} ↔ {}",
        left.file_name()
            .unwrap_or(left.as_os_str())
            .to_string_lossy(),
        right
            .file_name()
            .unwrap_or(right.as_os_str())
            .to_string_lossy()
    );

    let files = if is_binary(&left_bytes) || is_binary(&right_bytes) {
        vec![binary_file(left, &display_text)]
    } else {
        let left_text = String::from_utf8(left_bytes).map_err(|_| LoadError::NonUtf8 {
            path: left.to_path_buf(),
        })?;
        let right_text = String::from_utf8(right_bytes).map_err(|_| LoadError::NonUtf8 {
            path: right.to_path_buf(),
        })?;
        let diff = TextDiff::from_lines(&left_text, &right_text);
        let body = diff
            .unified_diff()
            .context_radius(3)
            .header(&format!("a/{display_text}"), &format!("b/{display_text}"))
            .to_string();
        if body.is_empty() {
            Vec::new()
        } else {
            parse_unified_diff(&format!(
                "diff --git a/{display_text} b/{display_text}\n{body}"
            ))
        }
    };

    Ok(LoadedReview {
        changeset: Changeset::new(source_label, title, files),
        reload_plan: ReloadPlan::Files {
            left: left.to_path_buf(),
            right: right.to_path_buf(),
        },
    })
}

fn read(path: &Path) -> Result<Vec<u8>, LoadError> {
    fs::read(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8 * 1024).any(|byte| *byte == 0)
}

fn binary_file(left: &Path, display_path: &str) -> DiffFile {
    let previous_path = left
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| name != display_path);
    DiffFile {
        id: stable_file_id(display_path, previous_path.as_deref()),
        path: display_path.into(),
        previous_path,
        patch: "Binary file contents differ\n".into(),
        hunks: Vec::new(),
        change_kind: FileChangeKind::Modified,
        is_binary: true,
        is_untracked: false,
        is_too_large: false,
        stats_truncated: false,
        language: None,
    }
}
