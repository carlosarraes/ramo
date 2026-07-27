use std::collections::HashMap;

use crate::diff::model::DiffFile;
use crate::notes::{LineRange, NoteAnchorSide, NoteTarget};
use crate::remote_review::{GithubReviewThread, GithubThreadSubject, RemoteLineSide};

#[derive(Debug, Clone)]
pub(crate) struct PlacedGithubThread {
    pub thread: GithubReviewThread,
    pub target: NoteTarget,
    pub file_level: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct UnplacedGithubThread {
    pub thread: GithubReviewThread,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GithubThreadPlacement {
    pub by_file: HashMap<String, Vec<PlacedGithubThread>>,
    pub unplaced: Vec<UnplacedGithubThread>,
}

pub(crate) fn place_github_threads(
    files: &[DiffFile],
    threads: Vec<GithubReviewThread>,
) -> GithubThreadPlacement {
    let mut placement = GithubThreadPlacement::default();
    for thread in threads {
        match place_thread(files, &thread) {
            Ok((file_id, placed)) => placement.by_file.entry(file_id).or_default().push(placed),
            Err(reason) => placement
                .unplaced
                .push(UnplacedGithubThread { thread, reason }),
        }
    }
    placement
}

fn place_thread(
    files: &[DiffFile],
    thread: &GithubReviewThread,
) -> Result<(String, PlacedGithubThread), String> {
    let file = files
        .iter()
        .find(|file| file.path == thread.path)
        .or_else(|| {
            files
                .iter()
                .find(|file| file.previous_path.as_deref() == Some(thread.path.as_str()))
        })
        .ok_or_else(|| "file is not present in the frozen diff".to_owned())?;
    let (target, file_level) = match &thread.subject {
        GithubThreadSubject::File => (
            NoteTarget {
                file_id: file.id.clone(),
                old_range: None,
                new_range: None,
                hunk_index: None,
                anchor_side: None,
                anchor_line: None,
            },
            true,
        ),
        GithubThreadSubject::Unsupported(subject) => {
            return Err(format!("unsupported GitHub thread subject {subject}"));
        }
        GithubThreadSubject::Line {
            side,
            start_side,
            start_line,
            end_line,
        } => {
            let side = side.ok_or_else(|| "line thread has no supported diff side".to_owned())?;
            if start_side.is_some_and(|start_side| start_side != side) {
                return Err("line thread starts and ends on different diff sides".into());
            }
            let end = end_line.ok_or_else(|| "line thread has no end line".to_owned())?;
            let start = start_line.unwrap_or(end);
            if start > end {
                return Err("line thread range starts after it ends".into());
            }
            let hunk_index = file
                .hunks
                .iter()
                .position(|hunk| {
                    (start..=end).all(|wanted| {
                        hunk.lines.iter().any(|line| match side {
                            RemoteLineSide::Left => line.old_lineno == Some(wanted),
                            RemoteLineSide::Right => line.new_lineno == Some(wanted),
                        })
                    })
                })
                .ok_or_else(|| {
                    let prefix = match side {
                        RemoteLineSide::Left => 'L',
                        RemoteLineSide::Right => 'R',
                    };
                    if start == end {
                        format!("line {prefix}{end} is not present in the frozen diff")
                    } else {
                        format!("range {prefix}{start}-{prefix}{end} is not present in one frozen diff hunk")
                    }
                })?;
            let range = LineRange { start, end };
            let (old_range, new_range, anchor_side) = match side {
                RemoteLineSide::Left => (Some(range), None, NoteAnchorSide::Old),
                RemoteLineSide::Right => (None, Some(range), NoteAnchorSide::New),
            };
            (
                NoteTarget {
                    file_id: file.id.clone(),
                    old_range,
                    new_range,
                    hunk_index: Some(hunk_index),
                    anchor_side: Some(anchor_side),
                    anchor_line: Some(end),
                },
                false,
            )
        }
    };
    Ok((
        file.id.clone(),
        PlacedGithubThread {
            thread: thread.clone(),
            target,
            file_level,
        },
    ))
}
