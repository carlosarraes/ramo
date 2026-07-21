use std::ffi::OsString;
use std::io;

use crate::process::command::{
    CommandExecutor, CommandRequest, CommandResult, SystemCommandExecutor,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteMode {
    Bracketed,
    Plain,
}

#[derive(Debug, Clone)]
pub struct TmuxPane {
    pub id: String,
    pub label: String,
    pub current_command: String,
}

pub struct TmuxClient<E> {
    executor: E,
    self_pane: Option<String>,
    server: Option<String>,
}

impl<E> TmuxClient<E> {
    pub fn new(executor: E) -> Self {
        Self::with_self_pane(executor, self_pane_id())
    }

    pub fn with_self_pane(executor: E, self_pane: Option<String>) -> Self {
        Self {
            executor,
            self_pane,
            server: None,
        }
    }

    pub fn with_server(executor: E, server: String) -> Self {
        Self {
            executor,
            self_pane: None,
            server: Some(server),
        }
    }

    pub fn into_executor(self) -> E {
        self.executor
    }
}

impl<E: CommandExecutor> TmuxClient<E> {
    pub fn list_panes(&mut self) -> io::Result<Vec<TmuxPane>> {
        let output = self.executor.execute(CommandRequest {
            argv: self.argv(&[
                "list-panes",
                "-a",
                "-F",
                "#{pane_id}\t#{session_name}:#{window_index}.#{pane_index}\t#{window_name}\t#{pane_current_command}",
            ]),
            stdin: None,
            inherit_stdio: false,
        })?;
        require_success("list panes", &output)?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(4, '\t');
                let id = parts.next()?.to_owned();
                let target = parts.next()?;
                let window = parts.next()?;
                let command = parts.next()?.to_owned();
                Some(TmuxPane {
                    label: format!("{id}  {target}  {window}  [{command}]"),
                    id,
                    current_command: command,
                })
            })
            .filter(|pane| self.self_pane.as_deref() != Some(pane.id.as_str()))
            .collect())
    }

    pub fn pane_exists(&mut self, id: &str) -> io::Result<bool> {
        let output = self.executor.execute(CommandRequest {
            argv: self.argv(&["display-message", "-p", "-t", id, "#{pane_id}"]),
            stdin: None,
            inherit_stdio: false,
        })?;
        Ok(output.code == Some(0))
    }

    pub fn send_to_pane(&mut self, target: &str, text: &str, mode: PasteMode) -> io::Result<()> {
        let load = self.executor.execute(CommandRequest {
            argv: self.argv(&["load-buffer", "-b", "pdiff-send", "-"]),
            stdin: Some(text.as_bytes().to_vec()),
            inherit_stdio: false,
        })?;
        require_success("load buffer", &load)?;

        let mut argv = self.argv(&["paste-buffer"]);
        if mode == PasteMode::Bracketed {
            argv.extend(strings(&["-p", "-r"]));
        }
        argv.extend(strings(&["-b", "pdiff-send", "-t", target, "-d"]));
        let paste = self.executor.execute(CommandRequest {
            argv,
            stdin: None,
            inherit_stdio: false,
        })?;
        require_success("paste buffer", &paste)
    }

    fn argv(&self, values: &[&str]) -> Vec<OsString> {
        let mut argv = vec![OsString::from("tmux")];
        if let Some(server) = &self.server {
            argv.extend([OsString::from("-L"), OsString::from(server)]);
        }
        argv.extend(values.iter().map(OsString::from));
        argv
    }
}

pub fn in_tmux() -> bool {
    std::env::var_os("TMUX").is_some()
}

pub fn paste_mode_for_command(command: &str) -> PasteMode {
    match command {
        "pi" => PasteMode::Plain,
        _ => PasteMode::Bracketed,
    }
}

pub fn self_pane_id() -> Option<String> {
    std::env::var("TMUX_PANE").ok()
}

pub fn list_panes() -> io::Result<Vec<TmuxPane>> {
    TmuxClient::new(SystemCommandExecutor).list_panes()
}

pub fn pane_exists(id: &str) -> bool {
    TmuxClient::new(SystemCommandExecutor)
        .pane_exists(id)
        .unwrap_or(false)
}

pub fn send_to_pane(target: &str, text: &str, mode: PasteMode) -> io::Result<()> {
    TmuxClient::new(SystemCommandExecutor).send_to_pane(target, text, mode)
}

fn strings(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn require_success(operation: &str, result: &CommandResult) -> io::Result<()> {
    if result.code == Some(0) {
        return Ok(());
    }
    let status = result
        .code
        .map_or_else(|| "signal".into(), |code| code.to_string());
    Err(io::Error::other(format!(
        "tmux {operation} failed with status {status}: {}",
        String::from_utf8_lossy(&result.stderr).trim()
    )))
}
