use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use crate::annotations::{model::Annotation, output};
use crate::app::{App, AppScreen, RemoteReviewOutcome};
use crate::cli::{Action, Invocation};
use crate::config::{ConfigPaths, ConfigResolver};
use crate::core::input::{ReviewInput, ReviewOutput};
use crate::diff::model::{DiffFile, FileChangeKind};
use crate::error::AppError;
use crate::input::{LoadContext, LoadOutcome, ReviewLoader};
use crate::pager::{page_plain_text, resolve_text_pager};
use crate::pi_extension;
use crate::process::command::SystemCommandExecutor;
use crate::remote_review::RemoteReviewPublisher;
use crate::review::{ContextSourceLoader, NativeContextSourceLoader};
use crate::review_map::{ReviewMapClient, ReviewMapResolveRequest, ReviewMapRuntime};
use crate::terminal::TerminalSession;
use crate::ui::review::ReviewHeading;
use crate::vcs::SystemCommandRunner;
use crate::watch::WatchRuntime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupAction {
    Print,
    Review,
    InstallPi,
    UninstallPi,
    Markup,
    Session,
    Daemon,
    Skill,
    Server,
}

pub fn resolve_action(action: &Action) -> StartupAction {
    match action {
        Action::Print(_) => StartupAction::Print,
        Action::Review(_) => StartupAction::Review,
        Action::InstallPi => StartupAction::InstallPi,
        Action::UninstallPi => StartupAction::UninstallPi,
        Action::MarkupRender(_) | Action::MarkupGuide => StartupAction::Markup,
        Action::Session(_) => StartupAction::Session,
        Action::DaemonServe => StartupAction::Daemon,
        Action::SkillPath => StartupAction::Skill,
        Action::Server(_) => StartupAction::Server,
    }
}

pub fn stdin_needs_tty_replacement(stdin_is_terminal: bool) -> bool {
    !stdin_is_terminal
}

pub fn initial_screen(
    kind: crate::core::input::InputKind,
    pager_mode: bool,
    start_on_map: bool,
) -> AppScreen {
    if kind == crate::core::input::InputKind::PullRequest && !pager_mode && start_on_map {
        AppScreen::ReviewMap
    } else {
        AppScreen::Review
    }
}

pub fn run(invocation: Invocation) -> Result<ExitCode, AppError> {
    match invocation.action {
        Action::Print(text) => {
            print!("{text}");
            io::stdout().flush()?;
            Ok(ExitCode::SUCCESS)
        }
        Action::InstallPi => {
            pi_extension::install("pi")?;
            Ok(ExitCode::SUCCESS)
        }
        Action::UninstallPi => {
            pi_extension::uninstall("pi")?;
            Ok(ExitCode::SUCCESS)
        }
        Action::MarkupGuide => {
            print!("{}", crate::markup::guide());
            io::stdout().flush()?;
            Ok(ExitCode::SUCCESS)
        }
        Action::MarkupRender(options) => {
            crate::markup::render(&options)?;
            Ok(ExitCode::SUCCESS)
        }
        Action::SkillPath => {
            let path = crate::session::materialize_review_skill()?;
            println!("{}", path.display());
            Ok(ExitCode::SUCCESS)
        }
        Action::Session(command) => {
            crate::session::run_session_command(command)?;
            Ok(ExitCode::SUCCESS)
        }
        Action::DaemonServe => {
            crate::session::run_daemon_from_environment()?;
            Ok(ExitCode::SUCCESS)
        }
        Action::Server(arguments) => run_server_companion(&arguments),
        Action::Review(input) => run_review(input, invocation.output),
    }
}

pub fn companion_path(ramo_executable: &std::path::Path) -> PathBuf {
    let file_name = if ramo_executable.extension() == Some(OsStr::new("exe")) {
        "ramo-server.exe"
    } else {
        "ramo-server"
    };
    ramo_executable.with_file_name(file_name)
}

