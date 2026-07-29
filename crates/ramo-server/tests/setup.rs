use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ramo_server::setup::{
    CommandOutput, SetupEnvironment, SetupPaths, apply_setup, build_setup_plan,
};

#[test]
fn setup_binds_loopback_and_publishes_only_through_tailscale() {
    let harness = SetupHarness::all_dependencies();
    let plan = build_setup_plan(&harness, paths()).unwrap();

    assert_eq!(plan.bind_address, "127.0.0.1:47831");
    assert!(plan.render().contains("Server: /opt/ramo/ramo-server"));
    assert!(
        plan.systemd_unit
            .contains("ExecStart=/opt/ramo/ramo-server serve")
    );
    assert!(plan.systemd_unit.contains("Restart=on-failure"));
    assert!(plan.systemd_unit.contains("UMask=0077"));
    assert!(!plan.systemd_unit.contains("0.0.0.0"));
    assert_eq!(plan.public_endpoint, "https://archlinux.example.ts.net");
    assert_eq!(
        plan.tailscale_arguments,
        [
            "serve",
            "--bg",
            "--yes",
            "--https=443",
            "http://127.0.0.1:47831"
        ]
    );
}

#[test]
fn missing_dependency_fails_before_any_setup_mutation() {
    let harness = SetupHarness::all_dependencies();
    harness.executables.lock().unwrap().remove("tailscale");

    let error = build_setup_plan(&harness, paths()).unwrap_err();

    assert!(error.message.contains("tailscale"));
}

#[test]
fn failed_publication_restores_previous_service_and_endpoint_files() {
    let harness = SetupHarness::all_dependencies();
    let directory = tempfile::tempdir().unwrap();
    let paths = SetupPaths {
        server_executable: PathBuf::from("/opt/ramo/ramo-server"),
        unit_path: directory.path().join("ramo-server.service"),
        endpoint_path: directory.path().join("endpoint"),
    };
    std::fs::write(&paths.unit_path, "old unit").unwrap();
    std::fs::write(&paths.endpoint_path, "https://old.example.ts.net").unwrap();
    let plan = build_setup_plan(&harness, paths.clone()).unwrap();
    *harness.fail_program.lock().unwrap() = Some("tailscale".into());

    assert!(apply_setup(&harness, &plan, false).is_err());
    assert_eq!(
        std::fs::read_to_string(paths.unit_path).unwrap(),
        "old unit"
    );
    assert_eq!(
        std::fs::read_to_string(paths.endpoint_path).unwrap(),
        "https://old.example.ts.net"
    );
}

struct SetupHarness {
    executables: Mutex<HashMap<String, PathBuf>>,
    fail_program: Mutex<Option<String>>,
}

impl SetupHarness {
    fn all_dependencies() -> Self {
        Self {
            executables: Mutex::new(
                ["gh", "ollama", "tailscale", "systemctl"]
                    .into_iter()
                    .map(|name| (name.into(), PathBuf::from(format!("/usr/bin/{name}"))))
                    .collect(),
            ),
            fail_program: Mutex::new(None),
        }
    }
}

impl SetupEnvironment for SetupHarness {
    fn operating_system(&self) -> &'static str {
        "linux"
    }

    fn resolve_executable(&self, name: &str) -> Option<PathBuf> {
        self.executables.lock().unwrap().get(name).cloned()
    }

    fn run(&self, program: &Path, arguments: &[String]) -> Result<CommandOutput, std::io::Error> {
        let name = program.file_name().unwrap().to_string_lossy();
        let stdout = match (name.as_ref(), arguments) {
            ("tailscale", args) if args == ["status", "--json"] => {
                r#"{"Self":{"DNSName":"archlinux.example.ts.net."}}"#.into()
            }
            _ => String::new(),
        };
        Ok(CommandOutput {
            success: self.fail_program.lock().unwrap().as_deref() != Some(name.as_ref()),
            stdout,
        })
    }
}

fn paths() -> SetupPaths {
    SetupPaths {
        server_executable: PathBuf::from("/opt/ramo/ramo-server"),
        unit_path: PathBuf::from("/home/carlos/.config/systemd/user/ramo-server.service"),
        endpoint_path: PathBuf::from("/home/carlos/.config/ramo-server/endpoint"),
    }
}
