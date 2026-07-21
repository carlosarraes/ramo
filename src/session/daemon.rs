use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{SessionAddress, SessionRegistration, SessionSelector, SessionSnapshot};

#[derive(Debug, Clone)]
pub struct RegisteredSession {
    pub registration: SessionRegistration,
    pub snapshot: SessionSnapshot,
    pub session_path: Option<String>,
    pub generation: u64,
    pub pending_request_ids: BTreeSet<String>,
    last_seen: Instant,
}

#[derive(Debug, Default)]
pub struct SessionRegistry {
    sessions: BTreeMap<String, RegisteredSession>,
    next_generation: u64,
}

impl SessionRegistry {
    pub fn register(
        &mut self,
        registration: SessionRegistration,
        snapshot: SessionSnapshot,
        session_path: Option<String>,
    ) -> u64 {
        self.next_generation = self.next_generation.saturating_add(1);
        let generation = self.next_generation;
        let session_id = registration.descriptor.session_id.clone();
        self.sessions.insert(
            session_id,
            RegisteredSession {
                registration,
                snapshot,
                session_path,
                generation,
                pending_request_ids: BTreeSet::new(),
                last_seen: Instant::now(),
            },
        );
        generation
    }

    pub fn unregister_generation(&mut self, session_id: &str, generation: u64) -> bool {
        if self
            .sessions
            .get(session_id)
            .is_some_and(|session| session.generation == generation)
        {
            self.sessions.remove(session_id);
            true
        } else {
            false
        }
    }

    pub fn update_snapshot(
        &mut self,
        session_id: &str,
        generation: u64,
        snapshot: SessionSnapshot,
    ) -> bool {
        let Some(session) = self.sessions.get_mut(session_id) else {
            return false;
        };
        if session.generation != generation {
            return false;
        }
        session.snapshot = snapshot;
        session.last_seen = Instant::now();
        true
    }

    pub fn update_registration(
        &mut self,
        session_id: &str,
        generation: u64,
        registration: SessionRegistration,
    ) -> bool {
        let Some(session) = self.sessions.get_mut(session_id) else {
            return false;
        };
        if session.generation != generation
            || registration.descriptor.session_id != session_id
            || registration.registration_version != super::SESSION_REGISTRATION_VERSION
        {
            return false;
        }
        session.registration = registration;
        session.last_seen = Instant::now();
        true
    }

    pub fn list(&self) -> Vec<RegisteredSession> {
        self.sessions.values().cloned().collect()
    }

    pub fn select(&self, selector: &SessionSelector) -> Result<RegisteredSession, String> {
        let matches: Vec<_> = self
            .sessions
            .values()
            .filter(|session| selector_matches(session, selector))
            .cloned()
            .collect();
        match matches.as_slice() {
            [session] => Ok(session.clone()),
            [] => Err(format!(
                "No live pdiff session matches {}.",
                describe_selector(selector)
            )),
            sessions => Err(format!(
                "{} live pdiff sessions match {}; select one by session id.",
                sessions.len(),
                describe_selector(selector)
            )),
        }
    }

    pub fn begin_request(
        &mut self,
        selector: &SessionSelector,
        request_id: &str,
    ) -> Result<(String, u64), String> {
        let selected = self.select(selector)?;
        let session_id = selected.registration.descriptor.session_id;
        let session = self
            .sessions
            .get_mut(&session_id)
            .expect("selected session exists");
        session.pending_request_ids.insert(request_id.to_owned());
        session.last_seen = Instant::now();
        Ok((session_id, session.generation))
    }

    pub fn complete_request(
        &mut self,
        session_id: &str,
        generation: u64,
        request_id: &str,
    ) -> bool {
        self.sessions
            .get_mut(session_id)
            .filter(|session| session.generation == generation)
            .is_some_and(|session| session.pending_request_ids.remove(request_id))
    }