fn run_server_companion(arguments: &[OsString]) -> Result<ExitCode, AppError> {
    let current = std::env::current_exe()?;
    let companion = companion_path(&current);
    if !companion.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "ramo-server is not installed; reinstall Ramo or run cargo install --path crates/ramo-server",
        )
        .into());
    }
    let status = std::process::Command::new(companion)
        .args(arguments)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;
    let code = status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1);
    Ok(ExitCode::from(code))
}

fn run_review(input: ReviewInput, review_output: ReviewOutput) -> Result<ExitCode, AppError> {
    let cwd = std::env::current_dir()?;
    let config_paths = ConfigPaths::discover(&cwd);
    let mut resolved_config = ConfigResolver::new(config_paths.clone()).resolve(&input)?;
    let runner = SystemCommandRunner;
    let load_context = LoadContext {
        cwd: &cwd,
        config: &resolved_config,
        runner: &runner,
    };
    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let mut pull_request: Option<(
        crate::remote_review::PullRequestReviewContext,
        Box<dyn RemoteReviewPublisher>,
    )> = None;
    let mut context_loader: Box<dyn ContextSourceLoader> =
        Box::new(NativeContextSourceLoader::default());
    let with_comments_requested = matches!(
        input,
        ReviewInput::PullRequest {
            with_comments: true,
            ..
        }
    );
    let mut imported_threads = Vec::new();
    let loaded = if matches!(input, ReviewInput::PullRequest { .. }) {
        let mut github = crate::github::GithubCli::new(SystemCommandExecutor);
        let loaded_pr =
            ReviewLoader.load_pull_request(&input, &mut stdin_lock, &load_context, &mut github)?;
        imported_threads = loaded_pr.imported_threads;
        context_loader = Box::new(crate::github::GithubContextSourceLoader::new(
            SystemCommandExecutor,
        ));
        pull_request = Some((loaded_pr.context, Box::new(github)));
        loaded_pr.review
    } else {
        let outcome =
            ReviewLoader.load_outcome_with_context(&input, &mut stdin_lock, &load_context)?;
        match outcome {
            LoadOutcome::Review(loaded) => *loaded,
            LoadOutcome::PlainText(text) => {
                let env = std::env::vars().collect();
                let pager = resolve_text_pager(&env)?;
                let code = page_plain_text(&text, &pager, io::stdout().is_terminal())?;
                return Ok(ExitCode::from(code));
            }
        }
    };

    if loaded.changeset.files.is_empty() {
        eprintln!("No changes to review.");
        return Ok(ExitCode::SUCCESS);
    }

    if with_comments_requested && imported_threads.is_empty() {
        resolved_config
            .startup_notices
            .push("No unresolved GitHub threads".into());
    }

    let pager_mode =
        input.kind() == crate::core::input::InputKind::Pager || input.options().pager == Some(true);
    let remote_update = if pager_mode {
        resolved_config.startup_notices.clear();
        None
    } else {
        let notice_count = resolved_config.startup_notices.len();
        crate::startup_notice::append_local_startup_notice(&mut resolved_config.startup_notices);
        let local_upgrade_notice = resolved_config.startup_notices.len() > notice_count;
        (!local_upgrade_notice)
            .then(|| crate::startup_notice::RemoteUpdateRuntime::start(&cwd))
            .flatten()
    };

    if resolved_config.theme == "auto" {
        let appearance = crate::ui::appearance::detect_terminal_appearance();
        resolved_config.theme = match appearance {
            Some(crate::ui::themes::TerminalAppearance::Light) => {
                crate::ui::themes::DEFAULT_LIGHT_THEME_ID.into()
            }
            Some(crate::ui::themes::TerminalAppearance::Dark) | None => {
                crate::ui::themes::DEFAULT_DARK_THEME_ID.into()
            }
        };
    }

    let reloadable = !matches!(loaded.reload_plan, crate::input::ReloadPlan::None);
    if resolved_config.watch && !reloadable {
        return Err(crate::input::LoadError::NotReloadable.into());
    }
    let mut watch_runtime = reloadable.then(|| {
        WatchRuntime::new(
            &loaded,
            cwd.clone(),
            resolved_config.clone(),
            resolved_config.watch,
            Instant::now(),
        )
    });
    let editor_base = match &loaded.reload_plan {
        crate::input::ReloadPlan::Vcs { repo_root, .. } => repo_root.clone(),
        _ => cwd.clone(),
    };

    let session_descriptor = crate::session::create_session_descriptor(&input, &loaded, &cwd);
    let review_heading = if let Some((context, _)) = pull_request.as_ref() {
        ReviewHeading::PullRequest {
            number: context.number,
            title: context.title.clone(),
            base_ref: context.base_ref.clone(),
            head_ref: context.head_ref.clone(),
        }
    } else {
        match &input {
            ReviewInput::VcsDiff {
                range: None,
                staged: false,
                ..
            } => ReviewHeading::Local("Working tree".into()),
            ReviewInput::VcsDiff { staged: true, .. } => {
                ReviewHeading::Local("Staged changes".into())
            }
            _ => ReviewHeading::Local(loaded.changeset.title.clone()),
        }
    };

    let review_map_startup = prepare_review_map(
        &loaded.changeset.files,
        pull_request.as_ref().map(|(context, _)| context),
        &resolved_config,
        pager_mode,
        &cwd,
    );

    replace_stdin_with_tty()?;
    let mut app = App::new_with_services(
        loaded.changeset.files,
        &resolved_config,
        pager_mode,
        context_loader,
        config_paths.user,
    );
    app.set_review_heading(review_heading);
    match review_map_startup {
        Ok(startup) => {
            let start_on_map =
                initial_screen(input.kind(), pager_mode, resolved_config.start_on_map)
                    == AppScreen::ReviewMap;
            app.attach_review_map(startup.map, startup.runtime, start_on_map);
            if let Some((client, request)) = startup.restart {
                app.configure_review_map_retry(client, request);
            }
            if let Some((code, message)) = startup.failure {
                app.set_review_map_failure(code, message);
            }
        }
        Err(message) => {
            eprintln!("ramo: Review Map disabled: {message}");
        }
    }
    if let Some((context, publisher)) = pull_request {
        app.attach_pull_request(context, publisher);
    }
    if let Some(remote_update) = remote_update {
        app.attach_remote_update(remote_update);
    }
    let session_client = crate::session::ensure_session_daemon()?;
    let (width, height) = crossterm::terminal::size().unwrap_or((100, 24));
    app.review_controller
        .attach_github_threads(imported_threads, crate::review::Viewport { width, height });
    let initial_snapshot = crate::session::build_snapshot(
        &mut app.review_controller,
        crate::review::Viewport { width, height },
        crate::session::session_timestamp(),
    );
    let registration =
        crate::session::build_registration(&session_descriptor, app.review_controller.files());
    match crate::session::SessionRegistrationClient::start(
        session_client.address(),
        registration,
        initial_snapshot.clone(),
        crate::session::current_session_path(),
    ) {
        Ok(client) => {
            app.attach_session_registration(client, session_descriptor, initial_snapshot.state)
        }
        Err(error) => eprintln!("ramo: live session registration disabled: {error}"),
    }
    let mut terminal = TerminalSession::enter()?;
    #[cfg(debug_assertions)]
    if std::env::var_os("RAMO_TEST_PANIC_AFTER_TERMINAL").is_some() {
        panic!("injected terminal panic");
    }
    #[cfg(debug_assertions)]
    let inject_runtime_error = std::env::var_os("RAMO_TEST_ERROR_AFTER_TERMINAL").is_some();
    #[cfg(not(debug_assertions))]
    let inject_runtime_error = false;
    let app_result = if inject_runtime_error {
        Err(io::Error::other("injected terminal runtime error"))
    } else {
        app.run_with_services(&mut terminal, watch_runtime.as_mut(), &editor_base)
    };
    let restore_result = terminal.restore();
    restore_result?;
    let result = app_result?;
    if should_finish_local_annotations(&input, result.remote_outcome) {
        finish_annotations(result.annotations, review_output)?;
    }
    Ok(ExitCode::SUCCESS)
}

