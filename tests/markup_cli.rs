use assert_cmd::Command;

#[test]
fn render_accepts_files_stdin_plain_text_and_json_without_entering_the_tui() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("note.stml");
    std::fs::write(&path, "<badge color=success>OK</badge> ready").unwrap();
    let output = Command::cargo_bin("ramo")
        .unwrap()
        .args([
            "markup",
            "render",
            path.to_str().unwrap(),
            "--width",
            "20",
            "--color",
            "never",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains(" OK  ready"));
    assert!(
        !output
            .stdout
            .windows(8)
            .any(|bytes| bytes == b"\x1b[?1049h")
    );

    let output = Command::cargo_bin("ramo")
        .unwrap()
        .args(["markup", "render", "-", "--width", "12", "--json"])
        .write_stdin("<b>hello</b>")
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["width"], 12);
    assert_eq!(value["lines"][0], "hello");
    assert!(value["notes"].as_array().unwrap().is_empty());
}

#[test]
fn guide_is_embedded_and_all_stml_fences_layout_at_reference_width() {
    let output = Command::cargo_bin("ramo")
        .unwrap()
        .args(["markup", "guide"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let guide = String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n");
    assert!(guide.contains("ramo markup render"));
    assert!(!guide.contains("hunk markup render"));
    let mut rest = guide.as_str();
    let mut count = 0;
    while let Some(start) = rest.find("```stml\n") {
        let body = &rest[start + 8..];
        let end = body.find("```").unwrap();
        let result = ramo::markup::layout_stml(body[..end].trim_end(), 56);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        count += 1;
        rest = &body[end + 3..];
    }
    assert!(count >= 5);
}

#[test]
fn render_reports_missing_files_and_invalid_widths_before_terminal_startup() {
    Command::cargo_bin("ramo")
        .unwrap()
        .args(["markup", "render", "missing.stml"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("missing.stml"));
    Command::cargo_bin("ramo")
        .unwrap()
        .args(["markup", "render", "-", "--width", "0"])
        .assert()
        .failure();
}

#[test]
fn render_color_modes_resolve_symbolic_named_and_hex_colors_natively() {
    let output = Command::cargo_bin("ramo")
        .unwrap()
        .args(["markup", "render", "--width", "20", "--color", "always"])
        .write_stdin("<color fg=#0f0>hex</color> <color fg=orange>named</color>")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("\x1b[38;2;0;255;0mhex\x1b[0m"),
        "{stdout:?}"
    );
    assert!(
        stdout.contains("\x1b[38;2;224;135;61mnamed\x1b[0m"),
        "{stdout:?}"
    );

    let output = Command::cargo_bin("ramo")
        .unwrap()
        .args(["markup", "render", "--json", "--color", "always"])
        .write_stdin("<b>plain json</b>")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!output.stdout.contains(&0x1b));
}
