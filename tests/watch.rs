use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use pdiff::core::input::VcsId;
use pdiff::core::input::{CommonOptions, ReviewInput};
use pdiff::input::{LoadContext, ReloadPlan, ReviewLoader};
use pdiff::vcs::SystemCommandRunner;
use pdiff::watch::{
    Coverage, NativeObserver, WatchCoordinator, WatchIntervals, WatchPlan, WatchRuntime,
    WatchTarget, WatchUpdate,
};

#[test]
fn direct_files_watch_parent_entries_so_atomic_replacements_are_seen() {
    let temp = tempfile::tempdir().unwrap();
    let before = temp.path().join("before.rs");
    let after = temp.path().join("after.rs");
    let plan = WatchPlan::from_reload_plan(
        &ReloadPlan::Files {
            left: before.clone(),
            right: after.clone(),
            display_path: None,
        },
        temp.path(),
    )
    .unwrap();

    assert_eq!(plan.coverage, Coverage::Hybrid);
    assert_eq!(
        plan.targets,
        vec![WatchTarget::Entries {
            directory: temp.path().to_path_buf(),
            entries: vec![after, before],
        }]
    );
}

#[test]
fn patch_paths_are_resolved_from_the_stable_launch_directory() {
    let temp = tempfile::tempdir().unwrap();
    let plan = WatchPlan::from_reload_plan(
        &ReloadPlan::PatchFile {
            path: PathBuf::from("review.patch"),
        },
        temp.path(),
    )
    .unwrap();

    assert_eq!(
        plan.targets,
        vec![WatchTarget::Entries {
            directory: temp.path().to_path_buf(),
            entries: vec![temp.path().join("review.patch")],
        }]
    );
}

#[test]
fn git_is_hybrid_while_jj_and_sapling_are_poll_only() {
    let root = PathBuf::from("repo-root");
    assert_eq!(
        WatchPlan::for_vcs(root.clone(), VcsId::Git).coverage,
        Coverage::Hybrid
    );
    for vcs in [VcsId::Jj, VcsId::Sl] {
        let plan = WatchPlan::for_vcs(root.clone(), vcs);
        assert_eq!(plan.coverage, Coverage::PollOnly);
        assert!(plan.targets.is_empty());
    }
}

#[test]
fn bursts_coalesce_and_inflight_hints_get_one_trailing_generation() {
    let start = Instant::now();
    let mut coordinator =
        WatchCoordinator::new(start, Duration::from_millis(200), Duration::from_secs(1));
    coordinator.event_hint(start);
    coordinator.event_hint(start + Duration::from_millis(150));
    assert_eq!(coordinator.tick(start + Duration::from_millis(349)), None);
    let first = coordinator
        .tick(start + Duration::from_millis(350))
        .unwrap();
    assert!(coordinator.accept_result(first));

    coordinator.event_hint(start + Duration::from_millis(360));
    assert_eq!(coordinator.tick(start + Duration::from_secs(2)), None);
    coordinator.finish(first, start + Duration::from_secs(2));
    let second = coordinator.tick(start + Duration::from_secs(2)).unwrap();
    assert!(second > first);
    assert!(!coordinator.accept_result(first));
    assert!(coordinator.accept_result(second));
}

#[test]
fn manual_hints_are_immediate_and_safety_polls_are_bounded() {
    let start = Instant::now();
    let mut coordinator = WatchCoordinator::with_safety_interval(
        start,
        Duration::from_millis(200),
        Duration::from_secs(1),
        Duration::from_secs(10),
    );
    assert_eq!(coordinator.tick(start + Duration::from_secs(9)), None);
    let safety = coordinator.tick(start + Duration::from_secs(10)).unwrap();
    coordinator.finish(safety, start + Duration::from_secs(10));
    coordinator.manual_hint(start + Duration::from_secs(11));
    assert!(coordinator.tick(start + Duration::from_secs(11)).is_some());
}

