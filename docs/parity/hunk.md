# Hunk parity ledger

Reference: `/home/carraes/github/hunk` at commit `53fcb2c`.

This ledger is the completion authority for the Rust port. Status meanings:

- `missing`: no aligned Rust implementation exists.
- `implemented`: a typed seam or partial implementation exists, but end-user behavior is not fully verified.
- `verified`: automated evidence covers the behavior at the appropriate boundary.

Only `verified` entries count toward final parity. The intentional exclusions are Hunk's top menu/dropdown UI and its JavaScript-specific OpenTUI component API; the latter is replaced by a reusable Rust library surface. Ramo also intentionally keeps pdiff-style semantic cursor navigation: `j`/`k` move between diff rows, `h`/`l` focus split sides, `n` toggles numbers, and Left/Right scroll horizontally. These bindings are product identity, not missing Hunk parity.

## Executable and command surface

| Capability | Status | Rust evidence | Verification evidence |
|---|---|---|---|
| One Rust `ramo` executable; no JS runtime | verified | `Cargo.toml`, `src/main.rs` | slice-2 `cargo build --release`, `file`, and `ldd` gate; `tests/library_surface.rs::parser_is_available_from_the_library_crate` |
| Reusable Rust library surface | verified | `src/lib.rs` | `tests/library_surface.rs::parser_is_available_from_the_library_crate` |
| Bare terminal invocation prints complete help | verified | `src/cli/normalize.rs::normalize`, top-level common-option help | `tests/cli_parse.rs::help_and_version_are_successful_print_actions`, `tests/cli_contract.rs::help_lists_every_foundation_review_command` |
| Bare piped invocation means patch stdin | verified | `src/cli/normalize.rs::normalize` | `tests/cli_parse.rs::bare_pipe_is_patch_stdin` |
| `ramo diff [target] [-- pathspecs]` | verified | `src/cli/normalize.rs::normalize_diff`, `src/vcs/git.rs` | `tests/cli_parse.rs::diff_supports_range_flags_and_pathspecs`, `tests/git_loading.rs::range_and_pathspec_review_only_the_requested_history` |
| `ramo diff --staged` | verified | `src/vcs/git.rs::GitAdapter` | `tests/git_loading.rs::staged_diff_excludes_untracked_and_unstaged_changes` |
| `ramo diff --cached` | verified | normalized to staged Git operation | `tests/cli_parse.rs::cached_alias_and_boolean_overrides_are_normalized`, `tests/git_loading.rs::staged_diff_excludes_untracked_and_unstaged_changes` |
| `ramo diff <left-file> <right-file>` | verified | `src/input/file_pair.rs::load` | `tests/cli_parse.rs::existing_two_file_operands_become_a_file_pair`, `tests/input_loading.rs::direct_files_are_diffed_without_an_external_program` |
| `ramo show [target] [-- pathspecs]` | verified | `src/cli/normalize.rs::normalize_show`, native VCS adapters | `tests/git_loading.rs::show_defaults_to_head_and_accepts_an_explicit_ref`, `tests/jj_loading.rs::jj_diff_and_show_load_git_patches_from_the_native_command_contract`, `tests/sl_loading.rs::sl_show_and_pathspecs_are_literal_argv` |
| `ramo stash show [ref]` | verified | `src/vcs/git.rs::GitAdapter` | `tests/git_loading.rs::stash_show_defaults_to_latest_stash_and_accepts_a_ref` |
| `ramo patch [file|-]` | verified | `src/input/patch.rs::load` | `tests/input_loading.rs::patch_stdin_loads_a_changeset`, `patch_file_uses_its_path_as_source_context` |
| `ramo pager` diff detection and text fallback | verified | `src/input/pager.rs`, `src/pager.rs` | `tests/pager.rs`, `tests/pty_pager.rs::patch_pager_enters_review_ui_and_quits_cleanly`, `plain_text_pager_does_not_enter_alternate_screen` |
| `ramo difftool <left> <right> [path]` | verified | `src/input/file_pair.rs::load` | `tests/cli_parse.rs::difftool_preserves_display_path_and_watch`, `tests/input_loading.rs::binary_pairs_use_a_placeholder_without_decoding_contents` |
| `ramo session list` | verified | `src/session/{client,http,daemon}.rs` | `tests/session_daemon.rs::installed_binary_serves_and_cli_commands_use_the_native_daemon_without_a_tui`, `tests/session_e2e.rs::two_live_terminals_route_isolated_commands_and_reconnect_before_idle_exit` |
| `ramo session get` | verified | native descriptor projection and selector routing | `tests/session_daemon.rs::registry_list_get_context_review_and_selector_errors_are_structured`, two-PTY ID/path selection in `tests/session_e2e.rs` |
| `ramo session context` | verified | `src/session/projection.rs::build_session_context` | isolated selected-hunk/note assertions in `tests/session_e2e.rs` |
| `ramo session review` | verified | opt-in bounded patch/note projection | `tests/session_projection.rs::projections_filter_patch_and_notes_and_clearing_requires_user_opt_in`, post-reload PTY export in `tests/pty_session.rs` |
| `ramo session navigate` | verified | UI-thread `ReviewController::navigate_session_target` bridge | `tests/session_bridge.rs::navigation_resolves_one_based_hunks_side_lines_and_annotated_directions`, two-PTY isolation in `tests/session_e2e.rs` |
| `ramo session reload` | verified | typed loader/config/watch transaction with root bounds | `tests/session_reload.rs`, public CLI-to-PTY reload in `tests/pty_session.rs::live_pty_routes_navigation_comments_failures_lists_and_clearing_on_the_ui_thread` |
| `ramo session comment add` | verified | native live-note bridge | `tests/session_bridge.rs::add_batch_markup_focus_and_failed_target_isolation_are_deterministic`, two-PTY isolation in `tests/session_e2e.rs` |
| `ramo session comment apply` | verified | bounded atomic batch bridge | `tests/session_cli.rs::comment_commands_validate_targets_batches_types_and_destructive_confirmation`, `tests/session_bridge.rs::add_batch_markup_focus_and_failed_target_isolation_are_deterministic` |
| `ramo session comment list` | verified | source-distinct snapshot projection | `tests/session_bridge.rs::list_remove_and_clear_keep_live_external_and_human_sources_distinct`, live PTY list in `tests/pty_session.rs` |
| `ramo session comment rm` | verified | stable live/human note removal | `tests/session_bridge.rs::list_remove_and_clear_keep_live_external_and_human_sources_distinct` |
| `ramo session comment clear` | verified | explicit confirmation plus human opt-in | `tests/session_cli.rs::comment_commands_validate_targets_batches_types_and_destructive_confirmation`, `tests/session_projection.rs::projections_filter_patch_and_notes_and_clearing_requires_user_opt_in`, `tests/pty_session.rs` |
| `ramo daemon serve` | verified | std-only same-binary loopback broker | `tests/session_daemon.rs::installed_binary_serves_and_cli_commands_use_the_native_daemon_without_a_tui` |
| `ramo mcp serve` alias | verified | same native daemon action; legacy HTTP MCP route is a tombstone | `tests/session_cli.rs::selector_conflicts_and_daemon_aliases_are_explicit`, `tests/session_daemon.rs::health_capabilities_and_legacy_tombstone_are_bounded_json_routes` |
| `ramo markup render` | verified | `src/markup/{layout,render,command}.rs`, direct runtime dispatch | `tests/markup_cli.rs::render_accepts_files_stdin_plain_text_and_json_without_entering_the_tui`, `render_color_modes_resolve_symbolic_named_and_hex_colors_natively` |
| `ramo markup guide` | verified | embedded `src/markup/guide.md` | `tests/markup_cli.rs::guide_is_embedded_and_all_stml_fences_layout_at_reference_width` |
| `ramo skill path` | verified | embedded, atomically materialized Rust-owned skill asset | `tests/skill_path.rs::skill_path_materializes_the_embedded_ramo_review_skill_without_a_runtime_bundle` |
| `ramo install pi` / `uninstall pi` | verified | `src/pi_extension.rs`, `src/runtime.rs::run` | `tests/runtime_resolution.rs::integrations_do_not_initialize_the_review_ui`, `tests/integrations.rs::pi_install_writes_a_markdown_prompt_and_no_typescript`, `pi_uninstall_preserves_unrelated_prompt_files` |
| `-v` / `--version` | verified | `src/cli/mod.rs::parse_from` | `tests/cli_contract.rs::version_is_plain_and_successful` |
| `--input`, `--output`, `--stdout` compatibility | verified | `src/cli/normalize.rs`, `src/runtime.rs::finish_annotations` | `tests/cli_parse.rs::legacy_input_and_output_flags_remain_accepted`, `tests/pty_notes.rs::stdout_export_is_printed_after_the_tui_restores_the_terminal`, explicit output in `human_note_draft_owns_keys_saves_inline_and_exports_markdown` |

