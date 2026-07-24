use std::collections::{HashSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::{DefaultTerminal, Frame};

use crate::annotations::model::Annotation;
use crate::config::{
    ResolvedConfig, ViewPreferenceChanges, ViewPreferences, save_view_preferences,
};
use crate::diff::model::{DiffFile, DiffLine, LineType};
use crate::process::command::SystemCommandExecutor;
use crate::process::editor::{EditorLauncher, build_editor_command};
use crate::remote_review::{
    PullRequestReviewContext, RemoteReviewComment, RemoteReviewPublisher, RemoteReviewRequest,
    ReviewVerdict,
};
use crate::review::{
    ContextSourceLoader, NativeContextSourceLoader, ReviewAction, ReviewController, ReviewEffect,
    ReviewHit, ReviewOptions, ReviewPoint, SelectionPoint, Viewport,
};
use crate::session::{
    SessionDescriptor, SessionRegistrationClient, SessionSnapshotState, build_registration,
    build_snapshot, session_timestamp,
};
use crate::startup_notice::{RemoteUpdatePoll, RemoteUpdateRuntime};
use crate::terminal::TerminalSession;
use crate::ui::dialogs::{DialogOverlay, ThemeSelection};
use crate::ui::highlight::HighlightCache;
use crate::ui::input::{AppAction, InputMode, map_key_event, map_mouse_event};
use crate::ui::themes::{AppTheme, ThemeRegistry};
use crate::vim::mode::Mode;
use crate::watch::{WatchRuntime, WatchUpdate};

trait TerminalHost {
    fn terminal(&mut self) -> &mut DefaultTerminal;
    fn enable_mouse_capture(&mut self) -> io::Result<()>;
    fn disable_mouse_capture(&mut self) -> io::Result<()>;
    fn suspend(&mut self) -> io::Result<()>;
    fn resume(&mut self) -> io::Result<()>;
    fn suspend_process(&mut self) -> io::Result<()>;
}

struct BorrowedTerminal<'a>(&'a mut DefaultTerminal);

impl TerminalHost for BorrowedTerminal<'_> {
    fn terminal(&mut self) -> &mut DefaultTerminal {
        self.0
    }

    fn enable_mouse_capture(&mut self) -> io::Result<()> {
        execute!(io::stdout(), EnableMouseCapture)
    }

    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        execute!(io::stdout(), DisableMouseCapture)
    }

    fn suspend(&mut self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "terminal suspension requires an owned terminal session",
        ))
    }

    fn resume(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn suspend_process(&mut self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "process suspension requires an owned terminal session",
        ))
    }
}

impl TerminalHost for TerminalSession {
    fn terminal(&mut self) -> &mut DefaultTerminal {
        self.terminal()
    }

