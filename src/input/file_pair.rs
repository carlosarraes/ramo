use std::fs;
use std::path::{Path, PathBuf};

use similar::TextDiff;

use crate::core::changeset::{Changeset, stable_file_id};
use crate::diff::model::{DiffFile, FileChangeKind, FileStats, SourceSpec};
use crate::diff::parser::parse_unified_diff;

use super::{LoadError, LoadedReview, ReloadPlan};

pub(super) fn load(
    left: &Path,
    right: &Path,
    display_path: Option<&Path>,
) -> Result<LoadedReview, LoadError> {
    let is_difftool = display_path.is_some();
    let left_absent = is_difftool && is_null_path(left);
    let right_absent = is_difftool && is_null_path(right);
    let left_bytes = if left_absent { Vec::new() } else { read(left)? };
    let right_bytes = if right_absent {
        Vec::new()
    } else {
        read(right)?
    };
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

    let mut files = if is_binary(&left_bytes) || is_binary(&right_bytes) {
        vec![binary_file(
            left,
            right,
            &display_text,
            left_absent,
            right_absent,
        )]
    } else {
        let left_text = String::from_utf8(left_bytes).map_err(|_| LoadError::NonUtf8 {
            path: left.to_path_buf(),
        })?;
        let right_text = String::from_utf8(right_bytes).map_err(|_| LoadError::NonUtf8 {
            path: right.to_path_buf(),
        })?;
        let diff = TextDiff::from_lines(&left_text, &right_text);
        let old_header = if left_absent {
            "/dev/null".to_string()
        } else {
            format!("a/{display_text}")
        };
        let new_header = if right_absent {
            "/dev/null".to_string()
        } else {
            format!("b/{display_text}")
        };
        let body = diff
            .unified_diff()
            .context_radius(3)
            .header(&old_header, &new_header)
            .to_string();
        if body.is_empty() {
            Vec::new()
        } else {
            let metadata = if left_absent {
                "new file mode 100644\n"
            } else if right_absent {
                "deleted file mode 100644\n"
            } else {
                ""
            };
            parse_unified_diff(&format!(
                "diff --git a/{display_text} b/{display_text}\n{metadata}{body}"
            ))
        }
    };
    for file in &mut files {
        file.old_source = if left_absent {
            SourceSpec::None
        } else {
            SourceSpec::File(left.to_path_buf())
        };
        file.new_source = if right_absent {
            SourceSpec::None
        } else {
            SourceSpec::File(right.to_path_buf())
        };
    }

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

fn is_null_path(path: &Path) -> bool {
    path == Path::new("/dev/null") || path == Path::new("NUL")
}

fn binary_file(
    left: &Path,
    right: &Path,
    display_path: &str,
    left_absent: bool,
    right_absent: bool,
) -> DiffFile {
    let previous_path = (!left_absent)
        .then(|| {
            left.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| name != display_path)
        })
        .flatten();
    DiffFile {
        id: stable_file_id(display_path, previous_path.as_deref()),
        path: display_path.into(),
        previous_path,
        summary: None,
        patch: "Binary file contents differ\n".into(),
        hunks: Vec::new(),
        change_kind: if left_absent {
            FileChangeKind::Added
        } else if right_absent {
            FileChangeKind::Deleted
        } else {
            FileChangeKind::Modified
        },
        is_binary: true,
        is_untracked: false,
        is_too_large: false,
        stats_truncated: false,
        language: None,
        stats: FileStats::default(),
        old_source: if left_absent {
            SourceSpec::None
        } else {
            SourceSpec::File(left.to_path_buf())
        },
        new_source: if right_absent {
            SourceSpec::None
        } else {
            SourceSpec::File(right.to_path_buf())
        },
    }
}