#[test]
fn native_observer_sees_an_atomic_replacement_of_an_exact_file() {
    let temp = tempfile::tempdir().unwrap();
    let watched = temp.path().join("after.rs");
    let replacement = temp.path().join("after-replacement.rs");
    fs::write(&watched, "initial\n").unwrap();
    let plan = WatchPlan {
        coverage: Coverage::Hybrid,
        targets: vec![WatchTarget::Entries {
            directory: temp.path().to_path_buf(),
            entries: vec![watched.clone()],
        }],
    };
    let mut observer = NativeObserver::start(&plan).unwrap();

    fs::write(&replacement, "replacement\n").unwrap();
    fs::rename(&replacement, &watched).unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let observed = observer.poll();
        assert_eq!(observed.error, None);
        if observed.changed {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native watcher did not report the atomic replacement"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn direct_file_review(
    temp: &tempfile::TempDir,
) -> (pdiff::input::LoadedReview, PathBuf, ResolvedWatchContext) {
    let left = temp.path().join("left.rs");
    let right = temp.path().join("right.rs");
    fs::write(&left, "old\n").unwrap();
    fs::write(&right, "first\n").unwrap();
    let input = ReviewInput::FilePair {
        left,
        right: right.clone(),
        display_path: None,
        options: CommonOptions::default(),
    };
    let resolved = ResolvedWatchContext::new(temp.path());
    let loaded = ReviewLoader
        .load_with_context(&input, &mut std::io::empty(), &resolved.context())
        .unwrap();
    (loaded, right, resolved)
}

struct ResolvedWatchContext {
    cwd: PathBuf,
    config: pdiff::config::ResolvedConfig,
    runner: SystemCommandRunner,
}

impl ResolvedWatchContext {
    fn new(cwd: &std::path::Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            config: pdiff::config::ResolvedConfig::default(),
            runner: SystemCommandRunner,
        }
    }

    fn context(&self) -> LoadContext<'_> {
        LoadContext {
            cwd: &self.cwd,
            config: &self.config,
            runner: &self.runner,
        }
    }
}

#[test]
fn polling_fallback_reloads_only_when_the_content_fingerprint_changes() {
    let temp = tempfile::tempdir().unwrap();
    let (loaded, right, resolved) = direct_file_review(&temp);
    let start = Instant::now();
    let mut runtime = WatchRuntime::with_intervals(
        &loaded,
        resolved.cwd.clone(),
        resolved.config.clone(),
        true,
        start,
        WatchIntervals {
            quiet: Duration::from_millis(1),
            maximum: Duration::from_millis(1),
            safety: Duration::from_secs(10),
        },
    );

    fs::write(&right, "second\n").unwrap();
    assert!(matches!(
        runtime.poll(start + Duration::from_secs(9)),
        WatchUpdate::Unchanged
    ));
    let WatchUpdate::Replaced { files, generation } = runtime.poll(start + Duration::from_secs(10))
    else {
        panic!("safety poll did not replace changed content");
    };
    assert!(generation > 0);
    assert!(files[0].patch.contains("+second"));
    assert!(matches!(
        runtime.poll(start + Duration::from_secs(20)),
        WatchUpdate::Unchanged
    ));
}

#[test]
fn failed_reload_keeps_the_applied_fingerprint_and_can_retry() {
    let temp = tempfile::tempdir().unwrap();
    let (loaded, right, resolved) = direct_file_review(&temp);
    let start = Instant::now();
    let mut runtime = WatchRuntime::with_intervals(
        &loaded,
        resolved.cwd.clone(),
        resolved.config.clone(),
        false,
        start,
        WatchIntervals::default(),
    );

    fs::remove_file(&right).unwrap();
    runtime.manual_reload(start);
    let WatchUpdate::Error { message } = runtime.poll(start) else {
        panic!("missing file did not produce a reload error");
    };
    assert!(message.contains(&right.display().to_string()));

    fs::write(&right, "recovered\n").unwrap();
    runtime.manual_reload(start + Duration::from_secs(1));
    let WatchUpdate::Replaced { files, .. } = runtime.poll(start + Duration::from_secs(1)) else {
        panic!("runtime did not retry after the source recovered");
    };
    assert!(files[0].patch.contains("+recovered"));
}

