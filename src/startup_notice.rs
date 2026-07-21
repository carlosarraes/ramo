use std::cmp::Ordering;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

const STATE_VERSION: u64 = 1;
const DISABLE_UPDATE_NOTICE_ENV: &str = "PDIFF_DISABLE_UPDATE_NOTICE";
const HUNK_DISABLE_UPDATE_NOTICE_ENV: &str = "HUNK_DISABLE_UPDATE_NOTICE";
const RELEASE_REPOSITORY: &str = "https://github.com/carlosarraes/pdiff.git";
const UPDATE_TIMEOUT: Duration = Duration::from_secs(5);
const UPDATE_DELAY: Duration = Duration::from_millis(1_200);
const UPDATE_REPEAT: Duration = Duration::from_secs(21_600);
const MAX_TAG_OUTPUT_BYTES: usize = 1024 * 1024;

pub fn append_local_startup_notice(notices: &mut Vec<String>) {
    let disabled = update_notices_disabled();
    let Some(path) = state_path() else {
        return;
    };
    if let Some(notice) = resolve_skill_refresh_notice(&path, env!("CARGO_PKG_VERSION"), disabled) {
        notices.push(notice);
    }
}

pub fn update_notices_disabled() -> bool {
    [DISABLE_UPDATE_NOTICE_ENV, HUNK_DISABLE_UPDATE_NOTICE_ENV]
        .iter()
        .any(|key| std::env::var_os(key).is_some_and(|value| value == "1"))
}

pub fn resolve_skill_refresh_notice(
    state_path: &Path,
    installed_version: &str,
    disabled: bool,
) -> Option<String> {
    if disabled || installed_version.is_empty() {
        return None;
    }

    let previous_version = fs::read_to_string(state_path)
        .ok()
        .and_then(|source| serde_json::from_str::<serde_json::Value>(&source).ok())
        .and_then(|state| {
            state
                .get("lastSeenCliVersion")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });

    let parent = state_path.parent()?;
    if fs::create_dir_all(parent).is_err() {
        return None;
    }
    let state = serde_json::json!({
        "version": STATE_VERSION,
        "lastSeenCliVersion": installed_version,
    });
    let Ok(source) = serde_json::to_string_pretty(&state) else {
        return None;
    };
    if fs::write(state_path, format!("{source}\n")).is_err() {
        return None;
    }

    match previous_version {
        Some(previous) if previous != installed_version => Some(format!(
            "pdiff {installed_version} installed • If your agent copied pdiff's skill, run pdiff skill path"
        )),
        _ => None,
    }
}

fn state_path() -> Option<PathBuf> {
    dirs::config_dir().map(|path| path.join("pdiff/state.json"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteUpdatePoll {
    Pending,
    Ready(String),
    Complete,
}

pub struct RemoteUpdateRuntime {
    receiver: Receiver<Option<String>>,
    cancelled: Arc<AtomicBool>,
}

impl RemoteUpdateRuntime {
    pub fn start(cwd: &Path) -> Option<Self> {
        if update_notices_disabled() {
            return None;
        }
        let cwd = cwd.to_path_buf();
        let installed_version = env!("CARGO_PKG_VERSION").to_owned();
        let timeout = update_timeout();
        let delay = update_delay();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("pdiff-update-notice".into())
            .spawn(move || {
                if !wait_interruptibly(&worker_cancelled, delay) {
                    return;
                }
                loop {
                    let notice = query_remote_tags(&cwd, timeout)
                        .and_then(|tags| select_remote_update_notice(&installed_version, &tags));
                    if sender.send(notice).is_err()
                        || !wait_interruptibly(&worker_cancelled, UPDATE_REPEAT)
                    {
                        return;
                    }
                }
            })
            .ok()?;
        Some(Self {
            receiver,
            cancelled,
        })
    }

    pub fn poll(&mut self) -> RemoteUpdatePoll {
        match self.receiver.try_recv() {
            Ok(Some(notice)) => RemoteUpdatePoll::Ready(notice),
            Ok(None) => RemoteUpdatePoll::Pending,
            Err(TryRecvError::Disconnected) => RemoteUpdatePoll::Complete,
            Err(TryRecvError::Empty) => RemoteUpdatePoll::Pending,
        }
    }
}

impl Drop for RemoteUpdateRuntime {
    fn drop(&mut self) {
        self.cancelled.store(true, AtomicOrdering::Release);
    }
}

pub fn select_remote_update_notice(installed_version: &str, tags: &str) -> Option<String> {
    let installed = Version::parse(installed_version)?;
    let candidates = tags.lines().filter_map(|line| {
        let (_, reference) = line.split_once('\t')?;
        Version::parse(reference.strip_prefix("refs/tags/v")?)
    });
    let selected = if installed.prerelease.is_empty() {
        candidates
            .clone()
            .filter(|candidate| candidate.prerelease.is_empty() && candidate > &installed)
            .max()
            .or_else(|| {
                candidates
                    .filter(|candidate| !candidate.prerelease.is_empty() && candidate > &installed)
                    .max()
            })
    } else {
        candidates.filter(|candidate| candidate > &installed).max()
    }?;
    Some(format!(
        "Update available: {} • install the latest pdiff release",
        selected.normalized()
    ))
}

fn query_remote_tags(cwd: &Path, timeout: Duration) -> Option<String> {
    let mut child = Command::new("git")
        .args([
            "-c",
            "credential.interactive=never",
            "ls-remote",
            "--tags",
            "--refs",
            RELEASE_REPOSITORY,
            "refs/tags/v*",
        ])
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || read_bounded(stdout, MAX_TAG_OUTPUT_BYTES));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                // A Git transport helper can inherit stdout. Do not let its pipe keep the
                // update worker blocked after the bounded parent process has been killed.
                drop(reader);
                return None;
            }
        }
    };
    let output = reader.join().ok()?;
    status
        .success()
        .then(|| String::from_utf8(output).ok())
        .flatten()
}