## Review options and configuration

| Capability | Status | Rust evidence | Verification evidence |
|---|---|---|---|
| `--mode auto|split|stack` | verified | `src/cli/args.rs::LayoutArg`, `ReviewController` | config precedence, `tests/review_state.rs`, `tests/pty_ui.rs` |
| `--watch` | verified | normalized `CommonOptions::watch`, `src/watch` | `tests/watch.rs`, `tests/pty_watch.rs::watch_mode_refreshes_after_an_atomic_save` |
| `--theme` | verified | normalized config plus `src/ui/themes.rs` | `tests/themes.rs`, `tests/ui_dialogs.rs`, `tests/pty_ui.rs` |
| `--agent-context` | verified | bounded `src/notes/context.rs`, attachment and reload plan integration | seven cases in `tests/agent_context.rs`, including ordering, rename matching, stdin conflict, sidecar-only watch reload, and limits |
| `--pager` | verified | normalized pager flag and pager config layer | config resolution and `tests/pty_pager.rs` |
| `--line-numbers` / `--no-line-numbers` | verified | normalized config plus shared row geometry | CLI/config, state, and render tests |
| `--wrap` / `--no-wrap` | verified | normalized config plus shared row geometry | `tests/review_state.rs`, `review::geometry` tests |
| `--hunk-headers` / `--no-hunk-headers` | verified | normalized config and zero-row header planning | `tests/ui_render.rs`, `tests/ui_input.rs` |
| `--agent-notes` / `--no-agent-notes` | verified | normalized/configured boolean and canonical note-row visibility | `tests/notes_state.rs::external_visibility_and_human_notes_drive_shared_geometry_and_survive_reload`, `tests/pty_notes.rs::agent_notes_toggle_in_the_live_review` |
| `--transparent-bg` / `--no-transparent-bg` | verified | normalized config and semantic theme resolution | `tests/themes.rs::fallback_auto_and_transparent_surfaces_are_predictable` |
| `--exclude-untracked` / inverse | verified | normalized/configured boolean, native Git/Sapling loaders | `tests/git_loading.rs::exclude_untracked_removes_only_synthetic_files`, `tests/sl_loading.rs::sl_exclude_untracked_skips_the_status_command` |
| Built-in/user/repo/command/pager/CLI precedence | verified | `src/config/load.rs::ConfigResolver` | seven cases in `tests/config_resolution.rs` |
| Platform user config path and nearest `.ramo/config.toml` | verified | `src/config/load.rs::ConfigPaths::discover` | `tests/config_resolution.rs::discovery_chooses_the_nearest_repository_config` |
| Unknown/malformed config diagnostics | verified | `src/config/load.rs::validate_keys` | `tests/config_resolution.rs::malformed_and_unknown_config_errors_name_the_file_and_key` |
| Save changed view preferences on quit | verified | `src/config/save.rs`, `App::request_quit` | `tests/config_persistence.rs`, `tests/pty_ui.rs` |
| Copied-decoration preference | verified | `ReviewOptions::copy_decorations` projects the rendered line-number/change-marker gutter | `tests/ui_render.rs::copied_decorations_config_includes_the_rendered_gutter_for_line_selection` |
| `transparentBackground` config compatibility alias | verified | typed camel-case compatibility field resolves ahead of the snake-case key | `tests/config_resolution.rs::transparent_background_accepts_hunks_camel_case_compatibility_key` |
| Built-in and custom theme definitions | verified | `src/ui/themes.rs` | `tests/themes.rs`, `tests/ui_dialogs.rs` |
| Terminal background auto-detection | verified | environment-only `COLORFGBG` hint with deterministic dark fallback; no active terminal query | `tests/terminal_appearance.rs::osc11_parsing_classification_and_environment_fallback_match_hunk`, `auto_theme_starts_without_querying_or_waiting_for_terminal_input`, `tests/pty_ui.rs::semantic_navigation_moves_the_rendered_cursor_without_startup_input` |
| Legacy Hunk theme aliases/syntax translation | verified | alias normalization plus semantic-role-to-TextMate translation with exact-scope precedence | `tests/themes.rs::registry_preserves_hunks_reference_order_and_legacy_aliases`, `deprecated_semantic_syntax_is_translated_and_emits_one_startup_notice` |
| Compatibility startup notices | verified | resolved config notices enter a timed, deduplicated native queue; pager mode suppresses application notices | `tests/themes.rs::deprecated_semantic_syntax_is_translated_and_emits_one_startup_notice`, `tests/pty_ui.rs::deprecated_theme_syntax_surfaces_a_native_startup_notice`, `local_and_remote_startup_notices_are_shown_in_order`, `tests/pty_pager.rs::patch_pager_suppresses_application_startup_notices` |
| Local copied-skill refresh notice after upgrades | verified | failure-tolerant version state in `src/startup_notice.rs`, disabled by `RAMO_DISABLE_UPDATE_NOTICE=1` or Hunk's compatibility variable | `tests/startup_notices.rs::copied_skill_refresh_notice_is_local_one_time_and_failure_tolerant`, `tests/pty_ui.rs::installed_version_change_surfaces_a_local_copied_skill_notice_once` |
| Bounded remote update notice | verified | optional literal-argv `git ls-remote` GitHub-tag query on a background thread; 1.2-second delay, five-second child kill, six-hour repeat, no TLS crate or mandatory helper | `tests/startup_notices.rs::remote_update_selection_matches_hunks_stable_and_prerelease_priority`, `tests/pty_ui.rs::remote_update_notice_uses_an_optional_nonblocking_git_query`, `slow_remote_update_query_is_killed_without_blocking_the_review`, `local_and_remote_startup_notices_are_shown_in_order` |

