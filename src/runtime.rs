use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use crate::annotations::{model::Annotation, output};
use crate::app::App;
use crate::cli::{Action, Invocation};
use crate::config::{ConfigPaths, ConfigResolver};
use crate::core::input::{ReviewInput, ReviewOutput};
use crate::error::AppError;
use crate::input::{LoadContext, LoadOutcome, ReviewLoader};
use crate::pager::{page_plain_text, resolve_text_pager};
use crate::pi_extension;
use crate::vcs::SystemCommandRunner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupAction {
    Print,
    Review,
    InstallPi,
    UninstallPi,
}

pub fn resolve_action(action: &Action) -> StartupAction {
    match action {
        Action::Print(_) => StartupAction::Print,
        Action::Review(_) => StartupAction::Review,
        Action::InstallPi => StartupAction::InstallPi,
        Action::UninstallPi => StartupAction::UninstallPi,
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
        Action::Review(input) => run_review(input, invocation.output),
    }
}

fn run_review(input: ReviewInput, review_output: ReviewOutput) -> Result<ExitCode, AppError> {
    let cwd = std::env::current_dir()?;
    let resolved_config = ConfigResolver::new(ConfigPaths::discover(&cwd)).resolve(&input)?;
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

    replace_stdin_with_tty()?;
    let pager_mode =
        input.kind() == crate::core::input::InputKind::Pager || input.options().pager == Some(true);
    let app = App::new_with_config(loaded.changeset.files, &resolved_config, pager_mode);
    let mut terminal = ratatui::init();
    let app_result = app.run(&mut terminal);
    ratatui::restore();
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
