# Performance evidence

The native harness runs with `cargo bench --bench parity`. It uses `std::time::Instant` and `std::hint::black_box`, adds no benchmark framework, and reports descriptive measurements rather than enforcing a machine-dependent timing threshold.

Local release-mode sample from 2026-07-21:

```text
ramo parity stress benchmark (descriptive; no timing threshold)
parse_large_patch_50000_changed_lines: iterations=5 total_ms=21.968 mean_ms=4.394
parse_2000_files: iterations=5 total_ms=6.587 mean_ms=1.317
parse_20000_non_ascii_changed_lines: iterations=5 total_ms=8.635 mean_ms=1.727
navigate_resize_100_files: iterations=3 total_ms=2693.422 mean_ms=897.807
manual_watch_reload_50_generations: iterations=2 total_ms=2.167 mean_ms=1.083
```

The navigation scenario performs 120 alternating 159/160/220-column viewport cycles per iteration across 100 files, including page navigation and file jumps. The watch scenario writes and reloads 50 distinct direct-file generations through `WatchRuntime` per iteration.

Memory shape is checked deterministically rather than sampled from a platform allocator:

- file/theme highlight buckets and visited lines both have explicit LRUs;
- repeated controller resizes and file replacements preserve stable geometry shapes;
- repeated context reads retain one result per source and invalidation empties the cache;
- repeated watch generations return one replacement snapshot without retaining prior results.