fn read_bounded(mut reader: impl Read, limit: usize) -> Vec<u8> {
    let mut captured = Vec::with_capacity(limit.min(16 * 1024));
    let mut buffer = [0_u8; 4096];
    while let Ok(count) = reader.read(&mut buffer) {
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(captured.len());
        captured.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    captured
}

fn update_timeout() -> Duration {
    #[cfg(debug_assertions)]
    if let Ok(value) = std::env::var("PDIFF_TEST_UPDATE_NOTICE_TIMEOUT_MS")
        && let Ok(milliseconds) = value.parse::<u64>()
    {
        return Duration::from_millis(milliseconds.max(1));
    }
    UPDATE_TIMEOUT
}

fn update_delay() -> Duration {
    #[cfg(debug_assertions)]
    if let Ok(value) = std::env::var("PDIFF_TEST_UPDATE_NOTICE_DELAY_MS")
        && let Ok(milliseconds) = value.parse::<u64>()
    {
        return Duration::from_millis(milliseconds);
    }
    UPDATE_DELAY
}

fn wait_interruptibly(cancelled: &AtomicBool, duration: Duration) -> bool {
    let deadline = Instant::now() + duration;
    while !cancelled.load(AtomicOrdering::Acquire) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return true;
        }
        std::thread::sleep(remaining.min(Duration::from_secs(1)));
    }
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Vec<Identifier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Identifier {
    Numeric(u64),
    Text(String),
}

impl Version {
    fn parse(source: &str) -> Option<Self> {
        let source = source.strip_prefix('v').unwrap_or(source);
        let source = source.split_once('+').map_or(source, |(core, _)| core);
        let (core, prerelease) = source
            .split_once('-')
            .map_or((source, None), |(core, prerelease)| {
                (core, Some(prerelease))
            });
        let mut core = core.split('.');
        let major = parse_core_number(core.next()?)?;
        let minor = parse_core_number(core.next()?)?;
        let patch = parse_core_number(core.next()?)?;
        if core.next().is_some() {
            return None;
        }
        let prerelease = match prerelease {
            Some(source) => source
                .split('.')
                .map(parse_identifier)
                .collect::<Option<Vec<_>>>()?,
            None => Vec::new(),
        };
        Some(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    fn normalized(&self) -> String {
        let mut output = format!("{}.{}.{}", self.major, self.minor, self.patch);
        if !self.prerelease.is_empty() {
            output.push('-');
            for (index, identifier) in self.prerelease.iter().enumerate() {
                if index > 0 {
                    output.push('.');
                }
                match identifier {
                    Identifier::Numeric(value) => output.push_str(&value.to_string()),
                    Identifier::Text(value) => output.push_str(value),
                }
            }
        }
        output
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(
                || match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
                    (true, true) | (false, false) => self.prerelease.cmp(&other.prerelease),
                    (true, false) => Ordering::Greater,
                    (false, true) => Ordering::Less,
                },
            )
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Numeric(left), Self::Numeric(right)) => left.cmp(right),
            (Self::Numeric(_), Self::Text(_)) => Ordering::Less,
            (Self::Text(_), Self::Numeric(_)) => Ordering::Greater,
            (Self::Text(left), Self::Text(right)) => left.cmp(right),
        }
    }
}

impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn parse_core_number(source: &str) -> Option<u64> {
    if source.len() > 1 && source.starts_with('0') {
        return None;
    }
    source.parse().ok()
}

fn parse_identifier(source: &str) -> Option<Identifier> {
    if source.is_empty()
        || !source
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return None;
    }
    if source.bytes().all(|byte| byte.is_ascii_digit()) {
        if source.len() > 1 && source.starts_with('0') {
            return None;
        }
        return source.parse().ok().map(Identifier::Numeric);
    }
    Some(Identifier::Text(source.to_owned()))
}
