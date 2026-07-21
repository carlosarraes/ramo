use std::io::{self, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{
    ClientSessionFrame, SESSION_REGISTRATION_VERSION, SESSION_WIRE_PREFACE, ServerSessionFrame,
    SessionRegistration, SessionSnapshot, read_session_frame, write_session_frame,
};
use crate::core::input::{InputKind, ReviewInput};
use crate::input::{LoadedReview, ReloadPlan};

#[derive(Debug, Clone)]
struct PublishedState {
    registration: SessionRegistration,
    snapshot: SessionSnapshot,
    session_path: Option<String>,
}

#[derive(Debug)]
enum RegistrationOutbound {
    Publish,
    Response {
        request_id: String,
        result: Result<serde_json::Value, String>,
        snapshot: Box<SessionSnapshot>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBridgeRequest {
    pub request_id: String,
    pub input: serde_json::Value,
}

#[derive(Debug)]
pub struct SessionRegistrationClient {
    state: Arc<Mutex<PublishedState>>,
    outbound: SyncSender<RegistrationOutbound>,
    requests: Mutex<mpsc::Receiver<SessionBridgeRequest>>,
    stop: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl SessionRegistrationClient {
    pub fn start(
        address: SocketAddr,
        registration: SessionRegistration,
        snapshot: SessionSnapshot,
        session_path: Option<String>,
    ) -> io::Result<Self> {
        write_session_frame(
            &mut io::sink(),
            &ClientSessionFrame::Register {
                version: SESSION_REGISTRATION_VERSION,
                registration: registration.clone(),
                snapshot: snapshot.clone(),
                session_path: session_path.clone(),
            },
        )?;
        let state = Arc::new(Mutex::new(PublishedState {
            registration,
            snapshot,
            session_path,
        }));
        let (outbound, receiver) = mpsc::sync_channel(32);
        let (request_sender, requests) = mpsc::sync_channel(32);
        let stop = Arc::new(AtomicBool::new(false));
        let connected = Arc::new(AtomicBool::new(false));
        let worker_state = Arc::clone(&state);
        let worker_stop = Arc::clone(&stop);
        let worker_connected = Arc::clone(&connected);
        let thread = thread::Builder::new()
            .name("pdiff-session-registration".into())
            .spawn(move || {
                let mut backoff = Duration::from_millis(25);
                while !worker_stop.load(Ordering::Acquire) {
                    match run_connection(
                        address,
                        &worker_state,
                        &receiver,
                        &worker_stop,
                        &worker_connected,
                        &request_sender,
                    ) {
                        Ok(()) if worker_stop.load(Ordering::Acquire) => break,
                        _ => {
                            worker_connected.store(false, Ordering::Release);
                            thread::sleep(backoff);
                            backoff = (backoff * 2).min(Duration::from_millis(500));
                        }
                    }
                }
                worker_connected.store(false, Ordering::Release);
            })
            .expect("session registration thread can start");
        Ok(Self {
            state,
            outbound,
            requests: Mutex::new(requests),
            stop,
            connected,
            thread: Mutex::new(Some(thread)),
        })
    }

    pub fn publish_snapshot(&self, snapshot: SessionSnapshot) {
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = snapshot;
        }
        let _ = self.outbound.try_send(RegistrationOutbound::Publish);
    }

    pub fn publish_registration(&self, registration: SessionRegistration) {
        if let Ok(mut state) = self.state.lock() {
            state.registration = registration;
        }
        let _ = self.outbound.try_send(RegistrationOutbound::Publish);
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    pub fn try_recv_request(&self) -> Option<SessionBridgeRequest> {
        self.requests.lock().ok()?.try_recv().ok()
    }

    pub fn respond(
        &self,
        request_id: String,
        result: Result<serde_json::Value, String>,
        snapshot: SessionSnapshot,
    ) -> Result<(), String> {
        if let Ok(mut state) = self.state.lock() {
            state.snapshot.clone_from(&snapshot);
        }
        self.outbound
            .try_send(RegistrationOutbound::Response {
                request_id,
                result,
                snapshot: Box::new(snapshot),
            })
            .map_err(|_| "session response queue is full or disconnected".into())
    }
}

pub fn create_session_descriptor(
    input: &ReviewInput,
    loaded: &LoadedReview,
    cwd: &std::path::Path,
) -> super::SessionDescriptor {
    let cwd = canonical_string(cwd);
    let repo_root = match &loaded.reload_plan {
        ReloadPlan::Vcs { repo_root, .. } => Some(canonical_string(repo_root)),
        _ => None,
    };
    let launched_at = session_timestamp();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    super::SessionDescriptor {
        session_id: format!("pdiff-{}-{nonce:x}", std::process::id()),
        pid: std::process::id(),
        cwd,
        repo_root,
        launched_at,
        input_kind: input_kind_name(input.kind()).into(),
        title: loaded.changeset.title.clone(),
        source_label: loaded.changeset.source_label.clone(),
    }
}

pub fn session_timestamp() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = duration.as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        duration.subsec_millis()
    )
}

pub fn current_session_path() -> Option<String> {
    std::env::var("PDIFF_SESSION_PATH")
        .ok()
        .filter(|path| !path.trim().is_empty())
        .or_else(platform_tty_path)
}

fn canonical_string(path: &std::path::Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn input_kind_name(kind: InputKind) -> &'static str {
    match kind {
        InputKind::Diff => "diff",
        InputKind::Show => "show",
        InputKind::StashShow => "stash-show",
        InputKind::Patch => "patch",
        InputKind::Pager => "pager",
        InputKind::Difftool => "difftool",
    }
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_epoch + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

#[cfg(unix)]
fn platform_tty_path() -> Option<String> {
    let mut buffer = [0_i8; 1024];
    let result = unsafe { libc::ttyname_r(libc::STDIN_FILENO, buffer.as_mut_ptr(), buffer.len()) };
    if result != 0 {
        return None;
    }
    let value = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) };
    value.to_str().ok().map(str::to_owned)
}

#[cfg(windows)]
fn platform_tty_path() -> Option<String> {
    None
}

impl Drop for SessionRegistrationClient {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.outbound.try_send(RegistrationOutbound::Publish);
        if let Some(thread) = self.thread.lock().ok().and_then(|mut thread| thread.take()) {
            let _ = thread.join();
        }
    }
}

