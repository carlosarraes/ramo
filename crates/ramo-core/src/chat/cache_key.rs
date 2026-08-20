use sha2::{Digest, Sha256};

/// Everything that decides whether two conversations are the same one.
///
/// `project` is not redundant with `identity`: pi scopes its sessions by working directory, so the
/// same pull request reviewed from two worktrees is two separate threads on pi's side. Leaving it
/// out would hand a transcript to a session that never saw it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationKey {
    pub project: String,
    /// `github:owner/repo#482`, or `local:<path>` for a diff with no pull request.
    pub identity: String,
    pub version: u32,
}

/// Hashed with each field length-prefixed, so `("ab", "c")` cannot collide with `("a", "bc")`.
/// The version is part of the hash rather than a field to check afterwards, which turns a format
/// change into a clean miss instead of a parse error.
pub fn conversation_key(key: &ConversationKey) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, key.project.as_bytes());
    hash_field(&mut hasher, key.identity.as_bytes());
    hash_field(&mut hasher, &key.version.to_be_bytes());
    format!("{:x}", hasher.finalize())
}

fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(project: &str, identity: &str, version: u32) -> ConversationKey {
        ConversationKey {
            project: project.into(),
            identity: identity.into(),
            version,
        }
    }

    #[test]
    fn every_field_moves_the_key_and_fields_cannot_run_together() {
        let base = conversation_key(&key("/p", "github:owner/repo#1", 1));
        for other in [
            key("/q", "github:owner/repo#1", 1),
            key("/p", "github:owner/repo#2", 1),
            key("/p", "github:owner/repo#1", 2),
        ] {
            assert_ne!(base, conversation_key(&other));
        }
        // Length prefixing: the concatenation is the same, the key must not be.
        assert_ne!(
            conversation_key(&key("/ab", "c", 1)),
            conversation_key(&key("/a", "bc", 1))
        );
    }
}
