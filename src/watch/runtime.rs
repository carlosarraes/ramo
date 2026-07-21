use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::config::ResolvedConfig;
use crate::core::changeset::Changeset;
use crate::diff::model::DiffFile;
use crate::input::{LoadContext, LoadedReview, ReloadPlan, ReviewLoader};
use crate::notes::AgentContextSource;
use crate::vcs::SystemCommandRunner;

use super::{Coverage, NativeObserver, WatchCoordinator, WatchPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchIntervals {
    pub quiet: Duration,
    pub maximum: Duration,
    pub safety: Duration,
}

impl Default for WatchIntervals {
    fn default() -> Self {
        Self {
            quiet: Duration::from_millis(200),
            maximum: Duration::from_secs(1),
            safety: Duration::from_secs(10),
        }
    }
}

#[derive(Debug)]
pub enum WatchUpdate {
    Unchanged,
    Replaced {
        files: Vec<DiffFile>,
        generation: u64,
    },
    Empty {
        generation: u64,
    },
    Error {
        message: String,
    },
}

pub struct WatchRuntime {
    reload_plan: ReloadPlan,
    agent_context: AgentContextSource,
    cwd: PathBuf,
    config: ResolvedConfig,
    coordinator: WatchCoordinator,
    observer: Option<NativeObserver>,
    automatic_enabled: bool,
    manual_pending: bool,
    pending_error: Option<String>,
    last_reported_error: Option<(String, Instant)>,
    applied_fingerprint: u64,
}

impl WatchRuntime {
    pub fn new(
        initial: &LoadedReview,
        cwd: PathBuf,
        config: ResolvedConfig,
        watch_enabled: bool,
        start: Instant,
    ) -> Self {
        let mut intervals = WatchIntervals::default();
        if WatchPlan::from_reload_plan(&initial.reload_plan, &cwd)
            .is_some_and(|plan| plan.coverage == Coverage::PollOnly)
        {
            intervals.safety = Duration::from_secs(2);
        }
        Self::with_intervals(initial, cwd, config, watch_enabled, start, intervals)
    }

    pub fn with_intervals(
        initial: &LoadedReview,
        cwd: PathBuf,
        config: ResolvedConfig,
        watch_enabled: bool,
        start: Instant,
        intervals: WatchIntervals,
    ) -> Self {
        let plan = WatchPlan::from_reload_plan(&initial.reload_plan, &cwd);
        let (observer, pending_error) = if watch_enabled
            && plan
                .as_ref()
                .is_some_and(|plan| plan.coverage == Coverage::Hybrid)
        {
            match NativeObserver::start(plan.as_ref().expect("hybrid plan exists")) {
                Ok(observer) => (Some(observer), None),
                Err(error) => (
                    None,
                    Some(format!(
                        "filesystem observation unavailable; polling instead: {error}"
                    )),
                ),
            }
        } else {
            (None, None)
        };
        let safety_interval = if pending_error.is_some() {
            intervals.safety.min(Duration::from_secs(2))
        } else {
            intervals.safety
        };
        Self {
            reload_plan: initial.reload_plan.clone(),
            agent_context: initial.agent_context.clone(),
            cwd,
            config,
            coordinator: WatchCoordinator::with_safety_interval(
                start,
                intervals.quiet,
                intervals.maximum,
                safety_interval,
            ),
            observer,
            automatic_enabled: watch_enabled,
            manual_pending: false,
            pending_error,
            last_reported_error: None,
            applied_fingerprint: fingerprint(&initial.changeset),
        }
    }

    pub fn manual_reload(&mut self, now: Instant) {
        self.manual_pending = true;
        self.coordinator.manual_hint(now);
    }

    pub fn poll(&mut self, now: Instant) -> WatchUpdate {
        if let Some(observer) = &mut self.observer {
            let observed = observer.poll();
            if observed.changed {
                self.coordinator.event_hint(now);
            }
            if observed.error.is_some() {
                self.pending_error = observed.error;
            }
        }

        if !self.automatic_enabled && !self.manual_pending {
            return match self.pending_error.take() {
                Some(message) => self.report_error(message, now),
                None => WatchUpdate::Unchanged,
            };
        }
        let Some(generation) = self.coordinator.tick(now) else {
            return match self.pending_error.take() {
                Some(message) => self.report_error(message, now),
                None => WatchUpdate::Unchanged,
            };
        };
        self.manual_pending = false;
        let runner = SystemCommandRunner;
        let context = LoadContext {
            cwd: &self.cwd,
            config: &self.config,
            runner: &runner,
        };
        let loaded =
            ReviewLoader.reload_with_agent(&self.reload_plan, &self.agent_context, &context);
        let accepted = self.coordinator.accept_result(generation);
        self.coordinator.finish(generation, now);
        if !accepted {
            return WatchUpdate::Unchanged;
        }
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                return self.report_error(error.to_string(), now);
            }
        };
        let next_fingerprint = fingerprint(&loaded.changeset);
        if next_fingerprint == self.applied_fingerprint {
            return WatchUpdate::Unchanged;
        }
        self.applied_fingerprint = next_fingerprint;
        self.reload_plan = loaded.reload_plan;
        self.agent_context = loaded.agent_context;
        if loaded.changeset.files.is_empty() {
            WatchUpdate::Empty { generation }
        } else {
            WatchUpdate::Replaced {
                files: loaded.changeset.files,
                generation,
            }
        }
    }

    fn report_error(&mut self, message: String, now: Instant) -> WatchUpdate {
        const DUPLICATE_INTERVAL: Duration = Duration::from_secs(10);
        if self
            .last_reported_error
            .as_ref()
            .is_some_and(|(previous, reported_at)| {
                previous == &message
                    && now.saturating_duration_since(*reported_at) < DUPLICATE_INTERVAL
            })
        {
            return WatchUpdate::Unchanged;
        }
        self.last_reported_error = Some((message.clone(), now));
        WatchUpdate::Error { message }
    }
}

fn fingerprint(changeset: &Changeset) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325_u64;
    let mut hash = |part: &[u8]| {
        for byte in part {
            value ^= u64::from(*byte);
            value = value.wrapping_mul(0x0000_0100_0000_01b3);
        }
        value ^= 0xff;
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    };
    hash(changeset.source_label.as_bytes());
    hash(changeset.title.as_bytes());
    hash(changeset.agent_summary.as_deref().unwrap_or("").as_bytes());
    for file in &changeset.files {
        hash(file.id.as_bytes());
        hash(file.path.as_bytes());
        hash(file.patch.as_bytes());
        let Some(agent) = &file.agent else {
            hash(b"no-agent-context");
            continue;
        };
        hash(agent.path.as_bytes());
        hash(agent.summary.as_deref().unwrap_or("").as_bytes());
        for note in &agent.annotations {
            hash(note.id.as_deref().unwrap_or("").as_bytes());
            hash(note.summary.as_bytes());
            hash(note.rationale.as_deref().unwrap_or("").as_bytes());
            hash(note.markup.as_deref().unwrap_or("").as_bytes());
            hash(note.source.as_str().as_bytes());
            for range in [note.old_range, note.new_range].into_iter().flatten() {
                hash(&range.start.to_le_bytes());
                hash(&range.end.to_le_bytes());
            }
            for tag in &note.tags {
                hash(tag.as_bytes());
            }
        }
    }
    value
}
