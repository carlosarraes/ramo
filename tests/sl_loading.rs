use std::collections::VecDeque;
use std::io;
use std::io::Cursor;
use std::path::Path;
use std::sync::Mutex;

use pdiff::config::ResolvedConfig;
use pdiff::core::input::{CommonOptions, ReviewInput, VcsId};
use pdiff::input::{LoadContext, ReviewLoader};
use pdiff::vcs::{CommandOutput, CommandRunner, CommandSpec, VcsError};

enum Reply {
    Output {
        expected: Vec<&'static str>,
        stdout: String,
        stderr: String,
        code: i32,
    },
    Missing(Vec<&'static str>),
}

struct ScriptedRunner {
    replies: Mutex<VecDeque<Reply>>,
}

impl ScriptedRunner {
    fn new(replies: impl IntoIterator<Item = Reply>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().collect()),
        }
    }

    fn assert_finished(&self) {
        assert!(self.replies.lock().unwrap().is_empty());
    }
}

impl CommandRunner for ScriptedRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, VcsError> {
        assert_eq!(spec.program, "sl");
        match self.replies.lock().unwrap().pop_front().unwrap() {
            Reply::Output {
                expected,
                stdout,
                stderr,
                code,
            } => {
                assert_eq!(spec.args, expected);
                if spec.accepted_exit_codes.contains(&code) {
                    Ok(CommandOutput {
                        code,
                        stdout: stdout.into_bytes(),
                        stderr: stderr.into_bytes(),
                    })
                } else {
                    Err(VcsError::Exit {
                        program: spec.program.clone(),
                        args: spec.args.clone(),
                        code,
                        stderr,
                    })
                }
            }
            Reply::Missing(expected) => {
                assert_eq!(spec.args, expected);
                Err(VcsError::Spawn {
                    program: spec.program.clone(),
                    source: io::Error::new(io::ErrorKind::NotFound, "not found"),
                })
            }
        }
    }
}

fn output(expected: &[&'static str], stdout: impl Into<String>) -> Reply {
    Reply::Output {
        expected: expected.to_vec(),
        stdout: stdout.into(),
        stderr: String::new(),
        code: 0,
    }
}

fn failure(expected: &[&'static str], stderr: &str) -> Reply {
    Reply::Output {
        expected: expected.to_vec(),
        stdout: String::new(),
        stderr: stderr.into(),
        code: 1,
    }
}

fn load(
    input: &ReviewInput,
    runner: &ScriptedRunner,
    cwd: &Path,
) -> Result<pdiff::input::LoadedReview, pdiff::input::LoadError> {
    let config = ResolvedConfig {
        vcs: Some(VcsId::Sl),
        ..Default::default()
    };
    ReviewLoader.load_with_context(
        input,
        &mut Cursor::new([]),
        &LoadContext {
            cwd,
            config: &config,
            runner,
        },
    )
}

#[test]
fn sl_builders_use_git_format_and_show_change() {
    assert_eq!(
        pdiff::vcs::sl::build_sl_diff_args(Some("main::."), &["src".into()]),
        ["diff", "--git", "-r", "main::.", "--", "src"]
    );
    assert_eq!(
        pdiff::vcs::sl::build_sl_show_args(None, &[]),
        ["diff", "--git", "--change", "."]
    );
}

#[test]
fn sl_working_copy_loads_tracked_and_unknown_files() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("unknown.txt"), "unknown\n").unwrap();
    let root = temp.path().to_string_lossy().into_owned();
    let runner = ScriptedRunner::new([
        output(
            &["--noninteractive", "--color", "never", "root"],
            format!("{root}\n"),
        ),
        output(
            &["--noninteractive", "--color", "never", "diff", "--git"],
            include_str!("fixtures/simple.patch"),
        ),
        output(
            &[
                "--noninteractive",
                "--color",
                "never",
                "status",
                "--unknown",
                "--print0",
                "--root-relative",
            ],
            "? unknown.txt\0",
        ),
    ]);
    let loaded = load(
        &ReviewInput::VcsDiff {
            range: None,
            staged: false,
            pathspecs: vec![],
            options: CommonOptions::default(),
        },
        &runner,
        temp.path(),
    )
    .unwrap();
    assert_eq!(loaded.changeset.files.len(), 2);
    assert_eq!(loaded.changeset.files[1].path, "unknown.txt");
    assert!(loaded.changeset.files[1].is_untracked);
}