struct ReviewMapStartup {
    map: ramo_core::review_map::ReviewMap,
    runtime: Option<ReviewMapRuntime>,
    restart: Option<(ReviewMapClient, ReviewMapResolveRequest)>,
    failure: Option<(ramo_core::review_map::ReviewMapFailureCode, String)>,
}

fn prepare_review_map(
    files: &[DiffFile],
    pull_request: Option<&crate::remote_review::PullRequestReviewContext>,
    config: &crate::config::ResolvedConfig,
    pager_mode: bool,
    cwd: &std::path::Path,
) -> Result<ReviewMapStartup, String> {
    let identity = pull_request.map_or_else(
        || ramo_core::review_map::ReviewMapIdentity {
            repository: format!("local/{}", cwd.display()),
            pull_request: 0,
            base_sha: "local-base".into(),
            head_sha: local_revision(files),
        },
        |context| ramo_core::review_map::ReviewMapIdentity {
            repository: context.repository.clone(),
            pull_request: context.number,
            base_sha: context.base_revision.clone(),
            head_sha: context.captured_revision.clone(),
        },
    );
    let input = ramo_core::review_map::ReviewMapInput {
        identity: identity.clone(),
        files: files.iter().map(review_map_input_file).collect(),
        codeowners: None,
    };
    let classifier = ramo_core::review_map::ClassifierConfig::with_patterns(
        config.test_file_patterns.clone(),
        Vec::new(),
    );
    let map = ramo_core::review_map::build_review_map(&input, &classifier)
        .map_err(|error| error.to_string())?;
    let Some(context) = pull_request else {
        return Ok(ReviewMapStartup {
            map,
            runtime: None,
            restart: None,
            failure: None,
        });
    };
    if pager_mode || !config.ai_summaries {
        return Ok(ReviewMapStartup {
            map,
            runtime: None,
            restart: None,
            failure: None,
        });
    }
    let unavailable = |message: String| ReviewMapStartup {
        map: map.clone(),
        runtime: None,
        restart: None,
        failure: Some((
            ramo_core::review_map::ReviewMapFailureCode::ServerUnreachable,
            message,
        )),
    };
    let Some(token_file) = config.review_map_token_file.as_deref() else {
        return Ok(unavailable(
            "Laptop analysis unavailable · configure review_map_token_file".into(),
        ));
    };
    let token = match read_review_map_token(token_file) {
        Ok(token) => token,
        Err(error) => {
            return Ok(unavailable(format!(
                "Laptop analysis unavailable · could not read {}: {error}",
                token_file.display()
            )));
        }
    };
    let client = match ReviewMapClient::new(&config.review_map_server, token) {
        Ok(client) => client,
        Err(error) => return Ok(unavailable(error.to_string())),
    };
    let request = ReviewMapResolveRequest::new(
        context.repository.clone(),
        context.number,
        context.captured_revision.clone(),
    );
    let runtime = ReviewMapRuntime::start(client.clone(), request.clone());
    Ok(ReviewMapStartup {
        map,
        runtime: Some(runtime),
        restart: Some((client, request)),
        failure: None,
    })
}

