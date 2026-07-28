use ::margem::{
    DiffDocument, DiffFile, DiffHunk, DiffLine, DiffLineKind, DocumentError, FileChange,
    ReviewDocument,
};
use ramo_core::{
    changeset::Changeset,
    diff::model::{FileChangeKind, LineType, MovedLineKind},
};

pub fn build_margem_document(changeset: &Changeset) -> Result<ReviewDocument, DocumentError> {
    let files = changeset
        .files
        .iter()
        .map(|file| DiffFile {
            id: file.id.clone(),
            path: file.path.clone(),
            previous_path: file.previous_path.clone(),
            language: file.language.clone(),
            change: map_file_change(file.change_kind),
            binary: file.is_binary,
            too_large: file.is_too_large,
            untracked: file.is_untracked,
            stats_truncated: file.stats_truncated,
            hunks: file
                .hunks
                .iter()
                .enumerate()
                .map(|(index, hunk)| DiffHunk {
                    id: format!("{}:hunk:{index}", file.id),
                    header: hunk.header.clone(),
                    old_start: hunk.old_start,
                    new_start: hunk.new_start,
                    lines: hunk
                        .lines
                        .iter()
                        .map(|line| DiffLine {
                            kind: map_line_kind(&line.kind),
                            content: line.content.clone(),
                            old_line: line.old_lineno,
                            new_line: line.new_lineno,
                            moved: line.moved.map(map_moved_line).map(str::to_owned),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();

    DiffDocument::new(changeset.title.clone(), files).map(ReviewDocument::from)
}

fn map_line_kind(kind: &LineType) -> DiffLineKind {
    match kind {
        LineType::Context => DiffLineKind::Context,
        LineType::Addition => DiffLineKind::Addition,
        LineType::Deletion => DiffLineKind::Deletion,
    }
}

fn map_file_change(kind: FileChangeKind) -> FileChange {
    match kind {
        FileChangeKind::Modified => FileChange::Modified,
        FileChangeKind::Added => FileChange::Added,
        FileChangeKind::Deleted => FileChange::Deleted,
        FileChangeKind::Renamed => FileChange::Renamed,
        FileChangeKind::Copied => FileChange::Copied,
    }
}

fn map_moved_line(kind: MovedLineKind) -> &'static str {
    match kind {
        MovedLineKind::OldMoved => "old_moved",
        MovedLineKind::OldMovedDimmed => "old_moved_dimmed",
        MovedLineKind::NewMoved => "new_moved",
        MovedLineKind::NewMovedDimmed => "new_moved_dimmed",
    }
}
