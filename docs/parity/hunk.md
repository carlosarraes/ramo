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
| One Rust `pdiff` executable; no JS runtime | verified | `Cargo.toml`, `src/main.rs` | `cargo build`, `tests/library_surface.rs::parser_is_available_from_the_library_crate` |
| Reusable Rust library surface | verified | `src/lib.rs` | `tests/library_surface.rs::parser_is_available_from_the_library_crate` |
| Bare terminal invocation prints help | verified | `src/cli/normalize.rs::normalize` | `tests/cli_parse.rs::help_and_version_are_successful_print_actions` |
| Bare piped invocation means patch stdin | verified | `src/cli/normalize.rs::normalize` | `tests/cli_parse.rs::bare_pipe_is_patch_stdin` |
| `pdiff diff [target] [-- pathspecs]` | implemented | `src/cli/normalize.rs::normalize_diff` | parsing: `tests/cli_parse.rs::diff_supports_range_flags_and_pathspecs`; VCS loader missing |
| `pdiff diff --staged` | implemented | `src/cli/normalize.rs::normalize_diff` | normalization: `tests/cli_parse.rs::cached_alias_and_boolean_overrides_are_normalized`; VCS loader missing |
| `pdiff diff --cached` | implemented | `src/cli/normalize.rs::normalize_diff` | normalization: `tests/cli_parse.rs::cached_alias_and_boolean_overrides_are_normalized`; VCS loader missing |
| `pdiff diff <left-file> <right-file>` | verified | `src/input/file_pair.rs::load` | `tests/cli_parse.rs::existing_two_file_operands_become_a_file_pair`, `tests/input_loading.rs::direct_files_are_diffed_without_an_external_program` |
| `pdiff show [target] [-- pathspecs]` | implemented | `src/cli/normalize.rs::normalize_show` | parsing: `tests/cli_parse.rs::show_and_stash_show_preserve_their_references`; VCS loader missing |
| `pdiff stash show [ref]` | implemented | `src/cli/normalize.rs::normalize_stash_show` | parsing: `tests/cli_parse.rs::show_and_stash_show_preserve_their_references`; VCS loader missing |
| `pdiff patch [file|-]` | verified | `src/input/patch.rs::load` | `tests/input_loading.rs::patch_stdin_loads_a_changeset`, `patch_file_uses_its_path_as_source_context` |
| `pdiff pager` diff detection and text fallback | implemented | typed `ReviewInput::Pager` | parsing: `tests/cli_parse.rs::patch_dash_and_pager_have_distinct_input_kinds`; loader missing |
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
| `--mode auto|split|stack` | implemented | `src/cli/args.rs::LayoutArg`, `src/config/model.rs` | parse/precedence: `tests/cli_parse.rs::diff_supports_range_flags_and_pathspecs`, `tests/config_resolution.rs::builtin_user_repo_command_and_cli_layers_merge_in_order`; UI wiring missing |
| `--watch` | implemented | normalized `CommonOptions::watch` | parsing: `tests/cli_parse.rs::difftool_preserves_display_path_and_watch`; watcher missing |
| `--theme` | implemented | normalized/configured theme id | config resolution tests; theme registry and UI wiring missing |
| `--agent-context` | implemented | normalized path | CLI parser coverage; loader and notes missing |
| `--pager` | implemented | normalized pager flag and pager config layer | `tests/config_resolution.rs::pager_section_overrides_command_section_for_pager_chrome`; pager UI missing |
| `--line-numbers` / `--no-line-numbers` | implemented | normalized/configured boolean | `tests/cli_parse.rs::cached_alias_and_boolean_overrides_are_normalized`; UI wiring missing |
| `--wrap` / `--no-wrap` | implemented | normalized/configured boolean | CLI/config tests; UI wiring missing |
| `--hunk-headers` / `--no-hunk-headers` | implemented | normalized/configured boolean | CLI/config tests; UI wiring missing |
| `--agent-notes` / `--no-agent-notes` | implemented | normalized/configured boolean | CLI/config tests; note UI missing |
| `--transparent-bg` / `--no-transparent-bg` | implemented | normalized/configured boolean | CLI/config tests; theme wiring missing |
| `--exclude-untracked` / inverse | implemented | normalized/configured boolean | `tests/cli_parse.rs::cached_alias_and_boolean_overrides_are_normalized`; VCS loader missing |
| Built-in/user/repo/command/pager/CLI precedence | verified | `src/config/load.rs::ConfigResolver` | seven cases in `tests/config_resolution.rs` |
| Platform user config path and nearest `.pdiff/config.toml` | verified | `src/config/load.rs::ConfigPaths::discover` | `tests/config_resolution.rs::discovery_chooses_the_nearest_repository_config` |
| Unknown/malformed config diagnostics | verified | `src/config/load.rs::validate_keys` | `tests/config_resolution.rs::malformed_and_unknown_config_errors_name_the_file_and_key` |
| Save changed view preferences on quit | missing | — | — |
| Built-in and custom theme definitions | missing | — | — |
| Terminal background auto-detection | missing | — | — |
| Legacy Hunk theme aliases/syntax translation | missing | — | — |

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
| Git working tree/range/pathspec loader | missing | — | — |
| Git staged loader | missing | — | — |
| Git untracked-file inclusion | missing | — | — |
| Git show and stash loader | missing | — | — |
| Git moved-line classification | missing | — | — |
| Jujutsu detection and loader | missing | — | — |
| Sapling detection and loader | missing | — | — |
| Lazy old/new source fetchers | missing | — | — |
| Large-file placeholders and truncated stats | missing | model fields only | — |
| Diff-aware pager and safe plain-text pager | missing | — | — |

