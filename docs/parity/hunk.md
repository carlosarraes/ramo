# Hunk parity ledger

Reference: `/home/carraes/github/hunk` at commit `53fcb2c`.

This ledger is the completion authority for the Rust port. Status meanings:

- `missing`: no aligned Rust implementation exists.
- `implemented`: a typed seam or partial implementation exists, but end-user behavior is not fully verified.
- `verified`: automated evidence covers the behavior at the appropriate boundary.

Only `verified` entries count toward final parity. The intentional exclusions are Hunk's top menu/dropdown UI and its JavaScript-specific OpenTUI component API; the latter is replaced by a reusable Rust library surface.

## Executable and command surface

| Capability | Status | Rust evidence | Verification evidence |
|---|---|---|---|
| One Rust `pdiff` executable; no JS runtime | verified | `Cargo.toml`, `src/main.rs` | slice-2 `cargo build --release`, `file`, and `ldd` gate; `tests/library_surface.rs::parser_is_available_from_the_library_crate` |
| Reusable Rust library surface | verified | `src/lib.rs` | `tests/library_surface.rs::parser_is_available_from_the_library_crate` |
| Bare terminal invocation prints help | verified | `src/cli/normalize.rs::normalize` | `tests/cli_parse.rs::help_and_version_are_successful_print_actions` |
| Bare piped invocation means patch stdin | verified | `src/cli/normalize.rs::normalize` | `tests/cli_parse.rs::bare_pipe_is_patch_stdin` |
| `pdiff diff [target] [-- pathspecs]` | verified | `src/cli/normalize.rs::normalize_diff`, `src/vcs/git.rs` | `tests/cli_parse.rs::diff_supports_range_flags_and_pathspecs`, `tests/git_loading.rs::range_and_pathspec_review_only_the_requested_history` |
| `pdiff diff --staged` | verified | `src/vcs/git.rs::GitAdapter` | `tests/git_loading.rs::staged_diff_excludes_untracked_and_unstaged_changes` |
| `pdiff diff --cached` | verified | normalized to staged Git operation | `tests/cli_parse.rs::cached_alias_and_boolean_overrides_are_normalized`, `tests/git_loading.rs::staged_diff_excludes_untracked_and_unstaged_changes` |
| `pdiff diff <left-file> <right-file>` | verified | `src/input/file_pair.rs::load` | `tests/cli_parse.rs::existing_two_file_operands_become_a_file_pair`, `tests/input_loading.rs::direct_files_are_diffed_without_an_external_program` |
| `pdiff show [target] [-- pathspecs]` | verified | `src/cli/normalize.rs::normalize_show`, native VCS adapters | `tests/git_loading.rs::show_defaults_to_head_and_accepts_an_explicit_ref`, `tests/jj_loading.rs::jj_diff_and_show_load_git_patches_from_the_native_command_contract`, `tests/sl_loading.rs::sl_show_and_pathspecs_are_literal_argv` |
| `pdiff stash show [ref]` | verified | `src/vcs/git.rs::GitAdapter` | `tests/git_loading.rs::stash_show_defaults_to_latest_stash_and_accepts_a_ref` |
| `pdiff patch [file|-]` | verified | `src/input/patch.rs::load` | `tests/input_loading.rs::patch_stdin_loads_a_changeset`, `patch_file_uses_its_path_as_source_context` |
| `pdiff pager` diff detection and text fallback | verified | `src/input/pager.rs`, `src/pager.rs` | `tests/pager.rs`, `tests/pty_pager.rs::patch_pager_enters_review_ui_and_quits_cleanly`, `plain_text_pager_does_not_enter_alternate_screen` |
| `pdiff difftool <left> <right> [path]` | verified | `src/input/file_pair.rs::load` | `tests/cli_parse.rs::difftool_preserves_display_path_and_watch`, `tests/input_loading.rs::binary_pairs_use_a_placeholder_without_decoding_contents` |
| `pdiff session list` | missing | — | — |
| `pdiff session get` | missing | — | — |
| `pdiff session context` | missing | — | — |
| `pdiff session review` | missing | — | — |
| `pdiff session navigate` | missing | — | — |
| `pdiff session reload` | missing | — | — |
| `pdiff session comment add` | missing | — | — |
| `pdiff session comment apply` | missing | — | — |
| `pdiff session comment list` | missing | — | — |
| `pdiff session comment rm` | missing | — | — |
| `pdiff session comment clear` | missing | — | — |
| `pdiff daemon serve` | missing | — | — |
| `pdiff mcp serve` alias | missing | — | — |
| `pdiff markup render` | missing | — | — |
| `pdiff markup guide` | missing | — | — |
| `pdiff skill path` | missing | — | — |
| `pdiff install pi` / `uninstall pi` | implemented | `src/pi_extension.rs`, `src/runtime.rs::run` | dispatch: `tests/runtime_resolution.rs::integrations_do_not_initialize_the_review_ui`; filesystem integration test missing |
| `-v` / `--version` | verified | `src/cli/mod.rs::parse_from` | `tests/cli_contract.rs::version_is_plain_and_successful` |
| `--input`, `--output`, `--stdout` compatibility | implemented | `src/cli/normalize.rs`, `src/runtime.rs::finish_annotations` | normalization: `tests/cli_parse.rs::legacy_input_and_output_flags_remain_accepted`; PTY output test missing |