fn review_map_input_file(file: &DiffFile) -> ramo_core::review_map::ReviewMapInputFile {
    ramo_core::review_map::ReviewMapInputFile {
        path: file.path.clone(),
        previous_path: file.previous_path.clone(),
        status: match file.change_kind {
            FileChangeKind::Modified => "modified",
            FileChangeKind::Added => "added",
            FileChangeKind::Deleted => "deleted",
            FileChangeKind::Renamed => "renamed",
            FileChangeKind::Copied => "copied",
        }
        .into(),
        additions: file.stats.additions,
        deletions: file.stats.deletions,
        patch: (!file.patch.is_empty()).then(|| file.patch.clone()),
        binary: file.is_binary,
    }
}

fn local_revision(files: &[DiffFile]) -> String {
    let mut revision = String::from("local");
    for file in files {
        revision.push(':');
        revision.push_str(&file.id);
        revision.push(':');
        revision.push_str(&file.stats.additions.to_string());
        revision.push(':');
        revision.push_str(&file.stats.deletions.to_string());
    }
    revision
}

fn read_review_map_token(path: &std::path::Path) -> io::Result<String> {
    const MAX_TOKEN_FILE: u64 = 16 * 1024;
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > MAX_TOKEN_FILE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "client token file exceeds 16 KiB",
        ));
    }
    let source = fs::read_to_string(path)?;
    let token = serde_json::from_str::<serde_json::Value>(&source)
        .ok()
        .and_then(|value| {
            value
                .get("token")
                .and_then(|token| token.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| source.trim().to_owned());
    if token.is_empty() || token.contains(['\r', '\n']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "client token file is malformed",
        ));
    }
    Ok(token)
}