## Input and normalized model

| Capability | Status | Rust evidence | Verification evidence |
|---|---|---|---|
| One normalized `Changeset` model | verified | `src/core/changeset.rs` | `src/core/changeset.rs::tests` |
| Stable file identity across reloads/renames | verified | `stable_file_id`, `DiffFile::id` | `file_ids_are_stable_across_reloads`, `test_renamed_file_has_stable_previous_path` |
| Added/deleted/renamed/copied/modified metadata | verified | `FileChangeKind`, `parse_file` | parser tests for all five forms |
| Patch chunk retained per file | verified | `DiffFile::patch`, `format_patch` | `tests/input_loading.rs::each_file_retains_only_its_own_exact_patch_chunk` |
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
| File filter and focus handling | verified | typed filter input, atomic one-Escape cancellation, and derived visible files | `tests/review_state.rs`, `tests/ui_input.rs`, `tests/pty_ui.rs::filter_owns_literal_keys_and_tab_returns_to_help_and_review` |
| Explicit semantic cursor and split-side focus | verified | `ReviewController` owns stable row identity and `ReviewSide`; renderer paints the active cell with selection precedence | `tests/review_state.rs::semantic_cursor_moves_between_diff_rows_and_clamps_at_review_bounds`, `focused_side_tracks_row_availability_and_explicit_context_focus`, `tests/ui_render.rs::cursor_paints_the_focused_split_side_and_selection_overrides_it`, `tests/pty_ui.rs::semantic_navigation_moves_the_rendered_cursor_without_startup_input` |
| File identity without sidebar | verified | every geometry section owns a one-row file header | `review::geometry::tests::file_sections_include_separator_and_header_after_the_first_file`, `tests/ui_render.rs::first_file_header_is_visible_without_the_sidebar` |
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
| Inline AI/agent/user note cards | verified | controller-owned `HumanNote`, normalized external notes, canonical `ReviewRow::Note` geometry | `tests/notes_state.rs`, `tests/ui_render.rs::inline_agent_notes_render_inside_the_measured_review_stream`, `tests/pty_notes.rs` |
| Wide-character selection/copy correctness | verified | `review::selection` terminal-cell projection | `tests/review_selection.rs`, `tests/ui_render.rs::stable_selection_projection_is_painted_on_the_selected_terminal_cells` |
| Restrained contextual bottom status | verified | status renders only for filter/toast feedback | `tests/pty_watch.rs::reload_error_keeps_the_last_valid_review_visible`, filter/help ownership in `tests/pty_ui.rs` |
| No top menu bar/dropdowns | verified | intentional product exclusion; no menu components | source inspection plus pager/UI PTY absence assertions |
| Help dialog | verified | `DialogOverlay::Help` | dialog/input tests and filter/help PTY test |
| Theme selector dialog | verified | `DialogOverlay::Theme` | theme/dialog tests and save PTY test |
| Save-preferences confirmation | verified | `DialogOverlay::Save`, targeted config writer | config persistence and PTY tests |
| Agent-skill dialog | verified | direct `InputMode::AgentSkill` overlay and native OSC 52 copy | `tests/ui_input.rs::agent_skill_dialog_owns_copy_and_close_keys`, `tests/ui_dialogs.rs::overlays_render_centered_and_remain_usable_at_small_sizes`, `tests/pty_ui.rs::direct_agent_skill_dialog_copies_native_guidance_and_closes` |

