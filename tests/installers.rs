use std::path::Path;

const POWERSHELL_INSTALLER: &str = include_str!("../install.ps1");

#[test]
fn powershell_installer_maps_both_windows_archives_and_has_a_network_free_dry_run() {
    for expected in ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"] {
        assert!(POWERSHELL_INSTALLER.contains(expected));
    }
    assert!(POWERSHELL_INSTALLER.contains("[switch]$DryRun"));
    assert!(POWERSHELL_INSTALLER.contains("pdiff-${Target}.zip"));
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
            .env("PDIFF_INSTALL_DRY_RUN", "1")
            .env("PDIFF_INSTALL_OS", os)
            .env("PDIFF_INSTALL_ARCH", architecture)
            .env("PDIFF_INSTALL_DIR", install.path())
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
                "https://github.com/carlosarraes/pdiff/releases/download/v0.0.6/pdiff-{target}.tar.gz"
            )),
            "{stdout}"
        );
        assert!(!install.path().join("pdiff").exists());
    }
}