## Review UI and controls

| Capability | Status | Rust evidence | Verification evidence |
|---|---|---|---|
| Continuous multi-file review stream | implemented | legacy `src/app.rs`, `src/ui/side_by_side.rs` | PTY parity test missing |
| Sidebar navigates rather than filters stream | implemented | legacy file list | Hunk-compatible behavior test missing |
| File filter and focus handling | missing | — | — |
| Responsive `auto` split/stack | missing | — | — |
| Explicit split layout | implemented | legacy side-by-side renderer | Hunk geometry/PTY test missing |
| Explicit stack layout | implemented | legacy unified renderer | Hunk geometry/PTY test missing |
| Resize anchor preservation | missing | — | — |
| Row/file windowing and adaptive overscan | missing | — | — |
| Syntax highlighting | implemented | `src/ui/highlight.rs` | render-model test missing |
| Character-level changed-content emphasis | missing | — | — |
| Line numbers and change markers | implemented | legacy renderer | PTY toggle test missing |
| Moved-line colors | missing | — | — |
| Optional hunk headers | implemented | legacy renderer | Hunk shortcut/config wiring missing |
| Wrapping and horizontal scroll | missing | — | — |
| Collapsed context and per-hunk expansion | missing | — | — |
| Binary/large/untracked/rename file UI | implemented | normalized metadata and legacy rename header | complete render tests missing |
| Inline AI/agent/user note cards | missing | legacy human comments are not parity note cards | — |
| Wide-character selection/copy correctness | missing | — | — |
| Restrained contextual bottom status | missing | legacy status always visible | — |
| No top menu bar/dropdowns | verified | intentional product exclusion; no menu components | source inspection and design spec |
| Help dialog | missing | — | — |
| Theme selector dialog | missing | — | — |
| Save-preferences confirmation | missing | — | — |
| Agent-skill dialog | missing | — | — |

### Keyboard actions

| Action | Status | Evidence |
|---|---|---|
| Arrow step scrolling | implemented | legacy `App::handle_nav_key`; Hunk PTY test missing |
| Space/`f` page down, `b` page up, Shift-Space | missing | — |
| `d`/`u` half-page scrolling | implemented | legacy Ctrl-d/Ctrl-u differs from Hunk |
| `g`/`G`, Home/End bounds | implemented | legacy partial bindings; parity tests missing |
| `[`/`]` hunk navigation | implemented | legacy functions; PTY tests missing |
| `,`/`.` file navigation | missing | legacy uses `H`/`L` |
| `{`/`}` annotated-hunk navigation | missing | — |
| `1`/`2`/`0` layout selection | missing | legacy uses Tab |
| `s` sidebar | missing | legacy uses `e` |
| `t` theme selector | missing | legacy tmux conflict must be remapped |
| `a` agent notes | missing | — |
| `z` unchanged context | missing | — |
| `l` line numbers | missing | legacy side focus conflict must be remapped |
| `w` wrapping | missing | — |
| `m` hunk metadata | missing | — |
| `e` editor | missing | — |
| `r` reload | missing | — |
| `/` file filter | missing | legacy text search is additive and must be remapped |
| `c` create review note | implemented | legacy human comment; inline parity behavior missing |
| Tab focus toggle | missing | legacy layout toggle must be remapped |
| `?` help | missing | — |
| `q` quit | implemented | legacy app; PTY test missing |
| Existing Vim selection/yank/tmux actions on new bindings | implemented | legacy model/functions; rebinding and tests missing |

### Mouse actions

| Action | Status | Evidence |
|---|---|---|
| Wheel vertical scrolling | missing | — |
| Shift-wheel horizontal scrolling | missing | — |
| Sidebar selection | missing | — |
| Scrollbar interaction | missing | — |
| Context expansion | missing | — |
| Text selection/copy | missing | — |

## Watch, process, and integrations

| Capability | Status | Rust evidence | Verification evidence |
|---|---|---|---|
| File/VCS reload plan seam | implemented | `ReloadPlan` | file plan: `identical_files_produce_an_empty_changeset_with_a_reload_plan`; VCS plans missing |
| Filesystem observation with debounce | missing | — | — |
| Polling fallback | missing | — | — |
| Serialized reload and stale-result protection | missing | — | — |
| Selection/viewport preservation on reload | missing | — | — |
| Manual `r` reload | missing | — | — |
| Error display retains last valid review | missing | — | — |
| TTY replacement after piped input | implemented | `src/runtime.rs::replace_stdin_with_tty` | decision: `only_piped_stdin_needs_a_tty_replacement`; PTY test missing |
| Terminal restoration on normal app error | implemented | `src/runtime.rs::run_review` | PTY test missing |
| Panic restoration, suspend/resume, editor job control | missing | — | — |
| `$EDITOR` file/line launch | missing | — | — |
| Pi integration | implemented | `src/pi_extension.rs` | filesystem integration test missing |
| tmux pane discovery/send | implemented | `src/tmux.rs` | integration tests and new key mapping missing |
| OSC 52 clipboard | implemented | `src/clipboard.rs` | output test missing |
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
| Lazy syntax/full-source loading | missing | — |
| Bounded highlight/geometry caches | missing | — |
| Shared geometry for rendering/interaction | missing | — |
| Large patch/many-file/non-ASCII benchmarks | missing | — |
| Navigation/resize/watch memory checks | missing | — |
| Cross-platform CLI/unit CI | missing | — |
| Unix PTY integration suite | missing | — |
| Single-binary install/release documentation | implemented | `README.md`, `install.sh` | release artifact smoke tests missing |
| Every in-scope row verified | missing | this ledger intentionally records remaining work | final audit pending |
