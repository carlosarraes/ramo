//! On-disk conversations, so closing a review does not end the thread.
//!
//! Modelled on the Review Map's server-side cache (`crates/ramo-server/src/cache.rs`): the same
//! atomic write, the same "delete and treat as a miss on any doubt" discipline, and the same trick
//! of hashing the format version into the filename so a bump is a clean miss rather than a parse
//! error. It differs in budget — conversations are capped by count, not bytes, because a
//! transcript is small and what matters is not accumulating one per pull request forever.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ramo_core::chat::{ConversationKey, conversation_key};

use super::{ChatState, ChatTurn};

/// Bumping this makes every existing entry unreachable rather than misread.
pub const CHAT_STORE_VERSION: u32 = 1;
const MAX_CONVERSATIONS: usize = 50;
const MAX_ENTRY_BYTES: u64 = 1024 * 1024;

static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredConversation {
    pub version: u32,
    pub key: ConversationKey,
    /// Kept even though it is derivable, so that changing how ids are minted is *detectable*
    /// rather than silently pointing a restored transcript at a session that never saw it.
    pub session_id: String,
    pub created_at: u64,
    pub last_accessed_at: u64,
    pub turns: Vec<ChatTurn>,
}

#[derive(Debug)]
pub struct ChatStore {
    directory: PathBuf,
}

impl ChatStore {
    /// A transcript is state rather than configuration, so it lives beside other state and not in
    /// the file the reviewer hand-edits.
    pub fn open() -> Option<Self> {
        let root = dirs::state_dir().or_else(dirs::data_local_dir)?;
        Some(Self {
            directory: root.join("ramo/chat"),
        })
    }

    pub fn with_directory(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn entry_path(&self, key: &ConversationKey) -> PathBuf {
        self.directory
            .join(format!("{}.json", conversation_key(key)))
    }

    /// Any doubt at all is a miss, and a doubtful file is removed rather than left to fail again.
    pub fn load(&self, key: &ConversationKey) -> Option<StoredConversation> {
        let path = self.entry_path(key);
        let readable = std::fs::metadata(&path).is_ok_and(|meta| meta.len() <= MAX_ENTRY_BYTES);
        if !readable {
            let _ = std::fs::remove_file(&path);
            return None;
        }
        let bytes = std::fs::read(&path).ok()?;
        match serde_json::from_slice::<StoredConversation>(&bytes) {
            Ok(entry) if entry.version == CHAT_STORE_VERSION && entry.key == *key => {
                let mut entry = entry;
                // A restored turn can never be resolved: its worker died with the last process, so
                // leaving it Pending would spin "thinking…" forever with nothing to answer it.
                for turn in &mut entry.turns {
                    if turn.state == ChatState::Pending {
                        turn.state = ChatState::Failed(INTERRUPTED.into());
                    }
                }
                Some(entry)
            }
            _ => {
                let _ = std::fs::remove_file(&path);
                None
            }
        }
    }

    pub fn save(
        &self,
        key: &ConversationKey,
        session_id: &str,
        turns: &[ChatTurn],
    ) -> std::io::Result<()> {
        create_private_directory(&self.directory)?;
        let now = unix_seconds();
        let created_at = self.load(key).map_or(now, |previous| previous.created_at);
        let entry = StoredConversation {
            version: CHAT_STORE_VERSION,
            key: key.clone(),
            session_id: session_id.to_owned(),
            created_at,
            last_accessed_at: now,
            // A turn still in flight is written as interrupted, never as pending.
            turns: turns
                .iter()
                .map(|turn| match turn.state {
                    ChatState::Pending => ChatTurn {
                        question: turn.question.clone(),
                        state: ChatState::Failed(INTERRUPTED.into()),
                    },
                    _ => turn.clone(),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&entry)?;
        atomic_write(&self.entry_path(key), &bytes)?;
        self.evict();
        Ok(())
    }

    /// Least recently used beyond the cap. Best effort: losing an old transcript is a smaller
    /// problem than failing a review over housekeeping.
    fn evict(&self) {
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return;
        };
        let mut found: Vec<(u64, PathBuf)> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .filter_map(|path| {
                let bytes = std::fs::read(&path).ok()?;
                match serde_json::from_slice::<StoredConversation>(&bytes) {
                    Ok(entry) => Some((entry.last_accessed_at, path)),
                    Err(_) => {
                        let _ = std::fs::remove_file(&path);
                        None
                    }
                }
            })
            .collect();
        if found.len() <= MAX_CONVERSATIONS {
            return;
        }
        found.sort_by_key(|(accessed, _)| *accessed);
        for (_, path) in found.iter().take(found.len() - MAX_CONVERSATIONS) {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub const INTERRUPTED: &str = "interrupted — ramo closed before the reply arrived";

fn create_private_directory(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent directory")
    })?;
    let id = TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(".ramo-{}-{id}.tmp", std::process::id()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        #[cfg(windows)]
        let _ = std::fs::remove_file(path);
        std::fs::rename(&temporary, path)?;
        #[cfg(unix)]
        std::fs::File::open(parent).and_then(|directory| directory.sync_all())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
