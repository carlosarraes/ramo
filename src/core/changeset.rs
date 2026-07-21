use crate::diff::model::{DiffFile, LineType};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileStats {
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone)]
pub struct Changeset {
    pub id: String,
    pub source_label: String,
    pub title: String,
    pub summary: Option<String>,
    pub agent_summary: Option<String>,
    pub files: Vec<DiffFile>,
}

impl Changeset {
    pub fn new(
        source_label: impl Into<String>,
        title: impl Into<String>,
        files: Vec<DiffFile>,
    ) -> Self {
        let source_label = source_label.into();
        let title = title.into();
        Self {
            id: format!("{source_label}:{title}"),
            source_label,
            title,
            summary: None,
            agent_summary: None,
            files,
        }
    }

    pub fn stats(&self) -> FileStats {
        self.files
            .iter()
            .flat_map(|file| &file.hunks)
            .flat_map(|hunk| &hunk.lines)
            .fold(FileStats::default(), |mut stats, line| {
                match line.kind {
                    LineType::Addition => stats.additions += 1,
                    LineType::Deletion => stats.deletions += 1,
                    LineType::Context => {}
                }
                stats
            })
    }
}

pub fn stable_file_id(path: &str, previous_path: Option<&str>) -> String {
    match previous_path {
        Some(previous) => format!("file:{previous}->{path}"),
        None => format!("file:{path}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::model::{DiffFile, FileChangeKind};

    #[test]
    fn changeset_totals_additions_and_deletions() {
        let file = DiffFile::for_test("src/lib.rs", FileChangeKind::Modified, 3, 2);
        let changeset = Changeset::new("working-tree", "Working tree", vec![file]);
        assert_eq!(
            changeset.stats(),
            FileStats {
                additions: 3,
                deletions: 2,
            }
        );
    }

    #[test]
    fn file_ids_are_stable_across_reloads() {
        assert_eq!(
            stable_file_id("src/lib.rs", None),
            stable_file_id("src/lib.rs", None)
        );
        assert_ne!(
            stable_file_id("src/lib.rs", None),
            stable_file_id("src/lib.rs", Some("src/old.rs")),
        );
    }
}
