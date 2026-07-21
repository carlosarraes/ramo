use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use crate::annotations::{model::Annotation, output};
use crate::app::App;
use crate::cli::{Action, Invocation};
use crate::config::{ConfigPaths, ConfigResolver};
use crate::core::input::{ReviewInput, ReviewOutput};
use crate::error::AppError;
use crate::input::{LoadContext, LoadOutcome, ReviewLoader};
use crate::pager::{page_plain_text, resolve_text_pager};
use crate::pi_extension;
use crate::review::NativeContextSourceLoader;
use crate::terminal::TerminalSession;
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
    }
}

pub fn stdin_needs_tty_replacement(stdin_is_terminal: bool) -> bool {
    !stdin_is_terminal
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
        Action::Review(input) => run_review(input, invocation.output),
    }
}

fn run_review(input: ReviewInput, review_output: ReviewOutput) -> Result<ExitCode, AppError> {
    let cwd = std::env::current_dir()?;
    let config_paths = ConfigPaths::discover(&cwd);
    let resolved_config = ConfigResolver::new(config_paths.clone()).resolve(&input)?;
    let runner = SystemCommandRunner;
    let load_context = LoadContext {
        cwd: &cwd,
        config: &resolved_config,
        runner: &runner,
    };
    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let outcome = ReviewLoader.load_outcome_with_context(&input, &mut stdin_lock, &load_context)?;
    let loaded = match outcome {
        LoadOutcome::Review(loaded) => *loaded,
        LoadOutcome::PlainText(text) => {
            let env = std::env::vars().collect();
            let pager = resolve_text_pager(&env)?;
            let code = page_plain_text(&text, &pager, io::stdout().is_terminal())?;
            return Ok(ExitCode::from(code));
        }
    };

    if loaded.changeset.files.is_empty() {
        eprintln!("No changes to review.");
        return Ok(ExitCode::SUCCESS);
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

    replace_stdin_with_tty()?;
    let pager_mode =
        input.kind() == crate::core::input::InputKind::Pager || input.options().pager == Some(true);
    let mut app = App::new_with_services(
        loaded.changeset.files,
        &resolved_config,
        pager_mode,
        Box::new(NativeContextSourceLoader::default()),
        config_paths.user,
    );
    let session_client = crate::session::ensure_session_daemon()?;
    let (width, height) = crossterm::terminal::size().unwrap_or((100, 24));
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
        Err(error) => eprintln!("pdiff: live session registration disabled: {error}"),
    }
    let mut terminal = TerminalSession::enter()?;
    #[cfg(debug_assertions)]
    if std::env::var_os("PDIFF_TEST_PANIC_AFTER_TERMINAL").is_some() {
        panic!("injected terminal panic");
    }
    #[cfg(debug_assertions)]
    let inject_runtime_error = std::env::var_os("PDIFF_TEST_ERROR_AFTER_TERMINAL").is_some();
    #[cfg(not(debug_assertions))]
    let inject_runtime_error = false;
    let app_result = if inject_runtime_error {
        Err(io::Error::other("injected terminal runtime error"))
    } else {
        app.run_with_services(&mut terminal, watch_runtime.as_mut(), &editor_base)
    };
    let restore_result = terminal.restore();
    restore_result?;
    let annotations = app_result?;
    finish_annotations(annotations, review_output)?;
    Ok(ExitCode::SUCCESS)
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
            output::write_markdown(&annotations, &PathBuf::from("pdiff-review.md"))?;
            eprintln!("Saved to pdiff-review.md.");
        }
        Ok(false) => eprintln!("\n{}", output::format_markdown(&annotations)),
        Err(_) => {
            output::write_markdown(&annotations, &PathBuf::from("pdiff-review.md"))?;
            eprintln!("Wrote {} comment(s) to pdiff-review.md", annotations.len());
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
    write!(writer, "Save {count} comment(s) to pdiff-review.md? [y/N] ")?;
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
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "interactive piped reviews require Windows console support",
        ))
    }
}
