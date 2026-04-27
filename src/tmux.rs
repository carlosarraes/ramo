use std::io::{self, Write};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteMode {
    /// Bracketed paste (-p -r): target receives the whole buffer as a single literal
    /// insertion. Suitable for apps that support bracketed paste (modern readline,
    /// Claude Code, vim/nvim, fish/zsh/bash 4.4+). Multi-line preserved.
    Bracketed,
    /// Plain paste-buffer (default): tmux converts each LF to CR (Enter). Each line
    /// gets submitted separately by the target. Suitable for line-submit apps that
    /// don't understand bracketed paste (e.g., pi). Multi-line collapses into N submits.
    Plain,
}

#[derive(Debug, Clone)]
pub struct TmuxPane {
    pub id: String,
    pub label: String,
    pub current_command: String,
}

pub fn in_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Decide which paste protocol to use for a target pane based on the program
/// running there (`tmux #{pane_current_command}`).
///
/// Customize the match arms below for the agents/shells you actually use.
/// A few notes from the codebase exploration:
///   - "claude" — Claude Code CLI, supports bracketed paste, multi-line works great.
///   - "pi"     — pi-mono coding agent (sets `process.title = "pi"`). Treats every
///                LF/CR as Enter, NO bracketed-paste support. Multi-line will fan out
///                into N separate submissions in Plain mode.
///   - "node"   — generic Node CLI (the binary didn't override process.title). Could
///                be either Claude or pi or anything else; pick a safe default.
///   - shells   — bash/zsh/fish with readline 8+ support bracketed paste.
///   - editors  — vim/nvim support bracketed paste in :insert mode.
///
/// TODO: pick the default for unknown commands, and add any agents you use.
pub fn paste_mode_for_command(cmd: &str) -> PasteMode {
    match cmd {
        "claude" => PasteMode::Bracketed,
        "pi" => PasteMode::Plain,
        _ => PasteMode::Bracketed,
    }
}

pub fn self_pane_id() -> Option<String> {
    std::env::var("TMUX_PANE").ok()
}

pub fn list_panes() -> io::Result<Vec<TmuxPane>> {
    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{pane_id}\t#{session_name}:#{window_index}.#{pane_index}\t#{window_name}\t#{pane_current_command}",
        ])
        .output()?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "tmux list-panes failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let self_id = self_pane_id();
    let text = String::from_utf8_lossy(&output.stdout);
    let panes = text
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            let id = parts.next()?.to_string();
            let target = parts.next()?;
            let window = parts.next()?;
            let cmd = parts.next()?;
            Some(TmuxPane {
                label: format!("{}  {}  {}  [{}]", id, target, window, cmd),
                id,
                current_command: cmd.to_string(),
            })
        })
        .filter(|p| self_id.as_deref() != Some(&p.id))
        .collect();

    Ok(panes)
}

pub fn pane_exists(id: &str) -> bool {
    Command::new("tmux")
        .args(["display-message", "-p", "-t", id, "#{pane_id}"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn send_to_pane(target: &str, text: &str, mode: PasteMode) -> io::Result<()> {
    let buffer_name = "pi-diff-send";

    let mut child = Command::new("tmux")
        .args(["load-buffer", "-b", buffer_name, "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| io::Error::other("failed to open tmux stdin"))?
        .write_all(text.as_bytes())?;
    let load = child.wait_with_output()?;
    if !load.status.success() {
        return Err(io::Error::other(format!(
            "tmux load-buffer failed: {}",
            String::from_utf8_lossy(&load.stderr)
        )));
    }

    let mut args: Vec<&str> = vec!["paste-buffer"];
    if mode == PasteMode::Bracketed {
        // -p: bracketed paste so the target treats the whole buffer as one insertion.
        // -r: keep LF as LF instead of converting to CR.
        args.push("-p");
        args.push("-r");
    }
    args.extend(["-b", buffer_name, "-t", target, "-d"]);

    let paste = Command::new("tmux").args(&args).output()?;
    if !paste.status.success() {
        return Err(io::Error::other(format!(
            "tmux paste-buffer failed: {}",
            String::from_utf8_lossy(&paste.stderr)
        )));
    }

    Ok(())
}