### Keyboard actions

| Action | Status | Evidence |
|---|---|---|
| `j`/`k`, Up/Down semantic row movement | verified | typed cursor actions plus controller clamp and rendered PTY tests |
| `h`/`l` split-side focus | verified | side availability snapping in `ReviewController` plus input/render tests |
| Left/Right horizontal scrolling | verified | typed horizontal actions plus input/state tests |
| Space/`f` page down, `b` page up, Shift-Space | verified | `tests/ui_input.rs`, `tests/review_state.rs` |
| `d`/`u` half-page scrolling | verified | `tests/ui_input.rs`, `tests/review_state.rs` |
| `g`/`G`, Home/End bounds | verified | input and controller navigation tests |
| `[`/`]` hunk navigation | verified | input and wrapping navigation tests |
| `,`/`.` file navigation | verified | input, state, and multi-file PTY tests |
| `{`/`}` annotated-hunk navigation | verified | `tests/review_state.rs::annotated_navigation_respects_filter_and_empty_filters_are_safe` |
| `1`/`2`/`0` layout selection | verified | input/state tests and PTY controls test |
| `s` sidebar | verified | input/state/mouse tests |
| `t` theme selector | verified | input/dialog and PTY tests |
| `a` agent notes | verified | canonical external-note visibility toggle | `tests/notes_state.rs`, `tests/pty_notes.rs::agent_notes_toggle_in_the_live_review` |
| `A` agent-skill setup | verified | direct dialog action with native prompt copy | input, dialog, and `tests/pty_ui.rs::direct_agent_skill_dialog_copies_native_guidance_and_closes` |
| `z` unchanged context | verified | context controller/app and PTY tests |
| `n` line numbers | verified | input/state/render tests |
| `w` wrapping | verified | input/state/geometry tests |
| `m` hunk metadata | verified | input/state/render tests |
| `e` editor | verified | typed effect, literal native command runner, terminal handoff | `tests/editor.rs`, `tests/pty_watch.rs::editor_key_launches_literal_file_and_line_argv_then_resumes_the_review` |
| `r` reload | verified | typed effect through `WatchRuntime::manual_reload` | `tests/reload.rs`, `tests/pty_watch.rs::manual_r_reloads_a_direct_file_without_watch_mode` |
| `/` file filter | verified | state/input and literal-key PTY tests |
| `c` create review note | verified | controller-owned inline draft; note input owns literal keys | `tests/notes_state.rs`, `tests/pty_notes.rs::human_note_draft_owns_keys_saves_inline_and_exports_markdown`, `escape_cancels_a_fresh_inline_draft` |
| Tab focus toggle | verified | filter-aware typed input and PTY test |
| `?` help | verified | input/dialog and PTY tests |
| `q` quit | verified | typed quit/save reduction | pager/UI PTY restoration tests |
| Existing Vim selection/yank/tmux actions on new bindings | verified | stable review selection projection plus OSC 52/tmux routing | selection/input/render tests and `tests/integrations.rs::real_tmux_server_receives_the_exact_native_buffer` |

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
| File reload plan seam | verified | `ReloadPlan::Files`, `ReloadPlan::PatchFile`, `ReviewLoader::reload` | `tests/reload.rs::patch_file_reload_reads_replacement_contents`, `difftool_reload_preserves_the_display_path` |
| VCS reload plan seam | verified | `ReloadPlan::Vcs` records the selected native adapter | Git/JJ/SL loader tests plus `tests/watch.rs::git_is_hybrid_while_jj_and_sapling_are_poll_only` |
| Filesystem observation with debounce | verified | `src/watch/observer.rs`, `WatchCoordinator` | `tests/watch.rs::native_observer_sees_an_atomic_replacement_of_an_exact_file`, `bursts_coalesce_and_inflight_hints_get_one_trailing_generation`; `tests/pty_watch.rs::watch_mode_refreshes_after_an_atomic_save` |
| Polling fallback | verified | content fingerprint safety polls; two-second degraded interval | `tests/watch.rs::polling_fallback_reloads_only_when_the_content_fingerprint_changes`, `unavailable_native_observation_degrades_to_two_second_polling` |
| Serialized reload and stale-result protection | verified | single-loop `WatchRuntime`, generation acceptance, trailing dirty bit | `tests/watch.rs::bursts_coalesce_and_inflight_hints_get_one_trailing_generation` |
| Selection/viewport preservation on reload | verified | `ReviewController::replace_files` plus stable semantic row identity | `tests/reload.rs::replacing_files_preserves_selected_file_and_viewport_anchor`, cursor-highlight preservation in `tests/pty_watch.rs::watch_mode_refreshes_after_an_atomic_save` |
| Manual `r` reload | verified | `ReviewEffect::Reload` routes to `WatchRuntime::manual_reload` | `tests/pty_watch.rs::manual_r_reloads_a_direct_file_without_watch_mode` |
| Error display retains last valid review | verified | `WatchUpdate::Error` never replaces files | `tests/pty_watch.rs::reload_error_keeps_the_last_valid_review_visible`, `tests/watch.rs::failed_reload_keeps_the_applied_fingerprint_and_can_retry` |
| TTY replacement after piped input | verified | `src/runtime.rs::replace_stdin_with_tty` | `tests/runtime_resolution.rs::only_piped_stdin_needs_a_tty_replacement`, `tests/pty_pager.rs::patch_pager_enters_review_ui_and_quits_cleanly` |
| Terminal restoration on normal app error | verified | `TerminalSession` RAII plus explicit pre-propagation restore | `tests/pty_watch.rs::runtime_error_after_terminal_entry_restores_before_printing_the_error` |
| Panic restoration, suspend/resume, editor job control | verified | `src/terminal.rs`, owned suspend/resume boundary | panic ordering and Ctrl-Z/SIGCONT PTY tests in `tests/pty_watch.rs` |
| `$EDITOR` file/line launch | verified | `src/process/editor.rs`, literal `CommandRequest` argv | `tests/editor.rs`; `tests/pty_watch.rs::editor_key_launches_literal_file_and_line_argv_then_resumes_the_review` |
| Pi integration | verified | Rust installer writes embedded `src/pi_prompt.md`; no extension runtime | `tests/integrations.rs::pi_install_writes_a_markdown_prompt_and_no_typescript`, `pi_uninstall_preserves_unrelated_prompt_files` |
| tmux pane discovery/send | verified | injected `TmuxClient`, stable selection routing | exact literal argv/stdin/failure tests plus isolated `tests/integrations.rs::real_tmux_server_receives_the_exact_native_buffer` |
| OSC 52 clipboard | verified | `src/clipboard.rs`, shared selection projection | CJK mouse-selection PTY test asserts exact OSC 52 payload |
| Linux support | verified | native x86-64 build, full unit/integration/PTY/session suite, release target | 2026-07-21 locked release build plus full local gate; CI coverage also configured |
| macOS support | implemented | Unix paths and x86-64/ARM64 target checks | cross-platform CI workflow added; first remote run pending |
| Windows support | implemented | native `CONIN$`/`SetStdHandle` piped-review restoration and x86-64/ARM64 target checks | `tests/runtime_resolution.rs`; Windows CI runtime job added but first remote run pending |