#[test]
fn sl_unknown_files_reuse_binary_and_large_file_policy() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("binary.dat"), [0, 1, 2]).unwrap();
    std::fs::write(temp.path().join("large.txt"), "line\n".repeat(250_000)).unwrap();
    let root = temp.path().to_string_lossy().into_owned();
    let runner = ScriptedRunner::new([
        output(
            &["--noninteractive", "--color", "never", "root"],
            format!("{root}\n"),
        ),
        output(
            &["--noninteractive", "--color", "never", "diff", "--git"],
            "",
        ),
        output(
            &[
                "--noninteractive",
                "--color",
                "never",
                "status",
                "--unknown",
                "--print0",
                "--root-relative",
            ],
            "? binary.dat\0? large.txt\0",
        ),
    ]);
    let loaded = load(
        &ReviewInput::VcsDiff {
            range: None,
            staged: false,
            pathspecs: vec![],
            options: CommonOptions::default(),
        },
        &runner,
        temp.path(),
    )
    .unwrap();
    assert!(loaded.changeset.files[0].is_binary);
    assert!(loaded.changeset.files[1].is_too_large);
    assert!(loaded.changeset.files[1].stats_truncated);
}

#[test]
fn sl_exclude_untracked_skips_the_status_command() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_string_lossy().into_owned();
    let runner = ScriptedRunner::new([
        output(
            &["--noninteractive", "--color", "never", "root"],
            format!("{root}\n"),
        ),
        output(
            &["--noninteractive", "--color", "never", "diff", "--git"],
            "",
        ),
    ]);
    let loaded = load(
        &ReviewInput::VcsDiff {
            range: None,
            staged: false,
            pathspecs: vec![],
            options: CommonOptions {
                exclude_untracked: Some(true),
                ..Default::default()
            },
        },
        &runner,
        temp.path(),
    )
    .unwrap();
    assert!(loaded.changeset.files.is_empty());
    runner.assert_finished();
}

#[test]
fn sl_show_and_pathspecs_are_literal_argv() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_string_lossy().into_owned();
    let runner = ScriptedRunner::new([
        output(
            &["--noninteractive", "--color", "never", "root"],
            format!("{root}\n"),
        ),
        output(
            &[
                "--noninteractive",
                "--color",
                "never",
                "diff",
                "--git",
                "--change",
                "change;safe",
                "--",
                "src/lib.rs",
            ],
            include_str!("fixtures/simple.patch"),
        ),
    ]);
    load(
        &ReviewInput::Show {
            reference: Some("change;safe".into()),
            pathspecs: vec!["src/lib.rs".into()],
            options: CommonOptions::default(),
        },
        &runner,
        temp.path(),
    )
    .unwrap();
}

#[test]
fn sl_rejects_staged_and_stash_before_spawning() {
    let temp = tempfile::tempdir().unwrap();
    let runner = ScriptedRunner::new([]);
    let staged = load(
        &ReviewInput::VcsDiff {
            range: None,
            staged: true,
            pathspecs: vec![],
            options: CommonOptions::default(),
        },
        &runner,
        temp.path(),
    )
    .unwrap_err()
    .to_string();
    assert!(staged.contains("Sapling has no staging area"));
    assert!(staged.contains("Remove `--staged`"));
    let stash = load(
        &ReviewInput::StashShow {
            reference: None,
            options: CommonOptions::default(),
        },
        &runner,
        temp.path(),
    )
    .unwrap_err()
    .to_string();
    assert!(stash.contains("does not support stash show"));
}

#[test]
fn sl_failures_name_the_tool_repo_and_revset() {
    let temp = tempfile::tempdir().unwrap();
    let input = ReviewInput::Show {
        reference: Some("missing".into()),
        pathspecs: vec![],
        options: CommonOptions::default(),
    };
    let missing = ScriptedRunner::new([Reply::Missing(vec![
        "--noninteractive",
        "--color",
        "never",
        "root",
    ])]);
    let message = load(&input, &missing, temp.path()).unwrap_err().to_string();
    assert!(message.contains("Sapling is required"));

    let no_repo = ScriptedRunner::new([failure(
        &["--noninteractive", "--color", "never", "root"],
        "abort: no repository found",
    )]);
    let message = load(&input, &no_repo, temp.path()).unwrap_err().to_string();
    assert!(message.contains("inside a Sapling repository"));

    let root = temp.path().to_string_lossy().into_owned();
    let invalid = ScriptedRunner::new([
        output(
            &["--noninteractive", "--color", "never", "root"],
            format!("{root}\n"),
        ),
        failure(
            &[
                "--noninteractive",
                "--color",
                "never",
                "diff",
                "--git",
                "--change",
                "missing",
            ],
            "abort: unknown revision 'missing'",
        ),
    ]);
    let message = load(&input, &invalid, temp.path()).unwrap_err().to_string();
    assert!(message.contains("Sapling revset `missing`"));
    assert!(message.contains("Check the revset"));
}
