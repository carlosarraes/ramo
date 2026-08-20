//! Locating pi's own transcript for a conversation.
//!
//! pi scopes sessions by working directory and creates one on demand, so `--session-id` resumes a
//! thread across restarts. What it does *not* do is tell us clearly when the session is gone: a
//! missing id prints a warning to stderr and still exits 0, which ramo's success path discards.
//! Probing the directory ourselves lets the decision to replay be made before a request is spent.

use std::path::{Path, PathBuf};

/// pi's own mangling, from `getDefaultSessionDirPath`: the resolved cwd with its leading separator
/// stripped and every `/`, `\` and `:` turned into `-`, wrapped in a pair of double dashes.
pub fn project_session_dir(agent_sessions: &Path, project: &Path) -> PathBuf {
    let mangled: String = project
        .to_string_lossy()
        .trim_start_matches(['/', '\\'])
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' => '-',
            other => other,
        })
        .collect();
    agent_sessions.join(format!("--{mangled}--"))
}

/// Whether pi still holds a transcript for this id. Sessions are written as
/// `<timestamp>_<session id>.jsonl`, so the id is a suffix match rather than the whole name.
pub fn session_exists(agent_sessions: &Path, project: &Path, session_id: &str) -> bool {
    let directory = project_session_dir(agent_sessions, project);
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    let suffix = format!("_{session_id}.jsonl");
    entries.filter_map(Result::ok).any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.ends_with(&suffix))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_directory_matches_pis_own_mangling_rule() {
        // Verified against real directories under `~/.pi/agent/sessions`.
        assert_eq!(
            project_session_dir(
                Path::new("/home/x/.pi/agent/sessions"),
                Path::new("/home/x/projs/ramo")
            ),
            Path::new("/home/x/.pi/agent/sessions/--home-x-projs-ramo--")
        );
    }

    #[test]
    fn a_session_is_found_by_its_id_suffix_and_missing_ones_are_reported_absent() {
        let temp = tempfile::tempdir().unwrap();
        let sessions = temp.path();
        let project = Path::new("/home/x/projs/ramo");
        assert!(!session_exists(sessions, project, "ramo-abc"));

        let directory = project_session_dir(sessions, project);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("2026-08-20T10-00-00-000Z_ramo-abc.jsonl"),
            "{}",
        )
        .unwrap();

        assert!(session_exists(sessions, project, "ramo-abc"));
        assert!(
            !session_exists(sessions, project, "ramo-ab"),
            "a partial id must not match"
        );
    }
}