## Notes, STML, and live sessions

| Capability | Status | Evidence |
|---|---|---|
| Agent-context JSON and narrative file order | verified | bounded normalization in `src/notes/context.rs`; `Changeset::apply_agent_context` | `tests/agent_context.rs` |
| Hunk/line-targeted AI and agent notes | verified | `src/notes/target.rs`, canonical note rows and annotated-hunk derivation | `tests/notes_state.rs`, `tests/ui_render.rs` |
| Human inline draft/edit/save/cancel | verified | `ReviewController` human-note authority plus `InputMode::Note` | `tests/notes_state.rs::controller_drafts_save_edit_cancel_remove_and_clear_by_stable_id`, `tests/pty_notes.rs` |
| Markdown review export | verified | normalized human note targets in `ReviewController::export_annotations`, `src/annotations/output.rs` | `tests/annotations.rs::export_contains_human_side_ranges_and_context_but_not_external_notes`, `tests/pty_notes.rs` |
| Deterministic STML parse/layout/render | verified | bounded parser, terminal-cell layout, symbolic renderer, bounded content-sensitive cache | `tests/stml_parse.rs`, `tests/stml_layout.rs`, `tests/ui_render.rs::inline_agent_markup_replaces_plain_fallback_and_keeps_semantic_span_style` |
| STML authoring guide | verified | embedded `src/markup/guide.md` | `tests/markup_cli.rs::guide_is_embedded_and_all_stml_fences_layout_at_reference_width` |
| Loopback same-binary daemon | verified | `tests/session_daemon.rs::installed_binary_serves_and_cli_commands_use_the_native_daemon_without_a_tui`; release binary inspection gate |
| API/daemon capability versions | verified | `tests/session_projection.rs::protocol_versions_capabilities_and_selectors_serialize_stably`, `tests/session_daemon.rs::health_capabilities_and_legacy_tombstone_are_bounded_json_routes` |
| Session registration/reconnection | verified | `tests/session_registration.rs::registration_snapshot_disconnect_and_reconnect_update_the_daemon_registry`, daemon restart with two real TUIs in `tests/session_e2e.rs` |
| Multi-session selectors and routing | verified | ID/repository/session-path selection, ambiguity, and isolated mutations in `tests/session_e2e.rs::two_live_terminals_route_isolated_commands_and_reconnect_before_idle_exit` |
| Session review/context projections | verified | `tests/session_projection.rs`, isolated live contexts in `tests/session_e2e.rs` |
| Session navigation/reload | verified | `tests/session_bridge.rs`, `tests/session_reload.rs`, `tests/pty_session.rs`, and two-PTY routing in `tests/session_e2e.rs` |
| Session comment add/apply/list/rm/clear | verified | `tests/session_bridge.rs`, `tests/pty_session.rs`, and isolated two-PTY note counts in `tests/session_e2e.rs` |
| Loopback enforcement, payload bounds, timeouts | verified | `tests/session_daemon.rs::session_api_enforces_method_content_type_body_limit_host_and_origin`, `tests/session_registration.rs::frames_are_versioned_big_endian_json_and_strictly_bounded` |
| Stale-daemon replacement | verified | `tests/session_daemon.rs::cli_replaces_a_stale_compatible_ramo_daemon_with_the_same_binary`; foreign-port refusal in `cli_does_not_replace_a_foreign_service_on_the_configured_port` |

