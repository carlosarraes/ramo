use std::collections::VecDeque;
use std::io;
use std::io::Cursor;
use std::path::Path;
use std::sync::Mutex;

use pdiff::config::ResolvedConfig;
use pdiff::core::input::{CommonOptions, ReviewInput, VcsId};
use pdiff::input::{LoadContext, ReloadPlan, ReviewLoader};
use pdiff::vcs::{CommandOutput, CommandRunner, CommandSpec, VcsError};

enum Reply {
    Output {
        expected: Vec<&'static str>,
        stdout: String,
        stderr: String,
        code: i32,
    },
    Missing {
        expected: Vec<&'static str>,
    },
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
        assert_eq!(spec.program, "jj");
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
            Reply::Missing { expected } => {
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
        vcs: Some(VcsId::Jj),
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
fn jj_builders_use_git_format_and_fileset_boundary() {
    assert_eq!(
        pdiff::vcs::jj::build_jj_diff_args(Some("main..@"), &["src/lib.rs".into()]),
        ["diff", "--git", "-r", "main..@", "--", "src/lib.rs"]
    );
    assert_eq!(
        pdiff::vcs::jj::build_jj_show_args(None, &[]),
        ["diff", "--git", "-r", "@"]
    );
}

#[test]
fn jj_diff_and_show_load_git_patches_from_the_native_command_contract() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_string_lossy().into_owned();
    let runner = ScriptedRunner::new([
        output(
            &["--no-pager", "--color", "never", "root"],
            format!("{root}\n"),
        ),
        output(
            &["--no-pager", "--color", "never", "diff", "--git"],
            include_str!("fixtures/simple.patch"),
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
    assert_eq!(loaded.changeset.files[0].path, "src/main.rs");
    assert!(loaded.changeset.title.ends_with("working copy"));
    assert!(matches!(loaded.reload_plan, ReloadPlan::Vcs { .. }));
    runner.assert_finished();

    let runner = ScriptedRunner::new([
        output(
            &["--no-pager", "--color", "never", "root"],
            format!("{root}\n"),
        ),
        output(
            &[
                "--no-pager",
                "--color",
                "never",
                "diff",
                "--git",
                "-r",
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
    runner.assert_finished();
}

#[test]
fn jj_staged_and_stash_operations_fail_without_spawning() {
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
    assert!(staged.contains("Jujutsu has no staging area"));
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
    runner.assert_finished();
}

#[test]
fn jj_failures_name_the_tool_repo_and_revset() {
    let temp = tempfile::tempdir().unwrap();
    let input = ReviewInput::Show {
        reference: Some("missing".into()),
        pathspecs: vec![],
        options: CommonOptions::default(),
    };

    let missing = ScriptedRunner::new([Reply::Missing {
        expected: vec!["--no-pager", "--color", "never", "root"],
    }]);
    let message = load(&input, &missing, temp.path()).unwrap_err().to_string();
    assert!(message.contains("Jujutsu is required"));
    assert!(message.contains("Install Jujutsu"));

    let no_repo = ScriptedRunner::new([failure(
        &["--no-pager", "--color", "never", "root"],
        "Error: There is no jj repo in this directory",
    )]);
    let message = load(&input, &no_repo, temp.path()).unwrap_err().to_string();
    assert!(message.contains("inside a Jujutsu repository"));

    let root = temp.path().to_string_lossy().into_owned();
    let invalid = ScriptedRunner::new([
        output(
            &["--no-pager", "--color", "never", "root"],
            format!("{root}\n"),
        ),
        failure(
            &[
                "--no-pager",
                "--color",
                "never",
                "diff",
                "--git",
                "-r",
                "missing",
            ],
            "Error: Revision not found: missing",
        ),
    ]);
    let message = load(&input, &invalid, temp.path()).unwrap_err().to_string();
    assert!(message.contains("Jujutsu revset `missing`"));
    assert!(message.contains("Check the revset"));
}
