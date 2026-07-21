use crate::diff::model::{DiffFile, Hunk};
use crate::notes::{
    HumanNote, LiveNote, NoteAnchorSide, NoteTarget, ReviewNote, resolve_note_target,
    stable_note_id,
};
use crate::review::{ReviewController, Viewport};

use super::model::{
    SelectedHunkSummary, SelectedSessionContext, SessionDescriptor, SessionFileSummary,
    SessionLiveCommentSummary, SessionRegistration, SessionRegistrationFile, SessionReview,
    SessionReviewFile, SessionReviewHunk, SessionReviewNoteSummary, SessionSnapshot,
    SessionSnapshotState,
};
use super::protocol::SESSION_REGISTRATION_VERSION;

pub fn build_registration(
    descriptor: &SessionDescriptor,
    files: &[DiffFile],
) -> SessionRegistration {
    SessionRegistration {
        registration_version: SESSION_REGISTRATION_VERSION,
        descriptor: descriptor.clone(),
        files: files.iter().map(registration_file).collect(),
    }
}

pub fn build_snapshot(
    controller: &mut ReviewController,
    viewport: Viewport,
    updated_at: impl Into<String>,
) -> SessionSnapshot {
    let snapshot = controller.snapshot(viewport).clone();
    let selected_file = snapshot
        .selected_file_id
        .as_deref()
        .and_then(|id| controller.files().iter().find(|file| file.id == id))
        .cloned();
    let selected_hunk = selected_file.as_ref().and_then(|file| {
        snapshot
            .selected_hunk_index
            .and_then(|index| file.hunks.get(index).map(|hunk| (index, hunk)))
    });
    let live_comments = controller
        .live_notes()
        .iter()
        .filter_map(|note| live_comment(controller.files(), note))
        .collect::<Vec<_>>();
    let review_notes = project_review_notes(
        controller.files(),
        controller.human_notes(),
        controller.live_notes(),
    );
    let note_markup_width = selected_file
        .as_ref()
        .map(|_| controller.note_markup_width(viewport, NoteAnchorSide::New));
    SessionSnapshot {
        updated_at: updated_at.into(),
        state: SessionSnapshotState {
            selected_file_id: selected_file.as_ref().map(|file| file.id.clone()),
            selected_file_path: selected_file.as_ref().map(|file| file.path.clone()),
            selected_hunk_index: selected_hunk.map_or(0, |(index, _)| index),
            selected_hunk_old_range: selected_hunk.and_then(|(_, hunk)| hunk_range(hunk, true)),
            selected_hunk_new_range: selected_hunk.and_then(|(_, hunk)| hunk_range(hunk, false)),
            show_agent_notes: snapshot.agent_notes,
            note_markup_width,
            live_comment_count: live_comments.len(),
            live_comments,
            review_note_count: review_notes.len(),
            review_notes,
        },
    }
}

pub fn build_session_context(
    registration: &SessionRegistration,
    snapshot: &SessionSnapshot,
) -> SelectedSessionContext {
    let selected_file = snapshot.state.selected_file_id.as_deref().and_then(|id| {
        registration
            .files
            .iter()
            .find(|file| file.id == id)
            .map(|file| file.summary.clone())
    });
    let selected_hunk = snapshot
        .state
        .selected_file_id
        .as_deref()
        .and_then(|id| registration.files.iter().find(|file| file.id == id))
        .and_then(|file| file.hunks.get(snapshot.state.selected_hunk_index))
        .map(|hunk| SelectedHunkSummary {
            index: hunk.index,
            old_range: hunk.old_range,
            new_range: hunk.new_range,
        });
    SelectedSessionContext {
        session_id: registration.descriptor.session_id.clone(),
        title: registration.descriptor.title.clone(),
        source_label: registration.descriptor.source_label.clone(),
        cwd: registration.descriptor.cwd.clone(),
        repo_root: registration.descriptor.repo_root.clone(),
        input_kind: registration.descriptor.input_kind.clone(),
        selected_file,
        selected_hunk,
        show_agent_notes: snapshot.state.show_agent_notes,
        note_markup_width: snapshot.state.note_markup_width,
        live_comment_count: snapshot.state.live_comment_count,
    }
}

pub fn build_session_review(
    registration: &SessionRegistration,
    snapshot: &SessionSnapshot,
    include_patch: bool,
    include_notes: bool,
) -> SessionReview {
    let files = registration
        .files
        .iter()
        .map(|file| review_file(file, include_patch))
        .collect::<Vec<_>>();
    let selected_file = snapshot
        .state
        .selected_file_id
        .as_deref()
        .and_then(|id| files.iter().find(|file| file.id == id))
        .cloned();
    let selected_hunk = selected_file
        .as_ref()
        .and_then(|file| file.hunks.get(snapshot.state.selected_hunk_index))
        .cloned();
    SessionReview {
        session_id: registration.descriptor.session_id.clone(),
        title: registration.descriptor.title.clone(),
        source_label: registration.descriptor.source_label.clone(),
        cwd: registration.descriptor.cwd.clone(),
        repo_root: registration.descriptor.repo_root.clone(),
        input_kind: registration.descriptor.input_kind.clone(),
        selected_file,
        selected_hunk,
        show_agent_notes: snapshot.state.show_agent_notes,
        live_comment_count: snapshot.state.live_comment_count,
        review_note_count: snapshot.state.review_note_count,
        review_notes: include_notes.then(|| snapshot.state.review_notes.clone()),
        files,
    }
}

