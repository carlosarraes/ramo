use std::io;
use std::path::Path;

use ramo::config::ResolvedConfig;
use ramo::core::input::{CommonOptions, ReviewInput, VcsId};
use ramo::vcs::{
    CommandOutput, CommandRunner, CommandSpec, VcsAdapter, VcsError, VcsLoadContext, VcsOperation,
};

#[test]
fn vcs_ids_are_closed_and_display_native_executable_names() {
    assert_eq!(VcsId::Git.executable(), "git");
    assert_eq!(VcsId::Jj.executable(), "jj");
    assert_eq!(VcsId::Sl.executable(), "sl");
}

#[test]
fn command_specs_are_argv_not_shell_strings() {
    let spec = CommandSpec::new("git", Path::new("/repo")).args([
        "diff",
        "--",
        "file; touch /tmp/not-run",
    ]);
    assert_eq!(spec.program, "git");
    assert_eq!(spec.args[2], "file; touch /tmp/not-run");
    assert_eq!(spec.accepted_exit_codes, vec![0]);
}

#[test]
fn neutral_operations_do_not_leak_cli_command_names() {
    assert_eq!(VcsOperation::WorkingTree.kind_name(), "working-tree diff");
    assert_eq!(VcsOperation::RevisionShow.kind_name(), "revision show");
    assert_eq!(VcsOperation::StashShow.kind_name(), "stash show");
}

#[test]
fn git_args_force_parseable_prefixes_and_preserve_pathspec_boundaries() {
    let args = ramo::vcs::git::build_git_diff_args(
        Some("main...HEAD"),
        true,
        &["src/lib.rs".into()],
        &[],
        false,
    );
    assert_eq!(
        &args[..8],
        [
            "-c",
            "diff.noprefix=false",
            "-c",
            "diff.mnemonicPrefix=false",
            "-c",
            "diff.srcPrefix=a/",
            "-c",
            "diff.dstPrefix=b/",
        ]
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--staged", "main...HEAD"])
    );
    assert_eq!(&args[args.len() - 2..], ["--", "src/lib.rs"]);
}

#[test]
fn git_show_and_stash_args_disable_external_diff_and_color() {
    let show = ramo::vcs::git::build_git_show_args(Some("HEAD^"), &["src".into()], false);
    assert!(show.windows(2).any(|pair| pair == ["show", "--format="]));
    assert!(show.iter().any(|arg| arg == "--no-ext-diff"));
    assert!(show.iter().any(|arg| arg == "--no-color"));
    assert_eq!(&show[show.len() - 2..], ["--", "src"]);

    let stash = ramo::vcs::git::build_git_stash_args(Some("stash@{1}"), false);
    assert!(stash.windows(3).any(|args| args == ["stash", "show", "-p"]));
    assert_eq!(stash.last().map(String::as_str), Some("stash@{1}"));
}

#[test]
fn nearest_checkout_wins_and_same_root_prefers_jj_then_sl_then_git() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".git")).unwrap();
    std::fs::create_dir_all(temp.path().join("nested/.git")).unwrap();
    std::fs::create_dir_all(temp.path().join("nested/.jj")).unwrap();
    std::fs::create_dir_all(temp.path().join("nested/src")).unwrap();
    let selected = ramo::vcs::detect::select_vcs(&temp.path().join("nested/src"), None).unwrap();
    assert_eq!(selected.id, VcsId::Jj);
    assert_eq!(selected.repo_root, temp.path().join("nested"));

    let explicit =
        ramo::vcs::detect::select_vcs(&temp.path().join("nested/src"), Some(VcsId::Git)).unwrap();
    assert_eq!(explicit.id, VcsId::Git);
    assert_eq!(explicit.repo_root, temp.path().join("nested"));
}

#[test]
fn upstream_mercurial_marker_is_not_misdetected_as_sapling() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".hg")).unwrap();
    std::fs::write(temp.path().join(".hg/requires"), "revlogv1\n").unwrap();
    assert_ne!(
        ramo::vcs::detect::select_vcs(temp.path(), None).map(|value| value.id),
        Some(VcsId::Sl)
    );
    std::fs::write(temp.path().join(".hg/requires"), "revlogv1\ntreestate\n").unwrap();
    assert_eq!(
        ramo::vcs::detect::select_vcs(temp.path(), None).unwrap().id,
        VcsId::Sl
    );
}

struct FailingRunner {
    kind: FailureKind,
}

enum FailureKind {
    Exit(&'static str),
    Missing,
}

impl CommandRunner for FailingRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, VcsError> {
        match self.kind {
            FailureKind::Exit(stderr) => Err(VcsError::Exit {
                program: spec.program.clone(),
                args: spec.args.clone(),
                code: 128,
                stderr: stderr.into(),
            }),
            FailureKind::Missing => Err(VcsError::Spawn {
                program: spec.program.clone(),
                source: io::Error::new(io::ErrorKind::NotFound, "not found"),
            }),
        }
    }
}

fn git_error(input: ReviewInput, failure: FailureKind) -> String {
    let runner = FailingRunner { kind: failure };
    let config = ResolvedConfig::default();
    let context = VcsLoadContext {
        cwd: Path::new("/repo"),
        config: &config,
        runner: &runner,
        git_executable: "git",
        jj_executable: "jj",
        sl_executable: "sl",
    };
    ramo::vcs::git::GitAdapter
        .load(&input, &context)
        .unwrap_err()
        .to_string()
}

#[test]
fn git_failures_are_operation_specific_and_actionable() {
    let not_repo = git_error(
        ReviewInput::VcsDiff {
            range: None,
            staged: false,
            pathspecs: vec![],
            options: CommonOptions::default(),
        },
        FailureKind::Exit("fatal: not a git repository"),
    );
    assert!(not_repo.contains("inside a Git repository"));

    let invalid_ref = git_error(
        ReviewInput::Show {
            reference: Some("missing-ref".into()),
            pathspecs: vec![],
            options: CommonOptions::default(),
        },
        FailureKind::Exit("fatal: bad revision 'missing-ref'"),
    );
    assert!(invalid_ref.contains("missing-ref"));
    assert!(invalid_ref.contains("Check the ref"));

    let missing_stash = git_error(
        ReviewInput::StashShow {
            reference: None,
            options: CommonOptions::default(),
        },
        FailureKind::Exit("No stash entries found."),
    );
    assert!(missing_stash.contains("git stash push"));

    let missing_git = git_error(
        ReviewInput::Show {
            reference: None,
            pathspecs: vec![],
            options: CommonOptions::default(),
        },
        FailureKind::Missing,
    );
    assert!(missing_git.contains("Git is required"));
    assert!(missing_git.contains("Install Git"));
}
