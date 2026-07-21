use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::{ConfigPaths, ConfigResolver, ResolvedConfig};
use crate::core::changeset::Changeset;
use crate::core::input::{PatchSource, ReviewInput};
use crate::diff::model::DiffFile;
use crate::input::{LoadContext, LoadedReview, ReloadPlan, ReviewLoader};
use crate::notes::AgentContextSource;
use crate::vcs::{CommandRunner, SystemCommandRunner};

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
    reload_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ReplacedReviewInput {
    pub input: ReviewInput,
    pub loaded: LoadedReview,
    pub cwd: PathBuf,
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
        let reload_roots = initial_reload_roots(initial, &cwd);
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
            reload_roots,
        }
    }

    pub fn replace_input(
        &mut self,
        input: ReviewInput,
        source_path: Option<&Path>,
        now: Instant,
    ) -> Result<ReplacedReviewInput, String> {
        self.replace_input_with_runner(input, source_path, now, &SystemCommandRunner)
    }

    pub fn replace_input_with_runner(
        &mut self,
        input: ReviewInput,
        source_path: Option<&Path>,
        now: Instant,
        runner: &dyn CommandRunner,
    ) -> Result<ReplacedReviewInput, String> {
        let cwd = self.validate_source_path(source_path)?;
        let input = normalize_reload_input(input, &cwd, &self.reload_roots)?;
        let config = ConfigResolver::new(ConfigPaths::discover(&cwd))
            .resolve(&input)
            .map_err(|error| error.to_string())?;
        let context = LoadContext {
            cwd: &cwd,
            config: &config,
            runner,
        };
        let loaded = ReviewLoader
            .load_with_context(&input, &mut std::io::empty(), &context)
            .map_err(|error| error.to_string())?;
        if matches!(loaded.reload_plan, ReloadPlan::None) {
            return Err("session reload requires a repeatable review input".into());
        }

        let reload_roots = self.reload_roots.clone();
        let mut replacement = Self::new(&loaded, cwd.clone(), config.clone(), config.watch, now);
        replacement.reload_roots = reload_roots;
        *self = replacement;
        Ok(ReplacedReviewInput { input, loaded, cwd })
    }

    pub fn editor_base(&self) -> &Path {
        match &self.reload_plan {
            ReloadPlan::Vcs { repo_root, .. } => repo_root,
            _ => &self.cwd,
        }
    }

    fn validate_source_path(&self, source_path: Option<&Path>) -> Result<PathBuf, String> {
        if self.reload_roots.is_empty() {
            return Err(
                "session reload requires the initial ramo session to be rooted in a repository"
                    .into(),
            );
        }
        let candidate = resolve_maybe_real_path(
            &source_path.map_or_else(|| self.cwd.clone(), |path| resolve_from(&self.cwd, path)),
        );
        ensure_in_roots(&self.reload_roots, &candidate, "source path")?;
        Ok(candidate)
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

fn initial_reload_roots(initial: &LoadedReview, cwd: &Path) -> Vec<PathBuf> {
    let root = match &initial.reload_plan {
        ReloadPlan::Vcs { repo_root, .. } => Some(resolve_maybe_real_path(repo_root)),
        ReloadPlan::Files { left, right, .. } => {
            let root = crate::vcs::detect::select_vcs(cwd, None)
                .map(|detection| resolve_maybe_real_path(&detection.repo_root));
            root.filter(|root| {
                [left, right]
                    .iter()
                    .map(|path| resolve_maybe_real_path(&resolve_from(cwd, path)))
                    .all(|path| path.starts_with(root))
            })
        }
        ReloadPlan::PatchFile { path } => {
            let root = crate::vcs::detect::select_vcs(cwd, None)
                .map(|detection| resolve_maybe_real_path(&detection.repo_root));
            root.filter(|root| resolve_maybe_real_path(&resolve_from(cwd, path)).starts_with(root))
        }
        ReloadPlan::None => None,
    };
    root.into_iter().collect()
}

fn normalize_reload_input(
    mut input: ReviewInput,
    cwd: &Path,
    roots: &[PathBuf],
) -> Result<ReviewInput, String> {
    if input.options().agent_context.as_deref() == Some(Path::new("-")) {
        return Err("session reload does not support `--agent-context -`".into());
    }
    let agent_context = input.options().agent_context.as_ref().map(|path| {
        let path = resolve_maybe_real_path(&resolve_from(cwd, path));
        ensure_in_roots(roots, &path, "agent context path")?;
        Ok::<_, String>(path)
    });
    if let Some(agent_context) = agent_context {
        set_agent_context(&mut input, Some(agent_context?));
    }

    match &mut input {
        ReviewInput::FilePair { left, right, .. } => {
            *left = validate_reload_file(roots, cwd, left, "left file")?;
            *right = validate_reload_file(roots, cwd, right, "right file")?;
        }
        ReviewInput::Patch {
            source: PatchSource::File(path),
            ..
        } => {
            *path = validate_reload_file(roots, cwd, path, "patch file")?;
        }
        ReviewInput::Patch {
            source: PatchSource::Stdin,
            ..
        }
        | ReviewInput::Pager { .. } => {
            return Err("session reload does not support stdin-backed patch or pager input".into());
        }
        ReviewInput::VcsDiff { .. } | ReviewInput::Show { .. } | ReviewInput::StashShow { .. } => {}
    }
    Ok(input)
}

fn set_agent_context(input: &mut ReviewInput, path: Option<PathBuf>) {
    match input {
        ReviewInput::VcsDiff { options, .. }
        | ReviewInput::Show { options, .. }
        | ReviewInput::StashShow { options, .. }
        | ReviewInput::FilePair { options, .. }
        | ReviewInput::Patch { options, .. }
        | ReviewInput::Pager { options } => options.agent_context = path,
    }
}

fn validate_reload_file(
    roots: &[PathBuf],
    cwd: &Path,
    path: &Path,
    description: &str,
) -> Result<PathBuf, String> {
    let candidate = resolve_maybe_real_path(&resolve_from(cwd, path));
    ensure_in_roots(roots, &candidate, description)?;
    Ok(candidate)
}

fn ensure_in_roots(roots: &[PathBuf], candidate: &Path, description: &str) -> Result<(), String> {
    if roots.iter().any(|root| candidate.starts_with(root)) {
        Ok(())
    } else {
        Err(format!(
            "session reload refused {description} outside the initial ramo root: {}",
            candidate.display()
        ))
    }
}

fn resolve_from(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn resolve_maybe_real_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    if let Ok(canonical) = std::fs::canonicalize(&absolute) {
        return canonical;
    }

    let mut current = absolute.as_path();
    let mut missing = Vec::new();
    loop {
        if let Ok(canonical) = std::fs::canonicalize(current) {
            let joined = missing
                .iter()
                .rev()
                .fold(canonical, |path, part| path.join(part));
            return lexical_normalize(&joined);
        }
        let Some(name) = current.file_name() else {
            return lexical_normalize(&absolute);
        };
        missing.push(name.to_os_string());
        let Some(parent) = current.parent() else {
            return lexical_normalize(&absolute);
        };
        current = parent;
    }
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
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