fn run_connection(
    address: SocketAddr,
    state: &Arc<Mutex<PublishedState>>,
    receiver: &mpsc::Receiver<RegistrationOutbound>,
    stop: &AtomicBool,
    connected: &AtomicBool,
    requests: &SyncSender<SessionBridgeRequest>,
) -> io::Result<()> {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(500))?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    stream.write_all(SESSION_WIRE_PREFACE)?;
    let initial = state
        .lock()
        .map_err(|_| io::Error::other("session publication state is poisoned"))?
        .clone();
    write_session_frame(
        &mut stream,
        &ClientSessionFrame::Register {
            version: SESSION_REGISTRATION_VERSION,
            registration: initial.registration,
            snapshot: initial.snapshot,
            session_path: initial.session_path,
        },
    )?;
    match read_session_frame::<_, ServerSessionFrame>(&mut stream)? {
        ServerSessionFrame::Registered { .. } => {}
        ServerSessionFrame::Error { message, .. } => return Err(io::Error::other(message)),
        _ => return Err(io::Error::other("daemon did not acknowledge registration")),
    }
    connected.store(true, Ordering::Release);
    stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    let mut last_ping = Instant::now();
    loop {
        if stop.load(Ordering::Acquire) {
            let _ = write_session_frame(&mut stream, &ClientSessionFrame::Unregister);
            return Ok(());
        }
        match receiver.recv_timeout(Duration::from_millis(10)) {
            Ok(RegistrationOutbound::Publish) => {
                if stop.load(Ordering::Acquire) {
                    continue;
                }
                let published = state
                    .lock()
                    .map_err(|_| io::Error::other("session publication state is poisoned"))?
                    .clone();
                write_session_frame(
                    &mut stream,
                    &ClientSessionFrame::Registration {
                        version: SESSION_REGISTRATION_VERSION,
                        registration: published.registration,
                    },
                )?;
                write_session_frame(
                    &mut stream,
                    &ClientSessionFrame::Snapshot {
                        version: SESSION_REGISTRATION_VERSION,
                        snapshot: published.snapshot,
                    },
                )?;
            }
            Ok(RegistrationOutbound::Response {
                request_id,
                result,
                snapshot,
            }) => {
                write_session_frame(
                    &mut stream,
                    &ClientSessionFrame::CommandResult {
                        version: SESSION_REGISTRATION_VERSION,
                        request_id,
                        result,
                        snapshot: *snapshot,
                    },
                )?;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        if last_ping.elapsed() >= Duration::from_millis(500) {
            write_session_frame(&mut stream, &ClientSessionFrame::Ping)?;
            last_ping = Instant::now();
        }
        match read_session_frame::<_, ServerSessionFrame>(&mut stream) {
            Ok(ServerSessionFrame::Command {
                request_id, input, ..
            }) => {
                if requests
                    .try_send(SessionBridgeRequest {
                        request_id: request_id.clone(),
                        input,
                    })
                    .is_err()
                {
                    let snapshot = state
                        .lock()
                        .map_err(|_| io::Error::other("session publication state is poisoned"))?
                        .snapshot
                        .clone();
                    write_session_frame(
                        &mut stream,
                        &ClientSessionFrame::CommandResult {
                            version: SESSION_REGISTRATION_VERSION,
                            request_id,
                            result: Err("TUI session command queue is full".into()),
                            snapshot,
                        },
                    )?;
                }
            }
            Ok(ServerSessionFrame::Pong) | Ok(ServerSessionFrame::Registered { .. }) => {}
            Ok(ServerSessionFrame::Error { message, .. }) => {
                return Err(io::Error::other(message));
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(error) => return Err(error),
        }
    }
}
