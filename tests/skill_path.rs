use assert_cmd::Command;

#[test]
fn skill_path_materializes_the_embedded_pdff_review_skill_without_a_runtime_bundle() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::cargo_bin("pdiff")
        .unwrap()
        .env("XDG_DATA_HOME", temp.path())
        .args(["skill", "path"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let path = String::from_utf8(output.stdout).unwrap();
    let path = std::path::Path::new(path.trim());
    assert!(path.is_file(), "{}", path.display());
    assert!(path.starts_with(temp.path()));
    let skill = std::fs::read_to_string(path).unwrap();
    assert!(skill.contains("name: pdiff-review"));
    assert!(skill.contains("pdiff session"));
    assert!(!skill.contains("hunk session"));
}
