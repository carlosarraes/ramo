use std::fs;
use std::path::{Path, PathBuf};

const STATE_VERSION: u64 = 1;
const DISABLE_UPDATE_NOTICE_ENV: &str = "PDIFF_DISABLE_UPDATE_NOTICE";
const HUNK_DISABLE_UPDATE_NOTICE_ENV: &str = "HUNK_DISABLE_UPDATE_NOTICE";

pub fn append_local_startup_notice(notices: &mut Vec<String>) {
    let disabled = [DISABLE_UPDATE_NOTICE_ENV, HUNK_DISABLE_UPDATE_NOTICE_ENV]
        .iter()
        .any(|key| std::env::var_os(key).is_some_and(|value| value == "1"));
    let Some(path) = state_path() else {
        return;
    };
    if let Some(notice) = resolve_skill_refresh_notice(&path, env!("CARGO_PKG_VERSION"), disabled) {
        notices.push(notice);
    }
}

pub fn resolve_skill_refresh_notice(
    state_path: &Path,
    installed_version: &str,
    disabled: bool,
) -> Option<String> {
    if disabled || installed_version.is_empty() {
        return None;
    }

    let previous_version = fs::read_to_string(state_path)
        .ok()
        .and_then(|source| serde_json::from_str::<serde_json::Value>(&source).ok())
        .and_then(|state| {
            state
                .get("lastSeenCliVersion")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });

    let parent = state_path.parent()?;
    if fs::create_dir_all(parent).is_err() {
        return None;
    }
    let state = serde_json::json!({
        "version": STATE_VERSION,
        "lastSeenCliVersion": installed_version,
    });
    let Ok(source) = serde_json::to_string_pretty(&state) else {
        return None;
    };
    if fs::write(state_path, format!("{source}\n")).is_err() {
        return None;
    }

    match previous_version {
        Some(previous) if previous != installed_version => Some(format!(
            "pdiff {installed_version} installed • If your agent copied pdiff's skill, run pdiff skill path"
        )),
        _ => None,
    }
}

fn state_path() -> Option<PathBuf> {
    dirs::config_dir().map(|path| path.join("pdiff/state.json"))
}