    pub fn prune_stale(&mut self, ttl: Duration) -> usize {
        let before = self.sessions.len();
        self.sessions
            .retain(|_, session| session.last_seen.elapsed() <= ttl);
        before - self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

fn selector_matches(session: &RegisteredSession, selector: &SessionSelector) -> bool {
    if let Some(session_id) = &selector.session_id {
        return session.registration.descriptor.session_id == *session_id;
    }
    if let Some(repo_root) = &selector.repo_root {
        return session.registration.descriptor.repo_root.as_ref() == Some(repo_root);
    }
    if let Some(session_path) = &selector.session_path {
        return session.session_path.as_ref() == Some(session_path);
    }
    false
}

fn describe_selector(selector: &SessionSelector) -> String {
    if let Some(session_id) = &selector.session_id {
        format!("session id {session_id:?}")
    } else if let Some(repo_root) = &selector.repo_root {
        format!("repository {repo_root:?}")
    } else if let Some(session_path) = &selector.session_path {
        format!("session path {session_path:?}")
    } else {
        "the empty selector".into()
    }
}

#[derive(Debug, Clone)]
pub struct SessionDaemonOptions {
    pub address: SessionAddress,
    pub idle_timeout: Duration,
    pub stale_session_ttl: Duration,
}

impl Default for SessionDaemonOptions {
    fn default() -> Self {
        Self {
            address: SessionAddress::parse(
                super::DEFAULT_SESSION_HOST,
                super::DEFAULT_SESSION_PORT,
            )
            .expect("default session address is valid"),
            idle_timeout: Duration::from_secs(60),
            stale_session_ttl: Duration::from_secs(45),
        }
    }
}

#[derive(Debug)]
struct DaemonState {
    stopped: bool,
    last_activity: Instant,
}

#[derive(Debug)]
pub struct SessionDaemonHandle {
    address: SocketAddr,
    registry: Arc<Mutex<SessionRegistry>>,
    state: Arc<(Mutex<DaemonState>, Condvar)>,
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl SessionDaemonHandle {
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn registry(&self) -> Arc<Mutex<SessionRegistry>> {
        Arc::clone(&self.registry)
    }

    pub fn wait_timeout(&self, timeout: Duration) -> bool {
        let (lock, condition) = &*self.state;
        let state = lock.lock().unwrap_or_else(|poison| poison.into_inner());
        condition
            .wait_timeout_while(state, timeout, |state| !state.stopped)
            .map(|(state, _)| state.stopped)
            .unwrap_or(true)
    }

    pub fn wait(self) {
        let (lock, condition) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|poison| poison.into_inner());
        while !state.stopped {
            state = condition
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_millis(50));
    }
}

impl Drop for SessionDaemonHandle {
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = self.thread.lock().ok().and_then(|mut thread| thread.take()) {
            let _ = thread.join();
        }
    }
}

pub fn spawn_session_daemon(options: SessionDaemonOptions) -> io::Result<SessionDaemonHandle> {
    let requested = options.address.socket_addr();
    let listener = TcpListener::bind(requested).map_err(|error| {
        let message = if error.kind() == io::ErrorKind::AddrInUse {
            format!(
                "cannot start the pdiff session broker on {requested}: address is already in use"
            )
        } else {
            format!("cannot start the pdiff session broker on {requested}: {error}")
        };
        io::Error::new(error.kind(), message)
    })?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let registry = Arc::new(Mutex::new(SessionRegistry::default()));
    let state = Arc::new((
        Mutex::new(DaemonState {
            stopped: false,
            last_activity: Instant::now(),
        }),
        Condvar::new(),
    ));
    let stop = Arc::new(AtomicBool::new(false));
    let daemon_registry = Arc::clone(&registry);
    let daemon_state = Arc::clone(&state);
    let daemon_stop = Arc::clone(&stop);
    let thread = thread::Builder::new()
        .name("pdiff-session-daemon".into())
        .spawn(move || {
            serve_loop(
                listener,
                address.port(),
                daemon_registry,
                daemon_state,
                daemon_stop,
                options,
            );
        })?;
    Ok(SessionDaemonHandle {
        address,
        registry,
        state,
        stop,
        thread: Mutex::new(Some(thread)),
    })
}

fn serve_loop(
    listener: TcpListener,
    port: u16,
    registry: Arc<Mutex<SessionRegistry>>,
    state: Arc<(Mutex<DaemonState>, Condvar)>,
    stop: Arc<AtomicBool>,
    options: SessionDaemonOptions,
) {
    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }
        match listener.accept() {
            Ok((stream, peer)) => {
                if !peer.ip().is_loopback() {
                    continue;
                }
                let registry = Arc::clone(&registry);
                let state = Arc::clone(&state);
                let stop = Arc::clone(&stop);
                {
                    let (lock, _) = &*state;
                    let mut daemon = lock.lock().unwrap_or_else(|poison| poison.into_inner());
                    daemon.last_activity = Instant::now();
                }
                let _ = thread::Builder::new()
                    .name("pdiff-session-http".into())
                    .spawn(move || {
                        if super::wire::connection_uses_session_wire(&stream).unwrap_or(false) {
                            serve_wire_connection(stream, &registry, &stop);
                        } else {
                            super::http::serve_http_connection(stream, port, &registry, &stop);
                        }
                        let (lock, _) = &*state;
                        let mut daemon = lock.lock().unwrap_or_else(|poison| poison.into_inner());
                        daemon.last_activity = Instant::now();
                    });
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break,
        }
        let idle = {
            let (lock, _) = &*state;
            let daemon = lock.lock().unwrap_or_else(|poison| poison.into_inner());
            daemon.last_activity.elapsed() >= options.idle_timeout
        };
        if let Ok(mut registry) = registry.lock() {
            registry.prune_stale(options.stale_session_ttl);
            if idle && registry.is_empty() {
                break;
            }
        }
    }
    let (lock, condition) = &*state;
    let mut daemon = lock.lock().unwrap_or_else(|poison| poison.into_inner());
    daemon.stopped = true;
    condition.notify_all();
}

