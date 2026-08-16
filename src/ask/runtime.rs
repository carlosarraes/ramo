use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use super::pi::AskError;

/// Each in-flight question holds a `pi` process running a max-reasoning request, so the
/// cap is about provider load and wall-clock sanity rather than local CPU.
pub const MAX_CONCURRENT_ASKS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AskId(pub u64);

#[derive(Debug)]
pub enum AskUpdate {
    Answered { id: AskId, body: String },
    Failed { id: AskId, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AskBusy;

impl std::fmt::Display for AskBusy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{MAX_CONCURRENT_ASKS} AI questions are already running; wait for one to finish"
        )
    }
}

impl std::error::Error for AskBusy {}

pub struct AskRuntime {
    sender: Sender<AskUpdate>,
    receiver: Receiver<AskUpdate>,
    cancelled: Arc<AtomicBool>,
    workers: Vec<(AskId, JoinHandle<()>)>,
    next_id: u64,
}

impl Default for AskRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AskRuntime {
    pub fn new() -> Self {
        let (sender, receiver) = channel();
        Self {
            sender,
            receiver,
            cancelled: Arc::new(AtomicBool::new(false)),
            workers: Vec::new(),
            next_id: 1,
        }
    }

    /// Spawns `job` on its own thread. The job is a closure rather than a `PiCli` so the
    /// async layer stays testable without spawning a subprocess.
    pub fn start<F>(&mut self, job: F) -> Result<AskId, AskBusy>
    where
        F: FnOnce() -> Result<String, AskError> + Send + 'static,
    {
        self.reap();
        if self.in_flight() >= MAX_CONCURRENT_ASKS {
            return Err(AskBusy);
        }
        let id = AskId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let sender = self.sender.clone();
        let cancelled = Arc::clone(&self.cancelled);
        let worker = std::thread::Builder::new()
            .name(format!("ramo-ask-{}", id.0))
            .spawn(move || {
                let update = match job() {
                    Ok(body) => AskUpdate::Answered { id, body },
                    Err(error) => AskUpdate::Failed {
                        id,
                        message: error.to_string(),
                    },
                };
                if !cancelled.load(Ordering::Acquire) {
                    let _ = sender.send(update);
                }
            })
            .map_err(|_| AskBusy)?;
        self.workers.push((id, worker));
        Ok(id)
    }

    pub fn try_recv(&self) -> Option<AskUpdate> {
        self.receiver.try_recv().ok()
    }

    pub fn in_flight(&self) -> usize {
        self.workers.len()
    }

    /// Drops finished worker handles so their slots become available again.
    pub fn reap(&mut self) {
        self.workers.retain(|(_, worker)| !worker.is_finished());
    }
}

impl Drop for AskRuntime {
    /// Deliberately does not join, unlike `ReviewMapRuntime`. A worker is blocked inside
    /// the command executor's deadline loop, which cannot be interrupted, so joining would
    /// stall quit for up to `ask_timeout_secs`. The cost is that quitting mid-question can
    /// leave up to `MAX_CONCURRENT_ASKS` short-lived `pi` children, each killed by that
    /// same executor deadline.
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn drain(runtime: &AskRuntime, expected: usize) -> Vec<AskUpdate> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut updates = Vec::new();
        while updates.len() < expected && Instant::now() < deadline {
            match runtime.try_recv() {
                Some(update) => updates.push(update),
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        updates
    }

    #[test]
    fn ids_are_unique_and_every_job_reports() {
        let mut runtime = AskRuntime::new();
        let first = runtime.start(|| Ok("one".into())).unwrap();
        let second = runtime.start(|| Ok("two".into())).unwrap();
        assert_ne!(first, second);

        let updates = drain(&runtime, 2);
        assert_eq!(updates.len(), 2);
        assert!(
            updates
                .iter()
                .all(|update| matches!(update, AskUpdate::Answered { .. }))
        );
    }

    #[test]
    fn failures_are_reported_with_their_message() {
        let mut runtime = AskRuntime::new();
        runtime.start(|| Err(AskError::MissingCli)).unwrap();

        let updates = drain(&runtime, 1);
        match updates.first() {
            Some(AskUpdate::Failed { message, .. }) => {
                assert!(message.contains("pi was not found"), "{message}");
            }
            other => panic!("expected a failure update, got {other:?}"),
        }
    }

    #[test]
    fn the_concurrency_cap_holds_until_a_slot_is_reaped() {
        let mut runtime = AskRuntime::new();
        for _ in 0..MAX_CONCURRENT_ASKS {
            runtime
                .start(|| {
                    std::thread::sleep(Duration::from_millis(200));
                    Ok("slow".into())
                })
                .unwrap();
        }
        assert_eq!(runtime.start(|| Ok("extra".into())), Err(AskBusy));

        drain(&runtime, MAX_CONCURRENT_ASKS);
        let deadline = Instant::now() + Duration::from_secs(5);
        while runtime.in_flight() > 0 && Instant::now() < deadline {
            runtime.reap();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(runtime.start(|| Ok("now fits".into())).is_ok());
    }

    #[test]
    fn dropping_with_a_question_in_flight_does_not_block_quit() {
        let started = Instant::now();
        {
            let mut runtime = AskRuntime::new();
            runtime
                .start(|| {
                    std::thread::sleep(Duration::from_secs(2));
                    Ok("late".into())
                })
                .unwrap();
        }
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "drop must not join the worker: took {:?}",
            started.elapsed()
        );
    }
}