fn registration_file(file: &DiffFile) -> SessionRegistrationFile {
    SessionRegistrationFile {
        summary: file_summary(file),
        patch: file.patch.clone(),
        hunks: file
            .hunks
            .iter()
            .enumerate()
            .map(|(index, hunk)| review_hunk(index, hunk))
            .collect(),
    }
}

fn review_file(file: &SessionRegistrationFile, include_patch: bool) -> SessionReviewFile {
    SessionReviewFile {
        summary: file.summary.clone(),
        patch: include_patch.then(|| file.patch.clone()),
        hunks: file.hunks.clone(),
    }
}

fn file_summary(file: &DiffFile) -> SessionFileSummary {
    SessionFileSummary {
        id: file.id.clone(),
        path: file.path.clone(),
        previous_path: file.previous_path.clone(),
        additions: file.stats.additions,
        deletions: file.stats.deletions,
        hunk_count: file.hunks.len(),
    }
}

fn review_hunk(index: usize, hunk: &Hunk) -> SessionReviewHunk {
    SessionReviewHunk {
        index,
        header: hunk.header.clone(),
        old_range: hunk_range(hunk, true),
        new_range: hunk_range(hunk, false),
    }
}

fn hunk_range(hunk: &Hunk, old: bool) -> Option<[u32; 2]> {
    let mut lines = hunk.lines.iter().filter_map(|line| {
        if old {
            line.old_lineno
        } else {
            line.new_lineno
        }
    });
    let first = lines.next()?;
    let (start, end) = lines.fold((first, first), |(start, end), line| {
        (start.min(line), end.max(line))
    });
    Some([start, end])
}

fn live_comment(files: &[DiffFile], live: &LiveNote) -> Option<SessionLiveCommentSummary> {
    let file = files.iter().find(|file| file.id == live.target.file_id)?;
    let (side, line) = if let Some(range) = live.target.new_range {
        ("new", range.start)
    } else {
        ("old", live.target.old_range?.start)
    };
    Some(SessionLiveCommentSummary {
        comment_id: live.note.id.clone().unwrap_or_default(),
        file_path: file.path.clone(),
        hunk_index: live.target.hunk_index.unwrap_or(0),
        side: side.into(),
        line,
        summary: live.note.summary.clone(),
        rationale: live.note.rationale.clone(),
        author: live.note.author.clone(),
        created_at: live.note.created_at.clone().unwrap_or_default(),
    })
}

fn project_review_notes(
    files: &[DiffFile],
    human_notes: &[HumanNote],
    live_notes: &[LiveNote],
) -> Vec<SessionReviewNoteSummary> {
    let mut notes = Vec::new();
    for file in files {
        if let Some(agent) = &file.agent {
            notes.extend(agent.annotations.iter().map(|note| {
                let target = resolve_note_target(file, note);
                review_note(
                    file,
                    &target,
                    note,
                    stable_note_id(file, note),
                    note.source.as_str(),
                )
            }));
        }
    }
    notes.extend(human_notes.iter().filter_map(|note| {
        let file = files.iter().find(|file| file.id == note.target.file_id)?;
        Some(SessionReviewNoteSummary {
            note_id: note.id.clone(),
            source: "user".into(),
            file_path: file.path.clone(),
            hunk_index: note.target.hunk_index,
            old_range: note.target.old_range.map(|range| [range.start, range.end]),
            new_range: note.target.new_range.map(|range| [range.start, range.end]),
            body: note.body.clone(),
            title: Some("Your note".into()),
            author: None,
            created_at: note.created_at.clone().unwrap_or_default(),
            updated_at: note.updated_at.clone(),
            editable: true,
        })
    }));
    notes.extend(live_notes.iter().filter_map(|live| {
        let file = files.iter().find(|file| file.id == live.target.file_id)?;
        Some(review_note(
            file,
            &live.target,
            &live.note,
            live.note.id.clone().unwrap_or_default(),
            "agent",
        ))
    }));
    notes
}

fn review_note(
    file: &DiffFile,
    target: &NoteTarget,
    note: &ReviewNote,
    id: String,
    source: &str,
) -> SessionReviewNoteSummary {
    let body = match note.rationale.as_deref() {
        Some(rationale) => format!("{}\n{rationale}", note.summary),
        None => note.summary.clone(),
    };
    SessionReviewNoteSummary {
        note_id: id,
        source: source.into(),
        file_path: file.path.clone(),
        hunk_index: target.hunk_index,
        old_range: target.old_range.map(|range| [range.start, range.end]),
        new_range: target.new_range.map(|range| [range.start, range.end]),
        body,
        title: note.title.clone(),
        author: note.author.clone(),
        created_at: note.created_at.clone().unwrap_or_default(),
        updated_at: note.updated_at.clone(),
        editable: note.editable,
    }
}