## Review options and configuration

| Capability | Status | Rust evidence | Verification evidence |
|---|---|---|---|
| `--mode auto|split|stack` | verified | `src/cli/args.rs::LayoutArg`, `ReviewController` | config precedence, `tests/review_state.rs`, `tests/pty_ui.rs` |
| `--watch` | implemented | normalized `CommonOptions::watch` | parsing: `tests/cli_parse.rs::difftool_preserves_display_path_and_watch`; watcher missing |
| `--theme` | verified | normalized config plus `src/ui/themes.rs` | `tests/themes.rs`, `tests/ui_dialogs.rs`, `tests/pty_ui.rs` |
| `--agent-context` | implemented | normalized path | CLI parser coverage; loader and notes missing |
| `--pager` | verified | normalized pager flag and pager config layer | config resolution and `tests/pty_pager.rs` |
| `--line-numbers` / `--no-line-numbers` | verified | normalized config plus shared row geometry | CLI/config, state, and render tests |
| `--wrap` / `--no-wrap` | verified | normalized config plus shared row geometry | `tests/review_state.rs`, `review::geometry` tests |
| `--hunk-headers` / `--no-hunk-headers` | verified | normalized config and zero-row header planning | `tests/ui_render.rs`, `tests/ui_input.rs` |
| `--agent-notes` / `--no-agent-notes` | implemented | normalized/configured boolean | CLI/config tests; note UI missing |
| `--transparent-bg` / `--no-transparent-bg` | verified | normalized config and semantic theme resolution | `tests/themes.rs::fallback_auto_and_transparent_surfaces_are_predictable` |
| `--exclude-untracked` / inverse | verified | normalized/configured boolean, native Git/Sapling loaders | `tests/git_loading.rs::exclude_untracked_removes_only_synthetic_files`, `tests/sl_loading.rs::sl_exclude_untracked_skips_the_status_command` |
| Built-in/user/repo/command/pager/CLI precedence | verified | `src/config/load.rs::ConfigResolver` | seven cases in `tests/config_resolution.rs` |
| Platform user config path and nearest `.pdiff/config.toml` | verified | `src/config/load.rs::ConfigPaths::discover` | `tests/config_resolution.rs::discovery_chooses_the_nearest_repository_config` |
| Unknown/malformed config diagnostics | verified | `src/config/load.rs::validate_keys` | `tests/config_resolution.rs::malformed_and_unknown_config_errors_name_the_file_and_key` |
| Save changed view preferences on quit | verified | `src/config/save.rs`, `App::request_quit` | `tests/config_persistence.rs`, `tests/pty_ui.rs` |
| Built-in and custom theme definitions | verified | `src/ui/themes.rs` | `tests/themes.rs`, `tests/ui_dialogs.rs` |
| Terminal background auto-detection | missing | — | — |
| Legacy Hunk theme aliases/syntax translation | verified | `ThemeRegistry` alias normalization and scope mapping | `tests/themes.rs` |

## Input and normalized model

