use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileStats {
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LineType {
    Context,
    Addition,
    Deletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovedLineKind {
    OldMoved,
    OldMovedDimmed,
    NewMoved,
    NewMovedDimmed,
}

impl LineType {
    pub fn prefix(&self) -> &'static str {
        match self {
            LineType::Addition => "+",
            LineType::Deletion => "-",
            LineType::Context => " ",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineType,
    pub content: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub moved: Option<MovedLineKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SourceSpec {
    None,
    File(PathBuf),
    GitBlob {
        repo_root: PathBuf,
        reference: String,
        path: String,
    },
    GitIndex {
        repo_root: PathBuf,
        path: String,
    },
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub old_start: u32,
    pub new_start: u32,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
}

#[derive(Debug, Clone)]
pub struct DiffFile {
    pub id: String,
    pub path: String,
    pub previous_path: Option<String>,
    pub summary: Option<String>,
    pub patch: String,
    pub hunks: Vec<Hunk>,
    pub change_kind: FileChangeKind,
    pub is_binary: bool,
    pub is_untracked: bool,
    pub is_too_large: bool,
    pub stats_truncated: bool,
    pub language: Option<String>,
    pub stats: FileStats,
    pub old_source: SourceSpec,
    pub new_source: SourceSpec,
}

impl DiffFile {
    pub fn line_counts(&self) -> (usize, usize) {
        (self.stats.additions, self.stats.deletions)
    }
}

#[cfg(test)]
impl DiffFile {
    pub fn for_test(
        path: &str,
        change_kind: FileChangeKind,
        additions: usize,
        deletions: usize,
    ) -> Self {
        let mut lines = Vec::new();
        lines.extend((0..additions).map(|index| DiffLine {
            kind: LineType::Addition,
            content: format!("added {index}"),
            old_lineno: None,
            new_lineno: Some(index as u32 + 1),
            moved: None,
        }));
        lines.extend((0..deletions).map(|index| DiffLine {
            kind: LineType::Deletion,
            content: format!("deleted {index}"),
            old_lineno: Some(index as u32 + 1),
            new_lineno: None,
            moved: None,
        }));
        Self {
            id: crate::core::changeset::stable_file_id(path, None),
            path: path.into(),
            previous_path: None,
            summary: None,
            patch: String::new(),
            hunks: vec![Hunk {
                old_start: 1,
                new_start: 1,
                header: "@@ -1 +1 @@".into(),
                lines,
            }],
            change_kind,
            is_binary: false,
            is_untracked: false,
            is_too_large: false,
            stats_truncated: false,
            language: None,
            stats: FileStats {
                additions,
                deletions,
            },
            old_source: SourceSpec::None,
            new_source: SourceSpec::None,
        }
    }
}
