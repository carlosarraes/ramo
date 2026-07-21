use std::ffi::OsString;
use std::io;
use std::path::Path;

use ramo::process::command::{CommandExecutor, CommandRequest, CommandResult};
use ramo::process::editor::{
    EditorError, EditorLaunchError, EditorLauncher, build_editor_command, should_suspend_for_editor,
};

fn strings(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn editor_commands_are_literal_argv_with_native_line_conventions() {
    let path = Path::new("/tmp/a file.rs");
    assert_eq!(
        build_editor_command("nvim --clean", path, 17).unwrap().argv,
        strings(&["nvim", "--clean", "+17", "/tmp/a file.rs"])
    );
    assert_eq!(
        build_editor_command("code --wait", path, 17).unwrap().argv,
        strings(&["code", "--wait", "--goto", "/tmp/a file.rs:17",])
    );
    assert_eq!(
        build_editor_command("hx", path, 0).unwrap().argv,
        strings(&["hx", "/tmp/a file.rs:1"])
    );
    assert_eq!(
        build_editor_command("emacs -nw", path, 17).unwrap().argv,
        strings(&["emacs", "-nw", "/tmp/a file.rs"])
    );
}

#[test]
fn gui_editors_do_not_take_terminal_ownership() {
    assert!(!should_suspend_for_editor("code --wait").unwrap());
    assert!(!should_suspend_for_editor("C:\\Tools\\Cursor.exe --wait").unwrap());
    assert!(should_suspend_for_editor("vim").unwrap());
    assert!(should_suspend_for_editor("emacs -nw").unwrap());
}

#[test]
fn empty_and_malformed_editor_settings_are_distinct() {
    assert!(matches!(
        build_editor_command("  ", Path::new("a.rs"), 1),
        Err(EditorError::NotConfigured)
    ));
    assert!(matches!(
        build_editor_command("'unterminated", Path::new("a.rs"), 1),
        Err(EditorError::InvalidCommand(_))
    ));
}

struct FakeExecutor {
    requests: Vec<CommandRequest>,
    result: Option<io::Result<CommandResult>>,
}

impl CommandExecutor for FakeExecutor {
    fn execute(&mut self, request: CommandRequest) -> io::Result<CommandResult> {
        self.requests.push(request);
        self.result.take().unwrap()
    }
}

#[test]
fn launcher_inherits_stdio_and_reports_spawn_and_exit_failures() {
    let command = build_editor_command("nvim --clean", Path::new("/tmp/a file.rs"), 17).unwrap();
    let executor = FakeExecutor {
        requests: Vec::new(),
        result: Some(Ok(CommandResult {
            code: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })),
    };
    let mut launcher = EditorLauncher::new(executor);
    launcher.launch(&command).unwrap();
    let executor = launcher.into_executor();
    assert_eq!(executor.requests.len(), 1);
    assert_eq!(executor.requests[0].argv, command.argv);
    assert!(executor.requests[0].inherit_stdio);
    assert_eq!(executor.requests[0].stdin, None);

    let mut launcher = EditorLauncher::new(FakeExecutor {
        requests: Vec::new(),
        result: Some(Err(io::Error::new(io::ErrorKind::NotFound, "missing"))),
    });
    assert!(matches!(
        launcher.launch(&command),
        Err(EditorLaunchError::Spawn(_))
    ));

    let mut launcher = EditorLauncher::new(FakeExecutor {
        requests: Vec::new(),
        result: Some(Ok(CommandResult {
            code: Some(7),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })),
    });
    assert_eq!(
        launcher.launch(&command).unwrap_err().to_string(),
        "editor exited with status 7"
    );
}