fn serve_wire_connection(
    mut stream: TcpStream,
    registry: &Arc<Mutex<SessionRegistry>>,
    stop: &Arc<AtomicBool>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let mut preface = [0_u8; super::SESSION_WIRE_PREFACE.len()];
    if stream.read_exact(&mut preface).is_err() || preface != super::SESSION_WIRE_PREFACE {
        return;
    }
    let first = match super::read_session_frame::<_, super::ClientSessionFrame>(&mut stream) {
        Ok(super::ClientSessionFrame::Register {
            registration,
            snapshot,
            session_path,
            ..
        }) if registration.registration_version == super::SESSION_REGISTRATION_VERSION => {
            (registration, snapshot, session_path)
        }
        _ => {
            let _ = super::write_session_frame(
                &mut stream,
                &super::ServerSessionFrame::Error {
                    version: super::SESSION_REGISTRATION_VERSION,
                    message: "first session frame must be a compatible registration".into(),
                },
            );
            return;
        }
    };
    let session_id = first.0.descriptor.session_id.clone();
    let generation = match registry.lock() {
        Ok(mut registry) => registry.register(first.0, first.1, first.2),
        Err(_) => return,
    };
    if super::write_session_frame(
        &mut stream,
        &super::ServerSessionFrame::Registered {
            version: super::SESSION_REGISTRATION_VERSION,
            generation,
        },
    )
    .is_err()
    {
        if let Ok(mut registry) = registry.lock() {
            registry.unregister_generation(&session_id, generation);
        }
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
    while !stop.load(Ordering::Acquire) {
        match super::read_session_frame::<_, super::ClientSessionFrame>(&mut stream) {
            Ok(super::ClientSessionFrame::Snapshot { snapshot, .. }) => {
                if let Ok(mut registry) = registry.lock() {
                    registry.update_snapshot(&session_id, generation, snapshot);
                }
            }
            Ok(super::ClientSessionFrame::Registration { registration, .. }) => {
                if let Ok(mut registry) = registry.lock() {
                    registry.update_registration(&session_id, generation, registration);
                }
            }
            Ok(super::ClientSessionFrame::Ping) => {
                if super::write_session_frame(&mut stream, &super::ServerSessionFrame::Pong)
                    .is_err()
                {
                    break;
                }
            }
            Ok(super::ClientSessionFrame::Unregister) => break,
            Ok(super::ClientSessionFrame::CommandResult { .. })
            | Ok(super::ClientSessionFrame::Register { .. }) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    if let Ok(mut registry) = registry.lock() {
        registry.unregister_generation(&session_id, generation);
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
}