    fn enable_mouse_capture(&mut self) -> io::Result<()> {
        self.enable_mouse_capture()
    }

    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        self.disable_mouse_capture()
    }

    fn suspend(&mut self) -> io::Result<()> {
        self.suspend()
    }

    fn resume(&mut self) -> io::Result<()> {
        self.resume()
    }

    fn suspend_process(&mut self) -> io::Result<()> {
        self.suspend_process()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FlatLine {
    pub file_idx: usize,
    pub hunk_idx: usize,
    pub line_idx: usize,
}

pub enum ViewLayout {
    SideBySide,
    Unified,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewMouseDrag {
    Divider,
    Scrollbar,
    Selection { anchor: SelectionPoint, moved: bool },
}

const STARTUP_NOTICE_DURATION: Duration = Duration::from_secs(7);

fn startup_notice_duration() -> Duration {
    #[cfg(debug_assertions)]
    if let Ok(value) = std::env::var("RAMO_TEST_STARTUP_NOTICE_DURATION_MS")
        && let Ok(milliseconds) = value.parse::<u64>()
    {
        return Duration::from_millis(milliseconds.max(1));
    }
    STARTUP_NOTICE_DURATION
}

fn inclusive_drag_selection(
    anchor: SelectionPoint,
    focus: SelectionPoint,
) -> (SelectionPoint, SelectionPoint) {
    if (anchor.row, anchor.cell) <= (focus.row, focus.cell) {
        (
            anchor,
            SelectionPoint::new(focus.row, focus.cell.saturating_add(1)),
        )
    } else {
        (
            SelectionPoint::new(anchor.row, anchor.cell.saturating_add(1)),
            focus,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TmuxSendCompletion {
    Review,
    SaveLegacyAnnotation,
    SaveHumanNote { viewport: Viewport },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteReviewOutcome {
    Published,
    Discarded,
}

pub struct AppRunResult {
    pub annotations: Vec<Annotation>,
    pub remote_outcome: Option<RemoteReviewOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteReturnState {
    Review,
    Verdict,
}

struct RemoteReviewSession {
    context: PullRequestReviewContext,
    service: Box<dyn RemoteReviewPublisher>,
    overall_body: String,
    overall_body_edited: bool,
    overall_edit_original: Option<String>,
    message_title: String,
    message_body: String,
    message_return: RemoteReturnState,
    outcome: Option<RemoteReviewOutcome>,
}

fn format_annotation_for_tmux(annotation: &Annotation) -> String {
    let language = Path::new(&annotation.file)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("diff");
    format!(
        "`{} {}`:\n\n```{}\n{}\n```\n\n{}",
        annotation.file,
        annotation.display_range,
        language,
        annotation.diff_context,
        annotation.comment,
    )
}

fn default_overall_body(count: usize) -> String {
    match count {
        0 => "Review submitted from Ramo.".into(),
        1 => "Review submitted from Ramo with 1 inline comment.".into(),
        count => format!("Review submitted from Ramo with {count} inline comments."),
    }
}

pub struct App {
    pub files: Vec<DiffFile>,
    pub flat_lines: Vec<FlatLine>,
    pub file_starts: Vec<usize>,
    pub line_counts: Vec<(usize, usize)>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub mode: Mode,
    pub focus_side: Side,
    pub annotations: Vec<Annotation>,
    pub layout: ViewLayout,
    pub review_controller: ReviewController,
    pub review_theme: AppTheme,
    pub review_highlights: HighlightCache,
    context_loader: Box<dyn ContextSourceLoader>,
    review_mouse_drag: Option<ReviewMouseDrag>,
    review_selection: Option<(SelectionPoint, SelectionPoint)>,
    review_keyboard_anchor: Option<SelectionPoint>,
    input_mode: InputMode,
    filter_buffer: String,
    theme_registry: ThemeRegistry,
    theme_selection: Option<ThemeSelection>,
    active_theme_id: String,
    transparent_background: bool,
    pager_mode: bool,
    initial_view_preferences: ViewPreferences,
    preference_path: Option<PathBuf>,
    pub should_quit: bool,
    pub comment_buf: String,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub pending_keys: Vec<char>,
    pub comment_selection: Option<(usize, usize)>,
    pub editing_annotation: Option<usize>,
    pub show_file_list: bool,
    pub show_comments: bool,
    pub focus_mode: bool,
    pub toast: Option<String>,
    startup_notice: Option<String>,
    startup_notice_deadline: Option<Instant>,
    startup_notice_queue: VecDeque<String>,
    seen_startup_notices: HashSet<String>,
    remote_update: Option<RemoteUpdateRuntime>,
    pub tmux_panes: Vec<crate::tmux::TmuxPane>,
    pub tmux_cursor: usize,
    pub tmux_last_target: Option<(String, crate::tmux::PasteMode)>,
    pub tmux_pending_text: String,
    tmux_completion: TmuxSendCompletion,
    reload_requested: bool,
    editor_request: Option<(String, Option<u32>)>,
    suspend_requested: bool,
    session_registration: Option<SessionRegistrationClient>,
    session_descriptor: Option<SessionDescriptor>,
    last_session_state: Option<SessionSnapshotState>,
    remote_review: Option<RemoteReviewSession>,
}

impl App {
    pub fn new(files: Vec<DiffFile>) -> Self {
        Self::new_with_config(files, &ResolvedConfig::default(), false)
    }

    pub fn new_with_config(
        files: Vec<DiffFile>,
        config: &ResolvedConfig,
        pager_mode: bool,
    ) -> Self {
        Self::new_with_context_loader(
            files,
            config,
            pager_mode,
            Box::new(NativeContextSourceLoader::default()),
        )
    }

    pub fn new_with_context_loader(
        files: Vec<DiffFile>,
        config: &ResolvedConfig,
        pager_mode: bool,
        context_loader: Box<dyn ContextSourceLoader>,
    ) -> Self {
        Self::new_with_services(files, config, pager_mode, context_loader, None)
    }

    pub fn new_with_preference_path(
        files: Vec<DiffFile>,
        config: &ResolvedConfig,
        pager_mode: bool,
        preference_path: Option<PathBuf>,
    ) -> Self {
        Self::new_with_services(
            files,
            config,
            pager_mode,
            Box::new(NativeContextSourceLoader::default()),
            preference_path,
        )
    }

    pub fn new_with_services(
        files: Vec<DiffFile>,
        config: &ResolvedConfig,
        pager_mode: bool,
        context_loader: Box<dyn ContextSourceLoader>,
        preference_path: Option<PathBuf>,
    ) -> Self {
        let flat_lines = build_flat_lines(&files);
        let file_starts = build_file_starts(&flat_lines);
        let line_counts = files.iter().map(|f| f.line_counts()).collect();
        let review_controller = ReviewController::new(
            files.clone(),
            ReviewOptions {
                layout: config.mode,
                show_sidebar: config.show_sidebar && !pager_mode,
                line_numbers: config.line_numbers,
                wrap_lines: config.wrap_lines,
                hunk_headers: config.hunk_headers,
                agent_notes: config.agent_notes,
                copy_decorations: config.copy_decorations,
                pager_mode,
                annotated_hunks: Vec::new(),
            },
        );
        let theme_registry = ThemeRegistry::new(config.custom_theme.clone());
        let review_theme =
            theme_registry.resolve(&config.theme, None, config.transparent_background);
        let active_theme_id = review_theme.id.clone();
        let mut startup_notices = config.startup_notices.iter().cloned();
        let startup_notice = startup_notices.next();
        let startup_notice_deadline = startup_notice
            .as_ref()
            .map(|_| Instant::now() + startup_notice_duration());
        let startup_notice_queue = startup_notices.collect();
        let seen_startup_notices = config.startup_notices.iter().cloned().collect();
        Self {
            files,
            flat_lines,
            file_starts,
            line_counts,
            cursor: 0,
            scroll_offset: 0,
            mode: Mode::Normal,
            focus_side: Side::Right,
            annotations: Vec::new(),
            layout: ViewLayout::SideBySide,
            review_controller,
            review_theme,
            review_highlights: HighlightCache::default(),
            context_loader,
            review_mouse_drag: None,
            review_selection: None,
            review_keyboard_anchor: None,
            input_mode: InputMode::Normal,
            filter_buffer: String::new(),
            theme_registry,
            theme_selection: None,
            active_theme_id,
            transparent_background: config.transparent_background,
            pager_mode,
            initial_view_preferences: ViewPreferences::from(config),
            preference_path,
            should_quit: false,
            comment_buf: String::new(),
            search_query: String::new(),
            search_matches: Vec::new(),
            pending_keys: Vec::new(),
            comment_selection: None,
            editing_annotation: None,
            show_file_list: true,
            show_comments: false,
            focus_mode: false,
            toast: None,
            startup_notice,
            startup_notice_deadline,
            startup_notice_queue,
            seen_startup_notices,
            remote_update: None,
            tmux_panes: Vec::new(),
            tmux_cursor: 0,
            tmux_last_target: None,
            tmux_pending_text: String::new(),
            tmux_completion: TmuxSendCompletion::Review,
            reload_requested: false,
            editor_request: None,
            suspend_requested: false,
            session_registration: None,
            session_descriptor: None,
            last_session_state: None,
            remote_review: None,
        }
    }

    pub fn attach_pull_request(
        &mut self,
        context: PullRequestReviewContext,
        service: Box<dyn RemoteReviewPublisher>,
    ) {
        self.remote_review = Some(RemoteReviewSession {
            context,
            service,
            overall_body: String::new(),
            overall_body_edited: false,
            overall_edit_original: None,
            message_title: String::new(),
            message_body: String::new(),
            message_return: RemoteReturnState::Review,
            outcome: None,
        });
    }

    pub fn remote_outcome(&self) -> Option<RemoteReviewOutcome> {
        self.remote_review
            .as_ref()
            .and_then(|session| session.outcome)
    }

    pub fn attach_remote_update(&mut self, runtime: RemoteUpdateRuntime) {
        self.remote_update = Some(runtime);
    }

    pub fn attach_session_registration(
        &mut self,
        registration: SessionRegistrationClient,
        descriptor: SessionDescriptor,
        initial_state: SessionSnapshotState,
    ) {
        self.session_registration = Some(registration);
        self.session_descriptor = Some(descriptor);
        self.last_session_state = Some(initial_state);
    }

    pub fn get_line(&self, flat_idx: usize) -> Option<&DiffLine> {
        let fl = self.flat_lines.get(flat_idx)?;
        self.files
            .get(fl.file_idx)?
            .hunks
            .get(fl.hunk_idx)?
            .lines
            .get(fl.line_idx)
    }

    pub fn active_file_idx(&self) -> Option<usize> {
        self.flat_lines.get(self.cursor).map(|fl| fl.file_idx)
    }

    pub fn line_hidden_on_side(&self, line: &DiffLine) -> bool {
        match self.focus_side {
            Side::Left => line.kind == LineType::Addition,
            Side::Right => line.kind == LineType::Deletion,
        }
    }

    fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.min(self.flat_lines.len().saturating_sub(1));
    }

    pub fn rendered_rows_between(&self, from: usize, to: usize) -> usize {
        if self.flat_lines.is_empty() || from >= self.flat_lines.len() {
            return 0;
        }
        let mut rows = 0;
        let mut last_file: Option<usize> = None;
        let mut last_hunk: Option<(usize, usize)> = None;
        let end = to.min(self.flat_lines.len() - 1);

        for (i, fl) in self.flat_lines[from..=end].iter().enumerate() {
            let flat_idx = from + i;

            // In focus mode, skip lines hidden by the renderer
            if self.focus_mode
                && let Some(line) = self.get_line(flat_idx)
                && self.line_hidden_on_side(line)
            {
                continue;
            }

            if last_file != Some(fl.file_idx) {
                if last_file.is_some() {
                    rows += 1; // visual file separator
                }
                rows += 1; // file header
                last_file = Some(fl.file_idx);
                last_hunk = None;
            }
            if last_hunk != Some((fl.file_idx, fl.hunk_idx)) && fl.line_idx == 0 {
                rows += 1;
                last_hunk = Some((fl.file_idx, fl.hunk_idx));
            }
            rows += 1;

            if self.show_comments
                && flat_idx < to
                && let Some(ann) = self
                    .annotations
                    .iter()
                    .find(|a| flat_idx >= a.flat_start && flat_idx <= a.flat_end)
                && flat_idx == ann.flat_end
            {
                rows += ann.comment.lines().count();
            }
        }
        rows
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        match &self.mode {
            Mode::VisualLine { anchor } => {
                let start = (*anchor).min(self.cursor);
                let end = (*anchor).max(self.cursor);
                Some((start, end))
            }
            Mode::CommentInsert | Mode::CommentNormal => self.comment_selection,
            _ => None,
        }
    }

    pub fn run(self, terminal: &mut DefaultTerminal) -> io::Result<Vec<Annotation>> {
        self.run_with_watch(terminal, None)
    }

    pub fn run_with_watch(
        self,
        terminal: &mut DefaultTerminal,
        watch: Option<&mut WatchRuntime>,
    ) -> io::Result<Vec<Annotation>> {
        self.run_loop(&mut BorrowedTerminal(terminal), watch, None)
            .map(|result| result.annotations)
    }

    pub fn run_with_services(
        self,
        terminal: &mut TerminalSession,
        watch: Option<&mut WatchRuntime>,
        editor_base: &Path,
    ) -> io::Result<AppRunResult> {
        self.run_loop(terminal, watch, Some(editor_base))
    }

    fn run_loop(
        mut self,
        terminal: &mut impl TerminalHost,
        mut watch: Option<&mut WatchRuntime>,
        editor_base: Option<&Path>,
    ) -> io::Result<AppRunResult> {
        terminal.enable_mouse_capture()?;
        let run_result = (|| -> io::Result<()> {
            let mut needs_redraw = true;
            while !self.should_quit {
                if needs_redraw {
                    terminal.terminal().draw(|frame| self.draw(frame))?;
                    needs_redraw = false;
                }
                let size = terminal.terminal().size()?;
                let viewport = Viewport {
                    width: size.width,
                    height: size.height,
                };
                self.publish_session_snapshot(viewport);
                needs_redraw |= self.apply_session_requests(viewport, watch.as_deref_mut());
                needs_redraw |= self.poll_startup_notices(Instant::now());
                if event::poll(Duration::from_millis(50))? {
                    match event::read()? {
                        Event::Key(key) => self.handle_key(key, viewport),
                        Event::Mouse(mouse) => self.handle_mouse(mouse, viewport),
                        Event::FocusGained
                        | Event::FocusLost
                        | Event::Paste(_)
                        | Event::Resize(_, _) => {}
                    }
                    needs_redraw = true;
                }
                if std::mem::take(&mut self.reload_requested) {
                    if let Some(runtime) = watch.as_deref_mut() {
                        runtime.manual_reload(Instant::now());
                    } else {
                        self.toast = Some("This input cannot be reloaded".into());
                        needs_redraw = true;
                    }
                }
                if let Some((path, line)) = self.editor_request.take() {
                    let base = watch
                        .as_deref()
                        .map(WatchRuntime::editor_base)
                        .or(editor_base);
                    if let Some(base) = base {
                        self.open_editor(terminal, base, &path, line)?;
                    } else {
                        self.toast = Some(match line {
                            Some(line) => format!("Open {path}:{line}"),
                            None => format!("Open {path}"),
                        });
                    }
                    needs_redraw = true;
                }
                if std::mem::take(&mut self.suspend_requested) {
                    if let Err(error) = terminal.suspend_process() {
                        self.toast = Some(error.to_string());
                    }
                    needs_redraw = true;
                }
                if let Some(runtime) = watch.as_deref_mut() {
                    needs_redraw |= self.apply_watch_update(runtime.poll(Instant::now()), viewport);
                }
            }
            Ok(())
        })();
        let disable_result = terminal.disable_mouse_capture();
        run_result?;
        disable_result?;
        Ok(AppRunResult {
            annotations: self.review_controller.export_annotations(),
            remote_outcome: self.remote_outcome(),
        })
    }

    fn open_editor(
        &mut self,
        terminal: &mut impl TerminalHost,
        base: &Path,
        path: &str,
        line: Option<u32>,
    ) -> io::Result<()> {
        let editor = match std::env::var("EDITOR") {
            Ok(editor) if !editor.trim().is_empty() => editor,
            _ => {
                self.toast = Some("$EDITOR is not set".into());
                return Ok(());
            }
        };
        let path = base.join(path);
        if !path.is_file() {
            self.toast = Some(format!(
                "Cannot edit {}: file does not exist",
                path.display()
            ));
            return Ok(());
        }
        let command = match build_editor_command(&editor, &path, line.unwrap_or(1)) {
            Ok(command) => command,
            Err(error) => {
                self.toast = Some(error.to_string());
                return Ok(());
            }
        };
        let mut launcher = EditorLauncher::new(SystemCommandExecutor);
        let launch_result = if command.suspend_terminal {
            terminal.suspend()?;
            let result = launcher.launch(&command);
            terminal.resume()?;
            result
        } else {
            launcher.launch(&command)
        };
        self.toast = Some(match launch_result {
            Ok(()) => "Editor closed".into(),
            Err(error) => error.to_string(),
        });
        Ok(())
    }

    fn poll_startup_notices(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if self
            .startup_notice_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.startup_notice = None;
            self.startup_notice_deadline = None;
            changed = true;
            if let Some(notice) = self.startup_notice_queue.pop_front() {
                self.startup_notice = Some(notice);
                self.startup_notice_deadline = Some(now + startup_notice_duration());
            }
        }
        let poll = self
            .remote_update
            .as_mut()
            .map(RemoteUpdateRuntime::poll)
            .unwrap_or(RemoteUpdatePoll::Complete);
        match poll {
            RemoteUpdatePoll::Pending => changed,
            RemoteUpdatePoll::Ready(notice) => {
                if !self.seen_startup_notices.insert(notice.clone()) {
                    return changed;
                }
                if self.startup_notice.is_some() {
                    self.startup_notice_queue.push_back(notice);
                    changed
                } else {
                    self.startup_notice = Some(notice);
                    self.startup_notice_deadline = Some(now + startup_notice_duration());
                    true
                }
            }
            RemoteUpdatePoll::Complete => {
                self.remote_update = None;
                changed
            }
        }
    }

    fn apply_watch_update(&mut self, update: WatchUpdate, viewport: Viewport) -> bool {
        match update {
            WatchUpdate::Unchanged => false,
            WatchUpdate::Replaced { files, .. } => {
                self.review_controller.replace_files(files, viewport);
                self.synchronize_review_files();
                if let (Some(client), Some(descriptor)) =
                    (&self.session_registration, &self.session_descriptor)
                {
                    client.publish_registration(build_registration(
                        descriptor,
                        self.review_controller.files(),
                    ));
                }
                self.last_session_state = None;
                self.toast = Some("Reloaded".into());
                true
            }
            WatchUpdate::Empty { .. } => {
                self.toast = Some("No changes; press r to check again".into());
                true
            }
            WatchUpdate::Error { message } => {
                self.toast = Some(format!("Reload failed: {message}"));
                true
            }
        }
    }

    fn synchronize_review_files(&mut self) {
        self.files = self.review_controller.files().to_vec();
        self.flat_lines = build_flat_lines(&self.files);
        self.file_starts = build_file_starts(&self.flat_lines);
        self.line_counts = self.files.iter().map(DiffFile::line_counts).collect();
        self.clamp_cursor();
        self.review_selection = None;
        self.review_keyboard_anchor = None;
        self.context_loader.invalidate();
    }

    fn publish_session_snapshot(&mut self, viewport: Viewport) {
        let Some(client) = &self.session_registration else {
            return;
        };
        let snapshot = build_snapshot(&mut self.review_controller, viewport, session_timestamp());
        if self.last_session_state.as_ref() == Some(&snapshot.state) {
            return;
        }
        self.last_session_state = Some(snapshot.state.clone());
        client.publish_snapshot(snapshot);
    }

    fn apply_session_requests(
        &mut self,
        viewport: Viewport,
        mut watch: Option<&mut WatchRuntime>,
    ) -> bool {
        let mut changed = false;
        for _ in 0..16 {
            let request = self
                .session_registration
                .as_ref()
                .and_then(SessionRegistrationClient::try_recv_request);
            let Some(request) = request else {
                break;
            };
            let result = if request
                .input
                .get("action")
                .and_then(serde_json::Value::as_str)
                == Some("reload")
            {
                self.apply_live_session_reload(&request.input, watch.as_deref_mut(), viewport)
            } else {
                crate::session::apply_session_request(
                    &mut self.review_controller,
                    &request.request_id,
                    &request.input,
                    viewport,
                )
            };
            let snapshot =
                build_snapshot(&mut self.review_controller, viewport, session_timestamp());
            self.last_session_state = Some(snapshot.state.clone());
            if let Some(client) = &self.session_registration {
                let _ = client.respond(request.request_id, result, snapshot);
            }
            changed = true;
        }
        changed
    }

    fn apply_live_session_reload(
        &mut self,
        input: &serde_json::Value,
        runtime: Option<&mut WatchRuntime>,
        viewport: Viewport,
    ) -> Result<serde_json::Value, String> {
        let runtime = runtime.ok_or_else(|| {
            "session reload requires the initial ramo session to be rooted in a repository"
                .to_owned()
        })?;
        let applied = crate::session::apply_session_reload(
            &mut self.review_controller,
            runtime,
            input,
            viewport,
        )?;
        self.synchronize_review_files();
        if let Some(descriptor) = self.session_descriptor.as_mut() {
            *descriptor = crate::session::refresh_session_descriptor(
                descriptor,
                &applied.input,
                &applied.loaded,
                &applied.cwd,
            );
        }
        if let (Some(client), Some(descriptor)) =
            (&self.session_registration, &self.session_descriptor)
        {
            client.publish_registration(build_registration(
                descriptor,
                self.review_controller.files(),
            ));
        }
        self.last_session_state = None;
        self.toast = Some("Reloaded live session".into());
        let selected = self.review_controller.snapshot(viewport).clone();
        let selected_file_path = selected.selected_file_id.as_deref().and_then(|id| {
            self.review_controller
                .files()
                .iter()
                .find(|file| file.id == id)
                .map(|file| file.path.clone())
        });
        Ok(serde_json::json!({
            "sessionId":self.session_descriptor.as_ref().map(|descriptor| descriptor.session_id.as_str()),
            "inputKind":self.session_descriptor.as_ref().map(|descriptor| descriptor.input_kind.as_str()),
            "title":applied.loaded.changeset.title,
            "sourceLabel":applied.loaded.changeset.source_label,
            "fileCount":self.review_controller.files().len(),
            "selectedFilePath":selected_file_path,
            "selectedHunkIndex":selected.selected_hunk_index.unwrap_or(0),
        }))
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(
            crate::ui::review::ReviewWidget::new(
                &mut self.review_controller,
                &self.review_theme,
                &mut self.review_highlights,
            )
            .selection(self.review_selection),
            area,
        );
        if self.input_mode == InputMode::Filter || !self.filter_buffer.is_empty() {
            let status = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
            frame.render_widget(
                Paragraph::new(format!(" Filter: {}", self.filter_buffer)).style(
                    Style::default()
                        .fg(self.review_theme.text)
                        .bg(self.review_theme.panel_alt),
                ),
                status,
            );
        } else if let Some(toast) = &self.toast {
            let status = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
            frame.render_widget(
                Paragraph::new(format!(" {toast}")).style(
                    Style::default()
                        .fg(self.review_theme.text)
                        .bg(self.review_theme.panel_alt),
                ),
                status,
            );
        } else if let Some(notice) = &self.startup_notice {
            let status = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
            frame.render_widget(
                Paragraph::new(format!(" {notice}")).style(
                    Style::default()
                        .fg(self.review_theme.text)
                        .bg(self.review_theme.panel_alt),
                ),
                status,
            );
        } else if let Some(session) = &self.remote_review {
            let status = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
            frame.render_widget(
                Paragraph::new(format!(" {}", session.context.status_label())).style(
                    Style::default()
                        .fg(self.review_theme.text)
                        .bg(self.review_theme.panel_alt),
                ),
                status,
            );
        }
        match self.input_mode {
            InputMode::Help => {
                frame.render_widget(DialogOverlay::help(&self.review_theme, true), area);
            }
            InputMode::AgentSkill => {
                frame.render_widget(DialogOverlay::agent_skill(&self.review_theme), area);
            }
            InputMode::Theme => {
                if let Some(selection) = &self.theme_selection {
                    frame.render_widget(
                        DialogOverlay::theme(
                            &self.review_theme,
                            selection.ids(),
                            selection.selected(),
                        ),
                        area,
                    );
                }
            }
            InputMode::Note => {
                if area.width < 48 {
                    frame.render_widget(
                        DialogOverlay::note(&self.review_theme, &self.comment_buf),
                        area,
                    );
                }
            }
            InputMode::SavePrompt => {
                frame.render_widget(DialogOverlay::save(&self.review_theme), area);
            }
            InputMode::PublishPrompt => {
                if let Some(session) = &self.remote_review {
                    frame.render_widget(
                        DialogOverlay::publish(
                            &self.review_theme,
                            session.context.number,
                            self.review_controller.human_notes().len(),
                        ),
                        area,
                    );
                }
            }
            InputMode::VerdictPrompt => {
                if let Some(session) = &self.remote_review {
                    frame.render_widget(
                        DialogOverlay::verdict(
                            &self.review_theme,
                            session.context.is_self_authored(),
                            &session.overall_body,
                        ),
                        area,
                    );
                }
            }
            InputMode::OverallComment => {
                frame.render_widget(
                    DialogOverlay::overall_comment(&self.review_theme, &self.comment_buf),
                    area,
                );
            }
            InputMode::Message => {
                if let Some(session) = &self.remote_review {
                    frame.render_widget(
                        DialogOverlay::message(
                            &self.review_theme,
                            &session.message_title,
                            &session.message_body,
                        ),
                        area,
                    );
                }
            }
            InputMode::Normal | InputMode::Filter => {}
        }
        if matches!(self.mode, Mode::TmuxPanePick) {
            frame.render_widget(
                DialogOverlay::tmux(&self.review_theme, &self.tmux_panes, self.tmux_cursor),
                area,
            );
        }
    }

    fn handle_key(&mut self, key: KeyEvent, viewport: Viewport) {
        if self.input_mode != InputMode::Normal {
            self.handle_ui_key(key, viewport);
            return;
        }
        match &self.mode {
            Mode::CommentInsert => self.handle_comment_insert_key(key),
            Mode::CommentNormal => self.handle_comment_normal_key(key),
            Mode::Command => self.handle_command_key(key),
            Mode::TmuxPanePick => self.handle_tmux_pick_key(key),
            Mode::VisualLine { .. } => {
                self.handle_nav_key(key, usize::from(viewport.height));
            }
            _ => self.handle_ui_key(key, viewport),
        }
    }

    pub fn input_mode(&self) -> InputMode {
        self.input_mode
    }

    pub fn handle_ui_key(&mut self, key: KeyEvent, viewport: Viewport) {
        if let Some(action) = map_key_event(key, self.input_mode, self.pager_mode) {
            self.apply_app_action(action, viewport);
        }
    }

    pub fn handle_mouse(&mut self, event: MouseEvent, viewport: Viewport) {
        if self.input_mode != InputMode::Normal {
            return;
        }
        if let Some(action) = map_mouse_event(event) {
            self.apply_app_action(action, viewport);
            return;
        }
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let point = ReviewPoint::new(event.column, event.row);
                match self.review_controller.hit_test(point, viewport) {
                    Some(ReviewHit::SidebarFile(file_id)) => {
                        self.review_controller
                            .apply(ReviewAction::SelectFile(file_id), viewport);
                    }
                    Some(ReviewHit::SidebarDivider) => {
                        self.review_mouse_drag = Some(ReviewMouseDrag::Divider);
                    }
                    Some(ReviewHit::Scrollbar) => {
                        self.review_mouse_drag = Some(ReviewMouseDrag::Scrollbar);
                        self.review_controller
                            .scroll_to_mouse_row(event.row, viewport);
                    }
                    Some(ReviewHit::Collapsed(gap)) => {
                        self.toast = self
                            .review_controller
                            .toggle_context_gap(&gap, self.context_loader.as_mut(), viewport)
                            .err()
                            .map(|failure| failure.to_string());
                    }
                    Some(ReviewHit::Note(id)) => {
                        if self.review_controller.edit_human_note(&id, viewport) {
                            self.comment_buf = self
                                .review_controller
                                .human_note_draft()
                                .map_or_else(String::new, |draft| draft.body.clone());
                            self.input_mode = InputMode::Note;
                        }
                    }
                    Some(ReviewHit::Diff(anchor)) => {
                        self.review_keyboard_anchor = None;
                        self.review_mouse_drag = Some(ReviewMouseDrag::Selection {
                            anchor,
                            moved: false,
                        });
                        self.review_selection = None;
                    }
                    None => {}
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => match self.review_mouse_drag {
                Some(ReviewMouseDrag::Divider) => {
                    self.review_controller
                        .resize_sidebar(event.column, viewport);
                }
                Some(ReviewMouseDrag::Scrollbar) => {
                    self.review_controller
                        .scroll_to_mouse_row(event.row, viewport);
                }
                Some(ReviewMouseDrag::Selection { anchor, .. }) => {
                    if let Some(ReviewHit::Diff(focus)) = self
                        .review_controller
                        .hit_test(ReviewPoint::new(event.column, event.row), viewport)
                    {
                        self.review_selection = Some(inclusive_drag_selection(anchor, focus));
                        self.review_mouse_drag = Some(ReviewMouseDrag::Selection {
                            anchor,
                            moved: true,
                        });
                    }
                }
                None => {}
            },
            MouseEventKind::Up(MouseButton::Left) => {
                if matches!(
                    self.review_mouse_drag,
                    Some(ReviewMouseDrag::Selection { moved: true, .. })
                ) {
                    self.copy_review_selection(viewport);
                } else if matches!(
                    self.review_mouse_drag,
                    Some(ReviewMouseDrag::Selection { moved: false, .. })
                ) {
                    self.review_selection = None;
                }
                self.review_mouse_drag = None;
            }
            _ => {}
        }
    }

    fn copy_review_selection(&mut self, viewport: Viewport) {
        let Some((anchor, focus)) = self.review_selection else {
            return;
        };
        let text = self
            .review_controller
            .selection_text(anchor, focus, viewport);
        if text.is_empty() {
            self.toast = Some("nothing to copy".into());
            return;
        }
        self.toast = Some(match crate::clipboard::copy_to_clipboard(&text) {
            Ok(()) => "copied selection".into(),
            Err(error) => format!("copy failed: {error}"),
        });
    }

    fn apply_app_action(&mut self, action: AppAction, viewport: Viewport) {
        match action {
            AppAction::Review(action) => {
                let effect = self.review_controller.apply(action, viewport);
                self.apply_review_effect(effect, viewport);
                if let Some(anchor) = self.review_keyboard_anchor
                    && let Some((_, focus)) = self.review_controller.selected_line_range(viewport)
                {
                    self.review_selection = Some((anchor, focus));
                }
            }
            AppAction::Insert(character) => match self.input_mode {
                InputMode::Filter => {
                    self.filter_buffer.push(character);
                    self.review_controller.apply(
                        ReviewAction::SetFilter(self.filter_buffer.clone()),
                        viewport,
                    );
                }
                InputMode::Note => {
                    self.comment_buf.push(character);
                    self.review_controller
                        .update_human_note_draft(&self.comment_buf, viewport);
                }
                InputMode::OverallComment => self.comment_buf.push(character),
                _ => {}
            },
            AppAction::Backspace => match self.input_mode {
                InputMode::Filter => {
                    self.filter_buffer.pop();
                    self.review_controller.apply(
                        ReviewAction::SetFilter(self.filter_buffer.clone()),
                        viewport,
                    );
                }
                InputMode::Note => {
                    self.comment_buf.pop();
                    self.review_controller
                        .update_human_note_draft(&self.comment_buf, viewport);
                }
                InputMode::OverallComment => {
                    self.comment_buf.pop();
                }
                _ => {}
            },
            AppAction::Cancel => self.cancel_input(viewport),
            AppAction::Confirm => self.confirm_input(viewport),
            AppAction::MoveChoice(delta) => {
                if let Some(selection) = &mut self.theme_selection {
                    selection.move_by(delta);
                    self.review_theme = self.theme_registry.resolve(
                        selection.preview_id(),
                        None,
                        self.transparent_background,
                    );
                }
            }
            AppAction::ToggleFocus => {
                self.input_mode = if self.input_mode == InputMode::Filter {
                    InputMode::Normal
                } else {
                    InputMode::Filter
                };
            }
            AppAction::ToggleContext => {
                if self.remote_review.is_some() {
                    self.show_remote_message(
                        "Unavailable for pull request",
                        "Unchanged local source is unavailable for pull request snapshots.",
                        RemoteReturnState::Review,
                    );
                    return;
                }
                self.toast = self
                    .review_controller
                    .toggle_context(self.context_loader.as_mut(), viewport)
                    .err()
                    .map(|failure| failure.to_string());
            }
            AppAction::BeginSelection => {
                if let Some((anchor, focus)) = self.review_controller.selected_line_range(viewport)
                {
                    self.review_keyboard_anchor = Some(anchor);
                    self.review_selection = Some((anchor, focus));
                }
            }
            AppAction::YankSelection => {
                if self.review_selection.is_none() {
                    self.review_selection = self.review_controller.selected_line_range(viewport);
                }
                self.copy_review_selection(viewport);
                self.review_keyboard_anchor = None;
                self.review_selection = None;
            }
            AppAction::SendSelection { reset_target } => {
                if reset_target {
                    self.tmux_last_target = None;
                }
                let selection = self
                    .review_selection
                    .or_else(|| self.review_controller.selected_line_range(viewport));
                if let Some((anchor, focus)) = selection {
                    let text = self
                        .review_controller
                        .selection_text(anchor, focus, viewport);
                    self.request_tmux_send(text, TmuxSendCompletion::Review);
                }
            }
            AppAction::SendNote { reset_target } => {
                if reset_target {
                    self.tmux_last_target = None;
                }
                self.review_controller
                    .update_human_note_draft(&self.comment_buf, viewport);
                if let Some(annotation) = self.review_controller.human_note_draft_annotation() {
                    self.request_tmux_send(
                        format_annotation_for_tmux(&annotation),
                        TmuxSendCompletion::SaveHumanNote { viewport },
                    );
                }
            }
            AppAction::Suspend => self.suspend_requested = true,
            AppAction::OpenAgentSkill => self.input_mode = InputMode::AgentSkill,
            AppAction::CopyAgentSkill => {
                self.toast = Some(
                    match crate::clipboard::copy_to_clipboard(
                        crate::ui::dialogs::AGENT_SKILL_PROMPT,
                    ) {
                        Ok(()) => "copied agent skill guidance".into(),
                        Err(error) => format!("copy failed: {error}"),
                    },
                );
            }
            AppAction::DisableSavePrompt => {
                let mut current = self.current_view_preferences();
                current.prompt_save_view_preferences = false;
                self.save_and_quit(current);
            }
            AppAction::Discard => {
                self.input_mode = InputMode::Normal;
                self.should_quit = true;
            }
            AppAction::ConfirmPublish => self.confirm_remote_publish(),
            AppAction::KeepReviewing => {
                self.input_mode = InputMode::Normal;
            }
            AppAction::DiscardRemoteReview => {
                if let Some(session) = &mut self.remote_review {
                    session.outcome = Some(RemoteReviewOutcome::Discarded);
                }
                self.input_mode = InputMode::Normal;
                self.should_quit = true;
            }
            AppAction::ChooseVerdict(verdict) => self.submit_remote_review(verdict, viewport),
            AppAction::EditOverallComment => {
                if let Some(session) = &mut self.remote_review {
                    session.overall_edit_original = Some(session.overall_body.clone());
                    self.comment_buf.clone_from(&session.overall_body);
                    self.input_mode = InputMode::OverallComment;
                }
            }
            AppAction::SaveOverallComment => {
                if let Some(session) = &mut self.remote_review {
                    session.overall_body.clone_from(&self.comment_buf);
                    session.overall_body_edited = true;
                    session.overall_edit_original = None;
                }
                self.comment_buf.clear();
                self.input_mode = InputMode::VerdictPrompt;
            }
            AppAction::DismissMessage => self.dismiss_remote_message(),
        }
    }

    fn apply_review_effect(&mut self, effect: ReviewEffect, viewport: Viewport) {
        match effect {
            ReviewEffect::FocusFilter => self.input_mode = InputMode::Filter,
            ReviewEffect::OpenHelp => self.input_mode = InputMode::Help,
            ReviewEffect::OpenThemeSelector => {
                self.theme_selection = Some(ThemeSelection::new(
                    self.theme_registry.selector_items(),
                    &self.active_theme_id,
                ));
                self.input_mode = InputMode::Theme;
            }
            ReviewEffect::StartNote => {
                if self.remote_review.is_some() {
                    match self
                        .review_controller
                        .begin_remote_human_note(self.review_selection, viewport)
                    {
                        Ok(Some(_)) => {
                            self.comment_buf.clear();
                            self.input_mode = InputMode::Note;
                        }
                        Ok(None) => {}
                        Err(error) => self.show_remote_message(
                            "Cannot publish this selection",
                            &error.to_string(),
                            RemoteReturnState::Review,
                        ),
                    }
                } else if self
                    .review_controller
                    .begin_human_note(self.review_selection, viewport)
                    .is_some()
                {
                    self.comment_buf.clear();
                    self.input_mode = InputMode::Note;
                }
            }
            ReviewEffect::EditFile { path, line } => {
                if self.remote_review.is_some() {
                    self.show_remote_message(
                        "Unavailable for pull request",
                        "The local checkout may not match this pull request snapshot.",
                        RemoteReturnState::Review,
                    );
                } else {
                    self.editor_request = Some((path, line));
                }
            }
            ReviewEffect::Reload => {
                if let Some(number) = self
                    .remote_review
                    .as_ref()
                    .map(|session| session.context.number)
                {
                    self.show_remote_message(
                        "Frozen pull request snapshot",
                        &format!(
                            "Pull request snapshots cannot reload. Reopen `ramo pr {number}`."
                        ),
                        RemoteReturnState::Review,
                    );
                } else {
                    self.reload_requested = true;
                }
            }
            ReviewEffect::Quit => self.request_quit(viewport),
            ReviewEffect::None | ReviewEffect::Redraw => {}
        }
    }

    fn clear_review_selection(&mut self) {
        self.review_keyboard_anchor = None;
        self.review_selection = None;
    }

    fn current_view_preferences(&self) -> ViewPreferences {
        let review = self.review_controller.view_preferences();
        ViewPreferences {
            mode: review.layout,
            theme: self.active_theme_id.clone(),
            show_sidebar: review.show_sidebar,
            line_numbers: review.line_numbers,
            wrap_lines: review.wrap_lines,
            hunk_headers: review.hunk_headers,
            agent_notes: review.agent_notes,
            transparent_background: self.transparent_background,
            prompt_save_view_preferences: self
                .initial_view_preferences
                .prompt_save_view_preferences,
        }
    }

    fn request_quit(&mut self, _viewport: Viewport) {
        if self
            .remote_review
            .as_ref()
            .is_some_and(|session| session.outcome.is_none())
        {
            self.input_mode = InputMode::PublishPrompt;
            return;
        }
        self.request_local_quit();
    }

    fn request_local_quit(&mut self) {
        let changes = ViewPreferenceChanges::between(
            &self.initial_view_preferences,
            &self.current_view_preferences(),
        );
        if self.pager_mode
            || self.preference_path.is_none()
            || !self.initial_view_preferences.prompt_save_view_preferences
            || changes.is_empty()
        {
            self.should_quit = true;
        } else {
            self.input_mode = InputMode::SavePrompt;
        }
    }

    fn confirm_remote_publish(&mut self) {
        let count = self.review_controller.human_notes().len();
        if let Some(session) = &mut self.remote_review {
            if !session.overall_body_edited {
                session.overall_body = default_overall_body(count);
            }
            self.input_mode = InputMode::VerdictPrompt;
        }
    }

    fn submit_remote_review(&mut self, verdict: ReviewVerdict, _viewport: Viewport) {
        let Some(session) = self.remote_review.as_ref() else {
            return;
        };
        if session.context.is_self_authored() && verdict != ReviewVerdict::Comment {
            self.show_remote_message(
                "Comment only",
                "GitHub does not allow approving or requesting changes on your own pull request.",
                RemoteReturnState::Verdict,
            );
            return;
        }
        let mut comments = Vec::with_capacity(self.review_controller.human_notes().len());
        for note in self.review_controller.human_notes() {
            let Some(target) = note.remote_target.clone() else {
                self.show_remote_message(
                    "Cannot publish review",
                    "A local note has no GitHub line target. Remove it or reopen the pull request.",
                    RemoteReturnState::Verdict,
                );
                return;
            };
            comments.push(RemoteReviewComment {
                target,
                body: note.body.clone(),
            });
        }
        let context = session.context.clone();
        let request = RemoteReviewRequest {
            commit_id: context.captured_revision.clone(),
            body: session.overall_body.clone(),
            verdict,
            comments,
        };
        let current_revision = {
            let session = self.remote_review.as_mut().expect("checked above");
            session.service.current_revision(&context)
        };
        let current_revision = match current_revision {
            Ok(revision) => revision,
            Err(error) => {
                self.show_remote_message(
                    "GitHub review failed",
                    &error.to_string(),
                    RemoteReturnState::Verdict,
                );
                return;
            }
        };
        if current_revision != context.captured_revision {
            self.show_remote_message(
                "Pull request changed",
                &format!(
                    "PR #{} changed while you were reviewing it. Reopen `ramo pr {}` before publishing.",
                    context.number, context.number
                ),
                RemoteReturnState::Verdict,
            );
            return;
        }
        let submit = {
            let session = self.remote_review.as_mut().expect("checked above");
            session.service.submit_review(&context, &request)
        };
        if let Err(error) = submit {
            self.show_remote_message(
                "GitHub review failed",
                &error.to_string(),
                RemoteReturnState::Verdict,
            );
            return;
        }
        if let Some(session) = &mut self.remote_review {
            session.outcome = Some(RemoteReviewOutcome::Published);
        }
        self.input_mode = InputMode::Normal;
        self.request_local_quit();
    }

    fn show_remote_message(&mut self, title: &str, body: &str, return_to: RemoteReturnState) {
        if let Some(session) = &mut self.remote_review {
            session.message_title = title.to_owned();
            session.message_body = body.to_owned();
            session.message_return = return_to;
            self.input_mode = InputMode::Message;
        }
    }

    fn dismiss_remote_message(&mut self) {
        self.input_mode = self
            .remote_review
            .as_ref()
            .map_or(InputMode::Normal, |session| match session.message_return {
                RemoteReturnState::Review => InputMode::Normal,
                RemoteReturnState::Verdict => InputMode::VerdictPrompt,
            });
    }

    fn save_and_quit(&mut self, current: ViewPreferences) {
        let Some(path) = self.preference_path.as_deref() else {
            self.should_quit = true;
            self.input_mode = InputMode::Normal;
            return;
        };
        let changes = ViewPreferenceChanges::between(&self.initial_view_preferences, &current);
        match save_view_preferences(path, &changes) {
            Ok(()) => {
                self.should_quit = true;
                self.input_mode = InputMode::Normal;
            }
            Err(error) => {
                self.should_quit = false;
                self.input_mode = InputMode::SavePrompt;
                self.toast = Some(error.to_string());
            }
        }
    }

    fn cancel_input(&mut self, viewport: Viewport) {
        match self.input_mode {
            InputMode::Filter if !self.filter_buffer.is_empty() => {
                self.filter_buffer.clear();
                self.review_controller
                    .apply(ReviewAction::SetFilter(String::new()), viewport);
                self.input_mode = InputMode::Normal;
            }
            InputMode::Theme => {
                if let Some(selection) = &self.theme_selection {
                    self.review_theme = self.theme_registry.resolve(
                        selection.cancel_id(),
                        None,
                        self.transparent_background,
                    );
                }
                self.theme_selection = None;
                self.input_mode = InputMode::Normal;
            }
            InputMode::Note => {
                self.review_controller.cancel_human_note_draft(viewport);
                self.comment_buf.clear();
                self.input_mode = InputMode::Normal;
                self.clear_review_selection();
            }
            InputMode::Help | InputMode::AgentSkill | InputMode::Filter | InputMode::SavePrompt => {
                self.input_mode = InputMode::Normal;
            }
            InputMode::PublishPrompt | InputMode::VerdictPrompt => {
                self.input_mode = InputMode::Normal;
            }
            InputMode::OverallComment => {
                if let Some(session) = &mut self.remote_review
                    && let Some(original) = session.overall_edit_original.take()
                {
                    session.overall_body = original;
                }
                self.comment_buf.clear();
                self.input_mode = InputMode::VerdictPrompt;
            }
            InputMode::Message => self.dismiss_remote_message(),
            InputMode::Normal => {
                self.review_keyboard_anchor = None;
                self.review_selection = None;
            }
        }
    }

    fn confirm_input(&mut self, viewport: Viewport) {
        match self.input_mode {
            InputMode::Theme => {
                if let Some(selection) = &self.theme_selection {
                    self.active_theme_id = selection.confirm_id().to_owned();
                }
                self.theme_selection = None;
                self.input_mode = InputMode::Normal;
            }
            InputMode::Note => {
                self.review_controller
                    .update_human_note_draft(&self.comment_buf, viewport);
                self.review_controller.save_human_note_draft(viewport);
                self.comment_buf.clear();
                self.input_mode = InputMode::Normal;
                self.clear_review_selection();
            }
            InputMode::SavePrompt => {
                self.save_and_quit(self.current_view_preferences());
            }
            _ => {}
        }
    }

    fn handle_nav_key(&mut self, key: KeyEvent, viewport_height: usize) {
        let half_page = viewport_height / 2;
        self.toast = None;

        if !self.pending_keys.is_empty() {
            if let KeyCode::Char(c) = key.code {
                let first = self.pending_keys[0];
                self.pending_keys.clear();
                match (first, c) {
                    ('g', 'g') => self.cursor = 0,
                    ('y', 'y') => self.perform_yank(self.cursor, self.cursor),
                    _ => {}
                }
            } else {
                self.pending_keys.clear();
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => {
                let last = self.flat_lines.len().saturating_sub(1);
                self.cursor = self
                    .next_interesting_line(self.cursor, true)
                    .unwrap_or_else(|| (self.cursor + 1).min(last));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.cursor = self
                    .next_interesting_line(self.cursor, false)
                    .unwrap_or_else(|| self.cursor.saturating_sub(1));
            }
            KeyCode::Char('h') => {
                self.focus_side = Side::Left;
                self.snap_cursor_to_visible_line();
            }
            KeyCode::Char('l') => {
                self.focus_side = Side::Right;
                self.snap_cursor_to_visible_line();
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor =
                    (self.cursor + half_page).min(self.flat_lines.len().saturating_sub(1));
                self.center_scroll(viewport_height);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor = self.cursor.saturating_sub(half_page);
                self.center_scroll(viewport_height);
            }
            KeyCode::Char('G') => {
                self.cursor = self.flat_lines.len().saturating_sub(1);
            }
            KeyCode::Char('L') if matches!(self.mode, Mode::Normal) => {
                self.jump_next_file();
                self.center_scroll(viewport_height);
            }
            KeyCode::Char('H') if matches!(self.mode, Mode::Normal) => {
                self.jump_prev_file();
                self.center_scroll(viewport_height);
            }
            KeyCode::Char('e') if matches!(self.mode, Mode::Normal) => {
                self.show_file_list = !self.show_file_list;
            }
            KeyCode::Char('F') if matches!(self.mode, Mode::Normal) => {
                self.focus_mode = !self.focus_mode;
                if self.focus_mode {
                    self.snap_cursor_to_visible_line();
                }
            }
            KeyCode::Char('g') => {
                self.pending_keys.push('g');
            }
            KeyCode::Char(']') => self.jump_next_hunk(),
            KeyCode::Char('[') => self.jump_prev_hunk(),
            KeyCode::Char('V') if matches!(self.mode, Mode::Normal) => {
                self.mode = Mode::VisualLine {
                    anchor: self.cursor,
                };
            }
            KeyCode::Char('y') if matches!(self.mode, Mode::VisualLine { .. }) => {
                if let Some((start, end)) = self.selection_range() {
                    self.perform_yank(start, end);
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Char('y') if matches!(self.mode, Mode::Normal) => {
                self.pending_keys.push('y');
            }
            KeyCode::Char('t') | KeyCode::Char('T')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.modifiers.contains(KeyModifiers::SHIFT)
                    && matches!(self.mode, Mode::Normal) =>
            {
                self.tmux_last_target = None;
                let text = self.yank_text(self.cursor, self.cursor);
                self.request_tmux_send(text, TmuxSendCompletion::Review);
            }
            KeyCode::Char('t')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(self.mode, Mode::Normal) =>
            {
                let text = self.yank_text(self.cursor, self.cursor);
                self.request_tmux_send(text, TmuxSendCompletion::Review);
            }
            KeyCode::Char('t')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(self.mode, Mode::VisualLine { .. }) =>
            {
                if let Some((start, end)) = self.selection_range() {
                    let text = self.yank_text(start, end);
                    self.request_tmux_send(text, TmuxSendCompletion::Review);
                }
            }
            KeyCode::Char('c') if matches!(self.mode, Mode::Normal | Mode::VisualLine { .. }) => {
                let range = if matches!(self.mode, Mode::VisualLine { .. }) {
                    self.selection_range().unwrap_or((self.cursor, self.cursor))
                } else {
                    (self.cursor, self.cursor)
                };

                // Check if an existing annotation covers this range
                // Only edit if the selection exactly matches an existing annotation
                let existing = self
                    .annotations
                    .iter()
                    .position(|ann| ann.flat_start == range.0 && ann.flat_end == range.1);
                if let Some(idx) = existing {
                    self.comment_buf = self.annotations[idx].comment.clone();
                    self.comment_selection = Some((
                        self.annotations[idx].flat_start,
                        self.annotations[idx].flat_end,
                    ));
                    self.editing_annotation = Some(idx);
                } else {
                    self.comment_buf.clear();
                    self.comment_selection = Some(range);
                    self.editing_annotation = None;
                }
                self.mode = Mode::CommentInsert;
            }
            KeyCode::Char('E') if matches!(self.mode, Mode::Normal) => {
                self.show_comments = !self.show_comments;
            }
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Char('/') if matches!(self.mode, Mode::Normal) => {
                self.mode = Mode::Command;
                self.search_query.clear();
            }
            KeyCode::Char('n') if matches!(self.mode, Mode::Normal) => {
                self.jump_next_search_match();
                self.center_scroll(viewport_height);
            }
            KeyCode::Char('N') if matches!(self.mode, Mode::Normal) => {
                self.jump_prev_search_match();
                self.center_scroll(viewport_height);
            }
            KeyCode::Tab => {
                self.layout = match self.layout {
                    ViewLayout::SideBySide => ViewLayout::Unified,
                    ViewLayout::Unified => ViewLayout::SideBySide,
                };
            }
            _ => {}
        }
        self.clamp_cursor();
    }

    fn handle_comment_insert_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if self.comment_buf.is_empty() {
                    self.mode = Mode::Normal;
                } else {
                    self.mode = Mode::CommentNormal;
                }
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let text = self.build_comment_context();
                self.request_tmux_send(text, TmuxSendCompletion::SaveLegacyAnnotation);
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.comment_buf.push('\n');
            }
            KeyCode::Enter => {
                self.submit_comment();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.comment_buf.pop();
            }
            KeyCode::Char(c) => self.comment_buf.push(c),
            _ => {}
        }
    }

    fn handle_comment_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.comment_buf.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Char('a') | KeyCode::Char('i') => {
                self.mode = Mode::CommentInsert;
            }
            KeyCode::Enter | KeyCode::Char('c') => {
                self.submit_comment();
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    fn handle_command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.search_query.clear();
            }
            KeyCode::Enter => {
                self.build_search_matches();
                self.jump_next_search_match();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.search_query.pop();
            }
            KeyCode::Char(c) => self.search_query.push(c),
            _ => {}
        }
    }

    fn submit_comment(&mut self) {
        if self.comment_buf.trim().is_empty() {
            // If editing and user cleared the comment, delete the annotation
            if let Some(idx) = self.editing_annotation.take()
                && idx < self.annotations.len()
            {
                self.annotations.remove(idx);
            }
            self.comment_buf.clear();
            return;
        }

        // If editing an existing annotation, update in place
        if let Some(idx) = self.editing_annotation.take()
            && idx < self.annotations.len()
        {
            self.annotations[idx].comment = self.comment_buf.clone();
            self.comment_buf.clear();
            return;
        }

        let (start, end) = match self.comment_selection.take() {
            Some(range) => range,
            None => (self.cursor, self.cursor),
        };

        let start_fl = match self.flat_lines.get(start) {
            Some(fl) => *fl,
            None => return,
        };
        let file = match self.files.get(start_fl.file_idx) {
            Some(f) => f.path.clone(),
            None => return,
        };

        let clamped_end = (start..=end)
            .rev()
            .find(|&i| {
                self.flat_lines
                    .get(i)
                    .is_some_and(|fl| fl.file_idx == start_fl.file_idx)
            })
            .unwrap_or(start);

        let mut context_lines = Vec::new();
        let mut old_lines: Vec<u32> = Vec::new();
        let mut new_lines: Vec<u32> = Vec::new();

        for i in start..=clamped_end {
            if let Some(line) = self.get_line(i) {
                if let Some(n) = line.old_lineno {
                    old_lines.push(n);
                }
                if let Some(n) = line.new_lineno {
                    new_lines.push(n);
                }
                context_lines.push(format!("{}{}", line.kind.prefix(), line.content));
            }
        }

        self.annotations.push(Annotation {
            file,
            flat_start: start,
            flat_end: clamped_end,
            display_range: build_display_range(&old_lines, &new_lines),
            diff_context: context_lines.join("\n"),
            comment: self.comment_buf.clone(),
        });

        self.comment_buf.clear();
    }

    fn build_search_matches(&mut self) {
        self.search_matches.clear();
        if self.search_query.is_empty() {
            return;
        }
        let query = self.search_query.to_lowercase();
        for (i, fl) in self.flat_lines.iter().enumerate() {
            if let Some(content) = self
                .files
                .get(fl.file_idx)
                .and_then(|f| f.hunks.get(fl.hunk_idx))
                .and_then(|h| h.lines.get(fl.line_idx))
                .map(|l| &l.content)
                && content.to_lowercase().contains(&query)
            {
                self.search_matches.push(i);
            }
        }
    }

    fn jump_next_search_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        let pos = self.search_matches.partition_point(|&m| m <= self.cursor);
        let idx = if pos < self.search_matches.len() {
            pos
        } else {
            0
        };
        self.cursor = self.search_matches[idx];
    }

    fn jump_prev_search_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        let pos = self.search_matches.partition_point(|&m| m < self.cursor);
        let idx = if pos > 0 {
            pos - 1
        } else {
            self.search_matches.len() - 1
        };
        self.cursor = self.search_matches[idx];
    }

    fn jump_next_hunk(&mut self) {
        if let Some(current) = self.flat_lines.get(self.cursor) {
            let (cf, ch) = (current.file_idx, current.hunk_idx);
            for (i, fl) in self.flat_lines.iter().enumerate().skip(self.cursor + 1) {
                if fl.file_idx != cf || fl.hunk_idx != ch {
                    self.cursor = i;
                    return;
                }
            }
        }
    }

    fn jump_prev_hunk(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let Some(current) = self.flat_lines.get(self.cursor) else {
            return;
        };
        let (cf, ch) = (current.file_idx, current.hunk_idx);

        let mut prev_end = self.cursor - 1;
        while prev_end > 0 {
            let fl = &self.flat_lines[prev_end];
            if fl.file_idx != cf || fl.hunk_idx != ch {
                break;
            }
            prev_end -= 1;
        }
        let target = &self.flat_lines[prev_end];
        let (tf, th) = (target.file_idx, target.hunk_idx);
        let start = self.flat_lines[..=prev_end]
            .iter()
            .rposition(|fl| fl.file_idx != tf || fl.hunk_idx != th)
            .map(|i| i + 1)
            .unwrap_or(0);
        self.cursor = start;
    }

    fn jump_next_file(&mut self) {
        let pos = self.file_starts.partition_point(|&s| s <= self.cursor);
        if pos < self.file_starts.len() {
            self.cursor = self.file_starts[pos];
        }
    }

    fn jump_prev_file(&mut self) {
        let pos = self.file_starts.partition_point(|&s| s <= self.cursor);
        if pos >= 2 {
            self.cursor = self.file_starts[pos - 2];
        } else if pos == 1 {
            self.cursor = self.file_starts[0];
        }
    }

    // Build the text to copy when the user yanks a visual-line selection.
    // Called with an inclusive range `start..=end` of flat_line indices.
    //
    // TODO (design choice): pick what gets yanked.
    //   Option 1 — "what you see" (current default):
    //     Skip lines hidden on the current side (Additions when Left, Deletions when Right).
    //     Emit raw `content` (no +/- prefix). Best for pasting runnable code.
    //
    //   Option 2 — "diff text":
    //     Include every line with its prefix: `format!("{}{}", line.kind.prefix(), line.content)`.
    //     Paste-able into a .patch file or a Markdown diff code block.
    //
    //   Option 3 — "both sides":
    //     Yield old+new as two blocks separated by `---`. Niche but useful for comparisons.
    //
    // Swap bodies freely — the keybind handler just consumes the returned String.
    fn perform_yank(&mut self, start: usize, end: usize) {
        let text = self.yank_text(start, end);
        if text.is_empty() {
            self.toast = Some("nothing to yank".to_string());
            return;
        }
        let line_count = text.lines().count();
        self.toast = Some(match crate::clipboard::copy_to_clipboard(&text) {
            Ok(()) => format!(
                "yanked {} line{}",
                line_count,
                if line_count == 1 { "" } else { "s" }
            ),
            Err(e) => format!("yank failed: {e}"),
        });
    }

    fn request_tmux_send(&mut self, text: String, completion: TmuxSendCompletion) {
        if text.trim().is_empty() {
            self.toast = Some("nothing to send".to_string());
            return;
        }
        if !crate::tmux::in_tmux() {
            self.toast = Some("not in tmux".to_string());
            return;
        }

        self.tmux_pending_text = text;
        self.tmux_completion = completion;

        // Try direct-send to remembered target
        if let Some((target, mode)) = self.tmux_last_target.clone() {
            if crate::tmux::pane_exists(&target) {
                self.dispatch_tmux_send(&target, mode);
                return;
            }
            // Stale target: fall through to picker with a toast
            self.tmux_last_target = None;
            self.toast = Some("last target gone, pick again".to_string());
        }

        self.open_tmux_picker();
    }

    fn open_tmux_picker(&mut self) {
        let panes = match crate::tmux::list_panes() {
            Ok(p) => p,
            Err(e) => {
                self.toast = Some(format!("tmux list failed: {e}"));
                self.tmux_pending_text.clear();
                self.tmux_completion = TmuxSendCompletion::Review;
                return;
            }
        };
        if panes.is_empty() {
            self.toast = Some("no other panes".to_string());
            self.tmux_pending_text.clear();
            self.tmux_completion = TmuxSendCompletion::Review;
            return;
        }
        self.tmux_panes = panes;
        self.tmux_cursor = 0;
        if matches!(
            self.tmux_completion,
            TmuxSendCompletion::SaveHumanNote { .. }
        ) {
            self.input_mode = InputMode::Normal;
        }
        self.mode = Mode::TmuxPanePick;
    }

    fn dispatch_tmux_send(&mut self, target: &str, mode: crate::tmux::PasteMode) {
        let text = std::mem::take(&mut self.tmux_pending_text);
        let completion = std::mem::replace(&mut self.tmux_completion, TmuxSendCompletion::Review);
        match crate::tmux::send_to_pane(target, &text, mode) {
            Ok(()) => {
                self.tmux_last_target = Some((target.to_string(), mode));
                let mode_label = match mode {
                    crate::tmux::PasteMode::Bracketed => "bracketed",
                    crate::tmux::PasteMode::Plain => "plain",
                };
                self.toast = Some(format!(
                    "sent {} byte{} to {} ({})",
                    text.len(),
                    if text.len() == 1 { "" } else { "s" },
                    target,
                    mode_label,
                ));
                match completion {
                    TmuxSendCompletion::Review => {}
                    TmuxSendCompletion::SaveLegacyAnnotation => self.submit_comment(),
                    TmuxSendCompletion::SaveHumanNote { viewport } => {
                        self.review_controller.save_human_note_draft(viewport);
                        self.comment_buf.clear();
                        self.input_mode = InputMode::Normal;
                        self.clear_review_selection();
                    }
                }
            }
            Err(e) => {
                self.toast = Some(format!("send failed: {e}"));
                if matches!(completion, TmuxSendCompletion::SaveHumanNote { .. }) {
                    self.input_mode = InputMode::Note;
                }
            }
        }
        self.tmux_panes.clear();
        self.tmux_cursor = 0;
        self.mode = Mode::Normal;
    }

    fn cancel_tmux_picker(&mut self) {
        let completion = std::mem::replace(&mut self.tmux_completion, TmuxSendCompletion::Review);
        self.tmux_panes.clear();
        self.tmux_cursor = 0;
        self.tmux_pending_text.clear();
        if matches!(completion, TmuxSendCompletion::SaveHumanNote { .. }) {
            self.input_mode = InputMode::Note;
        }
        self.mode = Mode::Normal;
    }

    fn handle_tmux_pick_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.cancel_tmux_picker(),
            KeyCode::Char('j') | KeyCode::Down => {
                if self.tmux_cursor + 1 < self.tmux_panes.len() {
                    self.tmux_cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.tmux_cursor = self.tmux_cursor.saturating_sub(1);
            }
            KeyCode::Char('g') => self.tmux_cursor = 0,
            KeyCode::Char('G') => {
                self.tmux_cursor = self.tmux_panes.len().saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(pane) = self.tmux_panes.get(self.tmux_cursor).cloned() {
                    let mode = crate::tmux::paste_mode_for_command(&pane.current_command);
                    self.dispatch_tmux_send(&pane.id, mode);
                }
            }
            _ => {}
        }
    }

    // Build the text sent to tmux from CommentInsert: file:line header +
    // fenced code block of the selected lines + the user's comment.
    // Falls back to raw comment_buf if no selection range is available.
    fn build_comment_context(&self) -> String {
        let Some((start, end)) = self.comment_selection else {
            return self.comment_buf.clone();
        };
        let end = end.min(self.flat_lines.len().saturating_sub(1));
        let Some(start_fl) = self.flat_lines.get(start).copied() else {
            return self.comment_buf.clone();
        };
        let file_path = self
            .files
            .get(start_fl.file_idx)
            .map(|f| f.path.as_str())
            .unwrap_or("?");

        let is_left = self.focus_side == Side::Left;
        let mut min_line: Option<u32> = None;
        let mut max_line: Option<u32> = None;
        let mut code = String::new();

        for i in start..=end {
            let Some(line) = self.get_line(i) else {
                continue;
            };
            if self.line_hidden_on_side(line) {
                continue;
            }
            let lineno = if is_left {
                line.old_lineno
            } else {
                line.new_lineno
            };
            if let Some(n) = lineno {
                min_line = Some(min_line.map_or(n, |m| m.min(n)));
                max_line = Some(max_line.map_or(n, |m| m.max(n)));
            }
            code.push_str(&line.content);
            code.push('\n');
        }

        let range = match (min_line, max_line) {
            (Some(a), Some(b)) if a != b => format!(":{a}-{b}"),
            (Some(a), _) => format!(":{a}"),
            _ => String::new(),
        };

        let ext = std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        format!(
            "`{}{}`:\n\n```{}\n{}```\n\n{}",
            file_path, range, ext, code, self.comment_buf,
        )
    }

    fn yank_text(&self, start: usize, end: usize) -> String {
        let end = end.min(self.flat_lines.len().saturating_sub(1));
        let mut out = String::new();
        for i in start..=end {
            let Some(line) = self.get_line(i) else {
                continue;
            };
            if self.line_hidden_on_side(line) {
                continue;
            }
            out.push_str(&line.content);
            out.push('\n');
        }
        out
    }

    fn next_interesting_line(&self, from: usize, forward: bool) -> Option<usize> {
        let skippable = |i: usize| {
            self.get_line(i)
                .is_some_and(|l| l.content.trim().is_empty() || self.line_hidden_on_side(l))
        };
        if forward {
            (from + 1..self.flat_lines.len()).find(|&i| !skippable(i))
        } else {
            (0..from).rev().find(|&i| !skippable(i))
        }
    }

    fn snap_cursor_to_visible_line(&mut self) {
        if let Some(line) = self.get_line(self.cursor)
            && self.line_hidden_on_side(line)
        {
            let visible = |i: usize| {
                self.get_line(i)
                    .is_some_and(|l| !self.line_hidden_on_side(l))
            };
            let forward = (self.cursor + 1..self.flat_lines.len()).find(|&i| visible(i));
            let backward = (0..self.cursor).rev().find(|&i| visible(i));
            self.cursor = forward.or(backward).unwrap_or(self.cursor);
        }
    }

    fn center_scroll(&mut self, viewport_height: usize) {
        let half = viewport_height / 2;
        self.scroll_offset = self.cursor.saturating_sub(half);
    }
}

