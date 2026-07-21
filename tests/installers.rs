#[cfg(unix)]
use std::path::Path;

const POWERSHELL_INSTALLER: &str = include_str!("../install.ps1");
#[cfg(unix)]
const UNIX_INSTALLER: &str = include_str!("../install.sh");

#[test]
fn powershell_installer_maps_both_windows_archives_and_has_a_network_free_dry_run() {
    for expected in ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"] {
        assert!(POWERSHELL_INSTALLER.contains(expected));
    }
    assert!(POWERSHELL_INSTALLER.contains("[switch]$DryRun"));
    assert!(POWERSHELL_INSTALLER.contains("ramo-${Target}.zip"));
}

#[cfg(unix)]
#[test]
fn unix_installer_dry_run_selects_archives_without_network_or_filesystem_mutation() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh");
    for (os, architecture, target) in [
        ("linux", "x86_64", "x86_64-unknown-linux-gnu"),
        ("darwin", "arm64", "aarch64-apple-darwin"),
    ] {
        let install = tempfile::tempdir().unwrap();
        let output = std::process::Command::new("bash")
            .arg(&script)
            .arg("v0.0.6")
            .env("RAMO_INSTALL_DRY_RUN", "1")
            .env("RAMO_INSTALL_OS", os)
            .env("RAMO_INSTALL_ARCH", architecture)
            .env("RAMO_INSTALL_DIR", install.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains(target), "{stdout}");
        assert!(
            stdout.contains(&format!(
                "https://github.com/carlosarraes/ramo/releases/download/v0.0.6/ramo-{target}.tar.gz"
            )),
            "{stdout}"
        );
        assert!(!install.path().join("ramo").exists());
    }
}

#[cfg(unix)]
#[test]
fn unix_installer_runs_from_bash_stdin_like_the_documented_curl_command() {
    let install = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new("bash")
        .args(["-s", "--", "v0.0.8"])
        .env("RAMO_INSTALL_DRY_RUN", "1")
        .env("RAMO_INSTALL_OS", "linux")
        .env("RAMO_INSTALL_ARCH", "x86_64")
        .env("RAMO_INSTALL_DIR", install.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), UNIX_INSTALLER.as_bytes()).unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(
            "https://github.com/carlosarraes/ramo/releases/download/v0.0.8/ramo-x86_64-unknown-linux-gnu.tar.gz"
        ),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn unix_installer_removes_only_the_confirmed_legacy_binary_in_its_install_directory() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh");
    assert!(!UNIX_INSTALLER.contains("command -v pdiff"));

    for (response, removed) in [("yes", true), ("no", false)] {
        let install = tempfile::tempdir().unwrap();
        let legacy = install.path().join("pdiff");
        std::fs::write(&legacy, "legacy").unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let unrelated = elsewhere.path().join("pdiff");
        std::fs::write(&unrelated, "unrelated").unwrap();

        let output = std::process::Command::new("bash")
            .args([
                "-c",
                "source \"$1\"; INSTALL_DIR=\"$2\"; RAMO_REMOVE_LEGACY=\"$3\"; remove_legacy_binary",
                "ramo-installer-test",
            ])
            .arg(&script)
            .arg(install.path())
            .arg(response)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(!legacy.exists(), removed);
        assert_eq!(std::fs::read_to_string(unrelated).unwrap(), "unrelated");
    }
}
