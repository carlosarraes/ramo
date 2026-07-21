use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ramo::config::ResolvedConfig;
use ramo::core::input::{CommonOptions, ReviewInput};
use ramo::diff::parser::parse_unified_diff;
use ramo::input::{LoadContext, ReviewLoader};
use ramo::review::{ReviewAction, ReviewController, ReviewOptions, ScrollUnit, Viewport};
use ramo::vcs::SystemCommandRunner;
use ramo::watch::{WatchIntervals, WatchRuntime, WatchUpdate};

struct BenchDir(PathBuf);

impl BenchDir {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after Unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("ramo-parity-bench-{}-{unique}", std::process::id()));
        std::fs::create_dir(&path).expect("create temporary benchmark directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for BenchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn patch(files: usize, changed_pairs: usize, non_ascii: bool) -> String {
    let mut patch = String::with_capacity(files.saturating_mul(changed_pairs).saturating_mul(64));
    for file in 0..files {
        patch.push_str(&format!(
            "diff --git a/src/file_{file}.rs b/src/file_{file}.rs\n--- a/src/file_{file}.rs\n+++ b/src/file_{file}.rs\n@@ -1,{changed_pairs} +1,{changed_pairs} @@\n"
        ));
        for line in 0..changed_pairs {
            if non_ascii {
                patch.push_str(&format!("-let antigo_{line} = \"界 café 🦀\";\n"));
                patch.push_str(&format!("+let novo_{line} = \"差分 ação 🚀\";\n"));
            } else {
                patch.push_str(&format!("-let old_{line} = {line};\n"));
                patch.push_str(&format!("+let new_{line} = {line};\n"));
            }
        }
    }
    patch
}

fn measure<T>(name: &str, iterations: usize, mut operation: impl FnMut() -> T) {
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(operation());
    }
    let elapsed = start.elapsed();
    println!(
        "{name}: iterations={iterations} total_ms={:.3} mean_ms={:.3}",
        elapsed.as_secs_f64() * 1_000.0,
        elapsed.as_secs_f64() * 1_000.0 / iterations as f64
    );
}

fn navigation_resize(files: Vec<ramo::diff::model::DiffFile>) {
    let mut controller = ReviewController::new(files, ReviewOptions::default());
    for cycle in 0..120 {
        let viewport = Viewport {
            width: [159, 160, 220][cycle % 3],
            height: [24, 40][cycle % 2],
        };
        controller.apply(
            ReviewAction::Scroll {
                delta: 1,
                unit: ScrollUnit::Page,
            },
            viewport,
        );
        if cycle % 11 == 0 {
            controller.apply(ReviewAction::MoveFile(1), viewport);
        }
        black_box(controller.snapshot(viewport));
    }
}

fn watch_reload() {
    let temp = BenchDir::new();
    let left = temp.path().join("left.rs");
    let right = temp.path().join("right.rs");
    std::fs::write(&left, "fn value() -> usize { 0 }\n").unwrap();
    std::fs::write(&right, "fn value() -> usize { 1 }\n").unwrap();
    let config = ResolvedConfig::default();
    let runner = SystemCommandRunner;
    let input = ReviewInput::FilePair {
        left,
        right: right.clone(),
        display_path: None,
        options: CommonOptions::default(),
    };
    let context = LoadContext {
        cwd: temp.path(),
        config: &config,
        runner: &runner,
    };
    let loaded = ReviewLoader
        .load_with_context(&input, &mut std::io::empty(), &context)
        .unwrap();
    let start = Instant::now();
    let mut runtime = WatchRuntime::with_intervals(
        &loaded,
        temp.path().to_path_buf(),
        config,
        false,
        start,
        WatchIntervals {
            quiet: Duration::ZERO,
            maximum: Duration::ZERO,
            safety: Duration::from_secs(60),
        },
    );
    for generation in 2..=51 {
        std::fs::write(&right, format!("fn value() -> usize {{ {generation} }}\n")).unwrap();
        let now = start + Duration::from_millis(generation as u64);
        runtime.manual_reload(now);
        assert!(matches!(runtime.poll(now), WatchUpdate::Replaced { .. }));
    }
}

fn main() {
    let large = patch(1, 25_000, false);
    let many = patch(2_000, 1, false);
    let unicode = patch(1, 10_000, true);
    let navigation = parse_unified_diff(&patch(100, 10, true));

    println!("ramo parity stress benchmark (descriptive; no timing threshold)");
    measure("parse_large_patch_50000_changed_lines", 5, || {
        parse_unified_diff(black_box(&large))
    });
    measure("parse_2000_files", 5, || {
        parse_unified_diff(black_box(&many))
    });
    measure("parse_20000_non_ascii_changed_lines", 5, || {
        parse_unified_diff(black_box(&unicode))
    });
    measure("navigate_resize_100_files", 3, || {
        navigation_resize(navigation.clone())
    });
    measure("manual_watch_reload_50_generations", 2, watch_reload);
}