| Capability | Status | Rust evidence | Verification evidence |
|---|---|---|---|
| One normalized `Changeset` model | verified | `src/core/changeset.rs` | `src/core/changeset.rs::tests` |
| Stable file identity across reloads/renames | verified | `stable_file_id`, `DiffFile::id` | `file_ids_are_stable_across_reloads`, `test_renamed_file_has_stable_previous_path` |
| Added/deleted/renamed/copied/modified metadata | verified | `FileChangeKind`, `parse_file` | parser tests for all five forms |
| Patch chunk retained per file | implemented | `DiffFile::patch`, `format_patch` | indirect parser coverage; explicit chunk-boundary test missing |
| ANSI CSI, OSC, and CRLF normalization | verified | `src/input/patch.rs::normalize_patch_text` | `tests/input_loading.rs::terminal_controls_and_crlf_are_removed_before_parsing` |
| Empty versus malformed patch diagnostics | verified | `LoadError` | `tests/input_loading.rs::empty_and_malformed_patch_inputs_have_distinct_errors` |
| Direct file comparison without external diff | verified | `similar::TextDiff` in `src/input/file_pair.rs` | `direct_files_are_diffed_without_an_external_program` |
| Identical file pair | verified | empty `Changeset` path | `identical_files_produce_an_empty_changeset_with_a_reload_plan` |
| Binary file placeholder | verified | `file_pair::binary_file` | `binary_pairs_use_a_placeholder_without_decoding_contents` |
| Missing/non-UTF-8 file errors | verified | `LoadError::Io`, `LoadError::NonUtf8` | `missing_and_non_utf8_direct_files_name_the_failed_path` |
| Git working tree/range/pathspec loader | verified | `src/vcs/git.rs` | `tests/git_loading.rs::working_tree_includes_tracked_and_untracked_files`, `range_and_pathspec_review_only_the_requested_history` |
| Git staged loader | verified | `src/vcs/git.rs::GitAdapter` | `tests/git_loading.rs::staged_diff_excludes_untracked_and_unstaged_changes` |
| Git untracked-file inclusion | verified | `src/vcs/untracked.rs` | `tests/git_loading.rs::working_tree_includes_tracked_and_untracked_files`, `exclude_untracked_removes_only_synthetic_files` |
| Git show and stash loader | verified | `src/vcs/git.rs` | `tests/git_loading.rs::show_defaults_to_head_and_accepts_an_explicit_ref`, `stash_show_defaults_to_latest_stash_and_accepts_a_ref` |
| Git moved-line classification | verified | `src/vcs/git.rs::parse_git_patch` | `tests/input_loading.rs::deterministic_git_ansi_colors_become_moved_line_classes` |
| Jujutsu detection and loader | verified | `src/vcs/detect.rs`, `src/vcs/jj.rs` | `tests/vcs_contract.rs::nearest_checkout_wins_and_same_root_prefers_jj_then_sl_then_git`, `tests/jj_loading.rs` |
| Sapling detection and loader | verified | `src/vcs/detect.rs`, `src/vcs/sl.rs` | `tests/vcs_contract.rs::nearest_checkout_wins_and_same_root_prefers_jj_then_sl_then_git`, `upstream_mercurial_marker_is_not_misdetected_as_sapling`, `tests/sl_loading.rs` |
| On-demand old/new source fetchers | verified | `src/vcs/source.rs::SourceReader`, `DiffFile::{old_source,new_source}` | `tests/git_loading.rs::source_specs_match_worktree_staged_show_and_rename_endpoints`, `source_reader_bounds_text_and_returns_none_for_absent_sides`, `source_reader_reads_and_caches_the_git_index_side` |
| Large-file placeholders and truncated stats | verified | bounded Git/Sapling loaders and normalized model fields | `tests/git_loading.rs::large_tracked_and_untracked_files_are_bounded_placeholders_with_stats`, `tests/sl_loading.rs::sl_unknown_files_reuse_binary_and_large_file_policy` |
| Diff-aware pager and safe plain-text pager | verified | `src/input/pager.rs`, `src/pager.rs` | `tests/pager.rs`, `tests/pty_pager.rs` |

## Review UI and controls