#[test]
fn disabled_watch_never_polls_but_manual_reload_still_works() {
    let temp = tempfile::tempdir().unwrap();
    let (loaded, right, resolved) = direct_file_review(&temp);
    let start = Instant::now();
    let mut runtime = WatchRuntime::with_intervals(
        &loaded,
        resolved.cwd.clone(),
        resolved.config.clone(),
        false,
        start,
        WatchIntervals {
            quiet: Duration::from_millis(1),
            maximum: Duration::from_millis(1),
            safety: Duration::from_secs(1),
        },
    );

    fs::write(&right, "manual only\n").unwrap();
    assert!(matches!(
        runtime.poll(start + Duration::from_secs(30)),
        WatchUpdate::Unchanged
    ));
    runtime.manual_reload(start + Duration::from_secs(30));
    let WatchUpdate::Replaced { files, .. } = runtime.poll(start + Duration::from_secs(30)) else {
        panic!("manual reload did not run with watch disabled");
    };
    assert!(files[0].patch.contains("+manual only"));
}

#[test]
fn repeated_manual_watch_generations_replace_one_snapshot_without_accumulating_results() {
    let temp = tempfile::tempdir().unwrap();
    let (loaded, right, resolved) = direct_file_review(&temp);
    let start = Instant::now();
    let mut runtime = WatchRuntime::with_intervals(
        &loaded,
        resolved.cwd.clone(),
        resolved.config.clone(),
        false,
        start,
        WatchIntervals::default(),
    );

    for cycle in 1..=100 {
        fs::write(&right, format!("generation {cycle}\n")).unwrap();
        let now = start + Duration::from_millis(cycle);
        runtime.manual_reload(now);
        let WatchUpdate::Replaced { files, generation } = runtime.poll(now) else {
            panic!("generation {cycle} did not replace the review");
        };
        assert_eq!(files.len(), 1);
        assert_eq!(generation, cycle);
        assert!(files[0].patch.contains(&format!("+generation {cycle}")));
    }

    assert!(matches!(
        runtime.poll(start + Duration::from_secs(1)),
        WatchUpdate::Unchanged
    ));
}

#[test]
fn repeated_reload_errors_are_suppressed_for_a_bounded_interval() {
    let temp = tempfile::tempdir().unwrap();
    let (loaded, right, resolved) = direct_file_review(&temp);
    let start = Instant::now();
    let mut runtime = WatchRuntime::with_intervals(
        &loaded,
        resolved.cwd.clone(),
        resolved.config.clone(),
        false,
        start,
        WatchIntervals::default(),
    );
    fs::remove_file(&right).unwrap();

    runtime.manual_reload(start);
    assert!(matches!(runtime.poll(start), WatchUpdate::Error { .. }));
    runtime.manual_reload(start + Duration::from_secs(1));
    assert!(matches!(
        runtime.poll(start + Duration::from_secs(1)),
        WatchUpdate::Unchanged
    ));
    runtime.manual_reload(start + Duration::from_secs(11));
    assert!(matches!(
        runtime.poll(start + Duration::from_secs(11)),
        WatchUpdate::Error { .. }
    ));
}

#[test]
fn unavailable_native_observation_degrades_to_two_second_polling() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let left = source.join("left.rs");
    let right = source.join("right.rs");
    fs::write(&left, "old\n").unwrap();
    fs::write(&right, "first\n").unwrap();
    let input = ReviewInput::FilePair {
        left: left.clone(),
        right: right.clone(),
        display_path: None,
        options: CommonOptions::default(),
    };
    let resolved = ResolvedWatchContext::new(temp.path());
    let loaded = ReviewLoader
        .load_with_context(&input, &mut std::io::empty(), &resolved.context())
        .unwrap();
    fs::remove_dir_all(&source).unwrap();

    let start = Instant::now();
    let mut runtime = WatchRuntime::new(
        &loaded,
        resolved.cwd.clone(),
        resolved.config.clone(),
        true,
        start,
    );
    let WatchUpdate::Error { message } = runtime.poll(start) else {
        panic!("watcher startup failure was not reported");
    };
    assert!(message.contains("polling instead"));

    fs::create_dir(&source).unwrap();
    fs::write(&left, "old\n").unwrap();
    fs::write(&right, "recovered\n").unwrap();
    assert!(matches!(
        runtime.poll(start + Duration::from_millis(1_999)),
        WatchUpdate::Unchanged
    ));
    let WatchUpdate::Replaced { files, .. } = runtime.poll(start + Duration::from_secs(2)) else {
        panic!("degraded polling did not recover the review");
    };
    assert!(files[0].patch.contains("+recovered"));
}