## Performance and release closure

| Capability | Status | Evidence |
|---|---|---|
| Lazy syntax/full-source loading | verified | visible-window `HighlightCache`, cached `NativeContextSourceLoader` | highlighting, context, and render tests |
| Bounded highlight/geometry/markup caches | verified | bounded file/theme and per-line highlight LRUs, active shared geometry, 1 MiB/128-entry STML LRU | `tests/highlighting.rs::line_lru_is_bounded_while_scrolling_through_one_large_file`, highlighting and 100,000-row geometry tests; `tests/stml_layout.rs::repeated_layout_is_value_deterministic` |
| Shared geometry for rendering/interaction | verified | `ReviewGeometry`, shared row columns and hit tests | geometry, selection, mouse, and render tests |
| Large patch/many-file/non-ASCII benchmarks | verified | dependency-free `benches/parity.rs` release harness | `cargo bench --bench parity`; scenario output in `docs/performance.md` |
| Navigation/resize/watch memory checks | verified | bounded per-line highlight LRU and non-accumulating controller/context/watch generations | `tests/performance_bounds.rs`, `tests/highlighting.rs::line_lru_is_bounded_while_scrolling_through_one_large_file`, `review::context::tests::repeated_context_reads_reuse_one_entry_per_source_and_invalidation_releases_all`, `tests/watch.rs::repeated_manual_watch_generations_replace_one_snapshot_without_accumulating_results` |
| Local release artifact and linkage | verified | locked optimized x86-64 Linux build is one 8.6 MiB ELF executable | `file` and `ldd` show only the platform loader, libc, libm, and libgcc_s; source/runtime audit finds no JS/TS or JS runtime launch path |
| Cross-platform CLI/unit CI | implemented | `.github/workflows/ci.yml` portable Linux/macOS/Windows matrix plus separate Unix PTY jobs | local workflow parse and four non-host Rust target checks; first remote run pending |
| Unix PTY integration suite | verified | bounded portable-PTY harnesses | `tests/pty_pager.rs`, `tests/pty_ui.rs`, `tests/pty_notes.rs`, `tests/pty_watch.rs`, `tests/pty_session.rs`, and two-terminal `tests/session_e2e.rs` |
| Single-binary install/release documentation | implemented | `README.md`, `install.sh`, `install.ps1`, locked six-target release workflow | `tests/installers.rs` network-free archive selection; Windows PowerShell dry-runs run in CI; release artifact verified locally in slice 2 |
| Every in-scope row verified | missing | this ledger intentionally records remaining work | final audit pending |