| Capability | Status | Rust evidence | Verification evidence |
|---|---|---|---|
| Continuous multi-file review stream | verified | `ReviewController`, `ReviewWidget` | `tests/review_state.rs`, `tests/ui_render.rs`, `tests/pty_ui.rs::resize_thresholds_keep_the_selected_file_anchor` |
| Sidebar navigates rather than filters stream | verified | controller sidebar entries and shared hit regions | state and mouse tests |
| File filter and focus handling | verified | typed filter input and derived visible files | `tests/review_state.rs`, `tests/ui_input.rs`, `tests/pty_ui.rs::filter_owns_literal_keys_and_tab_returns_to_help_and_review` |
| Responsive `auto` split/stack | verified | `resolve_responsive_layout` | geometry tests and 220→160→159 PTY resize test |
| Explicit split layout | verified | shared split row plan/geometry | row, state, render, and PTY tests |
| Explicit stack layout | verified | shared stack row plan/geometry | row, state, render, and PTY tests |
| Resize anchor preservation | verified | `review::anchor` transactions | anchor tests and resize PTY test |
| Row/file windowing and adaptive overscan | verified | `ReviewGeometry::visible_window` | geometry 100,000-row test and bounded render highlight test |
| Syntax highlighting | verified | bounded `HighlightCache` | `tests/highlighting.rs`, `tests/ui_render.rs::renderer_highlights_only_the_bounded_visible_window` |
| Character-level changed-content emphasis | verified | `review::emphasis` | emphasis unit tests and semantic render test |
| Line numbers and change markers | verified | semantic review cells and shared columns | row/render/state tests |
| Moved-line colors | verified | moved classes and semantic palettes | input, theme, and render tests |
| Optional hunk headers | verified | zero-row header plan | `tests/ui_render.rs::hunk_headers_can_occupy_zero_rows_and_file_states_render` |
| Wrapping and horizontal scroll | verified | shared cell-width geometry and typed actions | geometry, state, input, and mouse tests |
| Collapsed context and per-hunk expansion | verified | `src/review/context.rs` and owned native loader | `tests/context_expansion.rs`, `tests/pty_ui.rs::direct_controls_and_context_expansion_remain_native_across_layout_changes` |
| Binary/large/untracked/rename file UI | verified | typed placeholders and sidebar statuses | state and render tests |
| Inline AI/agent/user note cards | missing | legacy human comments are not parity note cards | — |
| Wide-character selection/copy correctness | verified | `review::selection` terminal-cell projection | `tests/review_selection.rs`, `tests/ui_render.rs::stable_selection_projection_is_painted_on_the_selected_terminal_cells` |
| Restrained contextual bottom status | implemented | status renders only for filter/toast feedback | save-error app test; dedicated PTY styling assertion remains staged |
| No top menu bar/dropdowns | verified | intentional product exclusion; no menu components | source inspection plus pager/UI PTY absence assertions |
| Help dialog | verified | `DialogOverlay::Help` | dialog/input tests and filter/help PTY test |
| Theme selector dialog | verified | `DialogOverlay::Theme` | theme/dialog tests and save PTY test |
| Save-preferences confirmation | verified | `DialogOverlay::Save`, targeted config writer | config persistence and PTY tests |
| Agent-skill dialog | missing | — | — |

### Keyboard actions

| Action | Status | Evidence |
|---|---|---|
| Arrow step scrolling | verified | typed key map plus controller clamp tests |
| Space/`f` page down, `b` page up, Shift-Space | verified | `tests/ui_input.rs`, `tests/review_state.rs` |
| `d`/`u` half-page scrolling | verified | `tests/ui_input.rs`, `tests/review_state.rs` |
| `g`/`G`, Home/End bounds | verified | input and controller navigation tests |
| `[`/`]` hunk navigation | verified | input and wrapping navigation tests |
| `,`/`.` file navigation | verified | input, state, and multi-file PTY tests |
| `{`/`}` annotated-hunk navigation | verified | `tests/review_state.rs::annotated_navigation_respects_filter_and_empty_filters_are_safe` |
| `1`/`2`/`0` layout selection | verified | input/state tests and PTY controls test |
| `s` sidebar | verified | input/state/mouse tests |
| `t` theme selector | verified | input/dialog and PTY tests |
| `a` agent notes | implemented | view preference toggles; normalized note cards remain slice 5 |
| `z` unchanged context | verified | context controller/app and PTY tests |
| `l` line numbers | verified | input/state/render tests |
| `w` wrapping | verified | input/state/geometry tests |
| `m` hunk metadata | verified | input/state/render tests |
| `e` editor | implemented | typed `EditFile` effect; process/job-control execution remains staged |
| `r` reload | implemented | typed reload effect; watcher/reload execution remains staged |
| `/` file filter | verified | state/input and literal-key PTY tests |
| `c` create review note | implemented | legacy human comment; inline parity behavior missing |
| Tab focus toggle | verified | filter-aware typed input and PTY test |
| `?` help | verified | input/dialog and PTY tests |
| `q` quit | verified | typed quit/save reduction | pager/UI PTY restoration tests |
| Existing Vim selection/yank/tmux actions on new bindings | implemented | stable review selection projection plus OSC 52/tmux routing | selection/input/render tests; live tmux integration remains staged |

### Mouse actions