pub fn should_finish_local_annotations(
    input: &ReviewInput,
    remote_outcome: Option<RemoteReviewOutcome>,
) -> bool {
    !matches!(input, ReviewInput::PullRequest { .. }) && remote_outcome.is_none()
}

fn finish_annotations(annotations: Vec<Annotation>, review_output: ReviewOutput) -> io::Result<()> {
    if review_output.stdout {
        output::print_markdown(&annotations);
        return Ok(());
    }
    if let Some(path) = review_output.markdown_path {
        output::write_markdown(&annotations, &path)?;
        eprintln!(
            "Wrote {} comment(s) to {}",
            annotations.len(),
            path.display()
        );
        return Ok(());
    }
    if annotations.is_empty() {
        eprintln!("No comments.");
        return Ok(());
    }
    match prompt_save_tty(annotations.len()) {
        Ok(true) => {
            output::write_markdown(&annotations, &PathBuf::from("ramo-review.md"))?;
            eprintln!("Saved to ramo-review.md.");
        }
        Ok(false) => eprintln!("\n{}", output::format_markdown(&annotations)),
        Err(_) => {
            output::write_markdown(&annotations, &PathBuf::from("ramo-review.md"))?;
            eprintln!("Wrote {} comment(s) to ramo-review.md", annotations.len());
        }
    }
    Ok(())
}

fn prompt_save_tty(count: usize) -> io::Result<bool> {
    let tty = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(tty_path())?;
    let mut writer = io::BufWriter::new(&tty);
    let mut reader = io::BufReader::new(&tty);
    write!(writer, "Save {count} comment(s) to ramo-review.md? [y/N] ")?;
    writer.flush()?;
    let mut answer = String::new();
    reader.read_line(&mut answer)?;
    Ok(answer.trim().eq_ignore_ascii_case("y"))
}

#[cfg(unix)]
fn tty_path() -> &'static str {
    "/dev/tty"
}

#[cfg(windows)]
fn tty_path() -> &'static str {
    "CONIN$"
}

#[cfg(unix)]
fn replace_stdin_with_tty() -> io::Result<()> {
    if !stdin_needs_tty_replacement(io::stdin().is_terminal()) {
        return Ok(());
    }
    use std::os::unix::io::AsRawFd;
    let tty = fs::OpenOptions::new().read(true).open("/dev/tty")?;
    let result = unsafe { libc::dup2(tty.as_raw_fd(), libc::STDIN_FILENO) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn replace_stdin_with_tty() -> io::Result<()> {
    if !stdin_needs_tty_replacement(io::stdin().is_terminal()) {
        return Ok(());
    }

    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    const STD_INPUT_HANDLE: u32 = -10_i32 as u32;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "SetStdHandle"]
        fn set_std_handle(handle_kind: u32, handle: *mut c_void) -> i32;
    }

    let console = fs::OpenOptions::new().read(true).open("CONIN$")?;
    // SAFETY: `console` owns a valid Windows console handle opened for reading. The handle is
    // intentionally kept alive below because the process standard-handle table does not own it.
    let succeeded = unsafe { set_std_handle(STD_INPUT_HANDLE, console.as_raw_handle()) };
    if succeeded == 0 {
        return Err(io::Error::last_os_error());
    }
    std::mem::forget(console);
    Ok(())
}