fn build_display_range(old_lines: &[u32], new_lines: &[u32]) -> String {
    let old_range = format_line_range(old_lines);
    let new_range = format_line_range(new_lines);

    match (old_range.as_deref(), new_range.as_deref()) {
        (Some(old), Some(new)) if old == new => old.to_string(),
        (Some(old), Some(new)) => format!("L{old}(old) L{new}(new)"),
        (Some(old), None) => format!("L{old}(old)"),
        (None, Some(new)) => format!("L{new}(new)"),
        (None, None) => String::new(),
    }
}

fn format_line_range(lines: &[u32]) -> Option<String> {
    let first = lines.first()?;
    let last = lines.last()?;
    if first == last {
        Some(format!("{first}"))
    } else {
        Some(format!("{first}-{last}"))
    }
}

fn build_flat_lines(files: &[DiffFile]) -> Vec<FlatLine> {
    let mut flat = Vec::new();
    for (fi, file) in files.iter().enumerate() {
        for (hi, hunk) in file.hunks.iter().enumerate() {
            for li in 0..hunk.lines.len() {
                flat.push(FlatLine {
                    file_idx: fi,
                    hunk_idx: hi,
                    line_idx: li,
                });
            }
        }
    }
    flat
}

fn build_file_starts(flat_lines: &[FlatLine]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut last_file = None;
    for (i, fl) in flat_lines.iter().enumerate() {
        if last_file != Some(fl.file_idx) {
            starts.push(i);
            last_file = Some(fl.file_idx);
        }
    }
    starts
}