| Action | Status | Evidence |
|---|---|---|
| Wheel vertical scrolling | verified | `map_mouse_event` and controller scrolling | `tests/ui_mouse.rs` |
| Shift-wheel horizontal scrolling | verified | typed horizontal mouse action | `tests/ui_mouse.rs` |
| Sidebar selection | verified | shared sidebar hit region | `tests/ui_mouse.rs` |
| Scrollbar interaction | verified | total-geometry scrollbar mapping | `tests/ui_mouse.rs` |
| Context expansion | verified | collapsed-row gap hit | `tests/ui_mouse.rs` |
| Text selection/copy | verified | shared cell projection and OSC 52 path | selection, mouse, and render tests |

## Watch, process, and integrations

| Capability | Status | Rust evidence | Verification evidence |
|---|---|---|---|
| File reload plan seam | verified | `ReloadPlan::Files` | `tests/input_loading.rs::identical_files_produce_an_empty_changeset_with_a_reload_plan` |
| VCS reload plan seam | implemented | `ReloadPlan::Vcs` from native adapters | plan construction covered by Git/JJ/SL loader tests; observation and reload execution remain slice 4 |
| Filesystem observation with debounce | missing | — | — |
| Polling fallback | missing | — | — |
| Serialized reload and stale-result protection | missing | — | — |
| Selection/viewport preservation on reload | missing | — | — |
| Manual `r` reload | missing | — | — |
| Error display retains last valid review | missing | — | — |
| TTY replacement after piped input | verified | `src/runtime.rs::replace_stdin_with_tty` | `tests/runtime_resolution.rs::only_piped_stdin_needs_a_tty_replacement`, `tests/pty_pager.rs::patch_pager_enters_review_ui_and_quits_cleanly` |
| Terminal restoration on normal app error | implemented | `src/runtime.rs::run_review` restores before propagating | normal-return restoration is PTY-verified; injected app-error coverage remains staged |
| Panic restoration, suspend/resume, editor job control | missing | — | — |
| `$EDITOR` file/line launch | missing | — | — |
| Pi integration | implemented | `src/pi_extension.rs` | filesystem integration test missing |
| tmux pane discovery/send | implemented | `src/tmux.rs`, stable selection routing | new key mapping is tested; live tmux integration remains staged |
| OSC 52 clipboard | verified | `src/clipboard.rs`, shared selection projection | CJK mouse-selection PTY test asserts exact OSC 52 payload |
| Linux support | implemented | native build/test | cross-platform CI evidence incomplete |
| macOS support | implemented | Unix paths present | CI evidence incomplete |
| Windows support | missing | piped interactive review explicitly returns unsupported | — |

## Notes, STML, and live sessions

| Capability | Status | Evidence |
|---|---|---|
| Agent-context JSON and narrative file order | missing | — |
| Hunk/line-targeted AI and agent notes | missing | — |
| Human inline draft/edit/save/cancel | implemented | legacy popup/comment model differs from parity; interaction tests missing |
| Markdown review export | verified | `src/annotations/output.rs` | `annotations::output::tests` |
| Deterministic STML parse/layout/render | missing | — |
| STML authoring guide | missing | — |
| Loopback same-binary daemon | missing | — |
| API/daemon capability versions | missing | — |
| Session registration/reconnection | missing | — |
| Multi-session selectors and routing | missing | — |
| Session review/context projections | missing | — |
| Session navigation/reload | missing | — |
| Session comment add/apply/list/rm/clear | missing | — |
| Loopback enforcement, payload bounds, timeouts | missing | — |
| Stale-daemon replacement | missing | — |

## Performance and release closure

| Capability | Status | Evidence |
|---|---|---|
| Lazy syntax/full-source loading | verified | visible-window `HighlightCache`, cached `NativeContextSourceLoader` | highlighting, context, and render tests |
| Bounded highlight/geometry caches | verified | bounded highlight LRU and active shared geometry | highlighting and 100,000-row geometry tests |
| Shared geometry for rendering/interaction | verified | `ReviewGeometry`, shared row columns and hit tests | geometry, selection, mouse, and render tests |
| Large patch/many-file/non-ASCII benchmarks | missing | — |
| Navigation/resize/watch memory checks | missing | — |
| Cross-platform CLI/unit CI | missing | — |
| Unix PTY integration suite | verified | bounded portable-PTY harnesses | `tests/pty_pager.rs` and `tests/pty_ui.rs` |
| Single-binary install/release documentation | implemented | `README.md`, `install.sh` | release artifact verified locally in slice 2; distributable install smoke remains staged |
| Every in-scope row verified | missing | this ledger intentionally records remaining work | final audit pending |
