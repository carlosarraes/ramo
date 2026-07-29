use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::Duration;

use ramo_core::review_map::{ReviewMap, ReviewMapFailureCode, ReviewMapStatus};

use super::{
    ReviewMapClientError, ReviewMapFailureNotice, ReviewMapResolveRequest, ReviewMapService,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewMapUpdate {
    Analyzing,
    Enriched(Box<ReviewMap>),
    Failed(ReviewMapFailureNotice),
    Unavailable(ReviewMapFailureNotice),
    Stale(ReviewMapFailureNotice),
}

pub struct ReviewMapRuntime {
    receiver: Receiver<ReviewMapUpdate>,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ReviewMapRuntime {
    pub fn start<C>(client: C, request: ReviewMapResolveRequest) -> Self
    where
        C: ReviewMapService,
    {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker = std::thread::spawn(move || {
            let mut response = match client.resolve(&request) {
                Ok(response) => response,
                Err(error) => {
                    let _ = sender.send(error_update(error));
                    return;
                }
            };
            let delays = [250_u64, 500, 1_000, 2_000];
            let mut delay_index = 0;
            loop {
                if worker_cancelled.load(Ordering::Acquire) {
                    return;
                }
                match response.state {
                    ReviewMapStatus::Ready | ReviewMapStatus::Analyzing => {
                        let _ = sender.send(ReviewMapUpdate::Analyzing);
                    }
                    ReviewMapStatus::Enriched => {
                        let _ = sender.send(ReviewMapUpdate::Enriched(Box::new(response.map)));
                        return;
                    }
                    ReviewMapStatus::Stale => {
                        let _ = sender.send(ReviewMapUpdate::Stale(notice(
                            response.failure,
                            ReviewMapFailureCode::ResultStale,
                            "The pull request changed while it was being analyzed",
                        )));
                        return;
                    }
                    ReviewMapStatus::Unavailable => {
                        let _ = sender.send(ReviewMapUpdate::Unavailable(notice(
                            response.failure,
                            ReviewMapFailureCode::ServerUnreachable,
                            "Local Review Map analysis is unavailable",
                        )));
                        return;
                    }
                    ReviewMapStatus::Failed => {
                        let _ = sender.send(ReviewMapUpdate::Failed(notice(
                            response.failure,
                            ReviewMapFailureCode::AnalysisFailed,
                            "Local Review Map analysis failed",
                        )));
                        return;
                    }
                }
                if cancellable_wait(
                    &worker_cancelled,
                    Duration::from_millis(delays[delay_index]),
                ) {
                    return;
                }
                delay_index = (delay_index + 1).min(delays.len() - 1);
                response = match client.poll(&response.job_id) {
                    Ok(response) => response,
                    Err(error) => {
                        let _ = sender.send(error_update(error));
                        return;
                    }
                };
            }
        });
        Self {
            receiver,
            cancelled,
            worker: Some(worker),
        }
    }

    pub fn try_recv(&self) -> Option<ReviewMapUpdate> {
        self.receiver.try_recv().ok()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Option<ReviewMapUpdate> {
        self.receiver.recv_timeout(timeout).ok()
    }
}

impl Drop for ReviewMapRuntime {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn cancellable_wait(cancelled: &AtomicBool, duration: Duration) -> bool {
    let interval = Duration::from_millis(25);
    let started = std::time::Instant::now();
    while started.elapsed() < duration {
        if cancelled.load(Ordering::Acquire) {
            return true;
        }
        std::thread::sleep(interval.min(duration.saturating_sub(started.elapsed())));
    }
    cancelled.load(Ordering::Acquire)
}

fn error_update(error: ReviewMapClientError) -> ReviewMapUpdate {
    let code = error.code();
    let notice = ReviewMapFailureNotice {
        code,
        message: error.message().to_owned(),
    };
    match code {
        ReviewMapFailureCode::ResultStale => ReviewMapUpdate::Stale(notice),
        ReviewMapFailureCode::ServerUnreachable
        | ReviewMapFailureCode::ClientUnauthorized
        | ReviewMapFailureCode::PairingRejected
        | ReviewMapFailureCode::OllamaUnavailable
        | ReviewMapFailureCode::ModelMissing => ReviewMapUpdate::Unavailable(notice),
        _ => ReviewMapUpdate::Failed(notice),
    }
}

fn notice(
    failure: Option<ReviewMapClientError>,
    fallback_code: ReviewMapFailureCode,
    fallback_message: &str,
) -> ReviewMapFailureNotice {
    failure.map_or(
        ReviewMapFailureNotice {
            code: fallback_code,
            message: fallback_message.into(),
        },
        |failure| ReviewMapFailureNotice {
            code: failure.code(),
            message: failure.message().to_owned(),
        },
    )
}
