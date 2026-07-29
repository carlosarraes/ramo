use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ramo_core::review_map::{
    ReviewMap, ReviewMapCacheIdentity, ReviewMapFailureCode, review_map_cache_key,
    validate_exact_map,
};

use crate::ReviewMapFailure;

static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheLimits {
    pub max_bytes: u64,
    pub max_age: Duration,
}

#[derive(Debug, Clone)]
pub struct ReviewMapCache {
    directory: PathBuf,
    limits: CacheLimits,
    access: Arc<Mutex<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntryInfo {
    pub file_name: String,
    pub size: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    cache_identity: ReviewMapCacheIdentity,
    map: ReviewMap,
    created_at: u64,
    last_accessed_at: u64,
}

impl ReviewMapCache {
    pub fn new(directory: impl AsRef<Path>, limits: CacheLimits) -> Result<Self, ReviewMapFailure> {
        let directory = directory.as_ref().to_owned();
        create_private_directory(&directory)?;
        Ok(Self {
            directory,
            limits,
            access: Arc::new(Mutex::new(())),
        })
    }

    pub fn entry_path(&self, identity: &ReviewMapCacheIdentity) -> PathBuf {
        self.directory
            .join(format!("{}.json", review_map_cache_key(identity)))
    }

    pub fn get(
        &self,
        identity: &ReviewMapCacheIdentity,
    ) -> Result<Option<ReviewMap>, ReviewMapFailure> {
        let _guard = self.lock()?;
        let path = self.entry_path(identity);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(cache_io("Could not read the Review Map cache", error)),
        };
        let mut entry = match serde_json::from_slice::<CacheEntry>(&bytes) {
            Ok(entry) if valid_entry(identity, &entry) => entry,
            _ => {
                remove_if_present(&path)?;
                return Ok(None);
            }
        };
        let now = unix_seconds();
        if now.saturating_sub(entry.last_accessed_at) > self.limits.max_age.as_secs() {
            remove_if_present(&path)?;
            return Ok(None);
        }
        entry.last_accessed_at = now;
        atomic_write(&path, &serde_json::to_vec(&entry).map_err(cache_encode)?)?;
        let map = entry.map;
        self.evict_locked(now)?;
        Ok(Some(map))
    }

    pub fn put(
        &self,
        identity: &ReviewMapCacheIdentity,
        map: &ReviewMap,
    ) -> Result<(), ReviewMapFailure> {
        let _guard = self.lock()?;
        if !identity_matches_map(identity, map) || validate_exact_map(map).is_err() {
            return Err(ReviewMapFailure::new(
                ReviewMapFailureCode::CacheUnavailable,
                "Refused to cache an incompatible Review Map",
            ));
        }
        let now = unix_seconds();
        let entry = CacheEntry {
            cache_identity: identity.clone(),
            map: map.clone(),
            created_at: now,
            last_accessed_at: now,
        };
        atomic_write(
            &self.entry_path(identity),
            &serde_json::to_vec(&entry).map_err(cache_encode)?,
        )?;
        self.evict_locked(now)
    }

    pub fn list(&self) -> Result<Vec<CacheEntryInfo>, ReviewMapFailure> {
        let _guard = self.lock()?;
        let mut entries = Vec::new();
        for item in std::fs::read_dir(&self.directory)
            .map_err(|error| cache_io("Could not inspect the Review Map cache", error))?
        {
            let item = item.map_err(|error| cache_io("Could not inspect a cache entry", error))?;
            let path = item.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                entries.push(CacheEntryInfo {
                    file_name: item.file_name().to_string_lossy().into_owned(),
                    size: item
                        .metadata()
                        .map_err(|error| cache_io("Could not inspect a cache entry", error))?
                        .len(),
                });
            }
        }
        entries.sort_by(|left, right| left.file_name.cmp(&right.file_name));
        Ok(entries)
    }

    pub fn clear(&self) -> Result<usize, ReviewMapFailure> {
        let _guard = self.lock()?;
        let mut removed = 0;
        for item in std::fs::read_dir(&self.directory)
            .map_err(|error| cache_io("Could not inspect the Review Map cache", error))?
        {
            let path = item
                .map_err(|error| cache_io("Could not inspect a cache entry", error))?
                .path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                remove_if_present(&path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, ReviewMapFailure> {
        self.access.lock().map_err(|_| {
            ReviewMapFailure::new(
                ReviewMapFailureCode::CacheUnavailable,
                "The Review Map cache lock is unavailable",
            )
        })
    }

    fn evict_locked(&self, now: u64) -> Result<(), ReviewMapFailure> {
        let mut entries = Vec::new();
        for item in std::fs::read_dir(&self.directory)
            .map_err(|error| cache_io("Could not inspect the Review Map cache", error))?
        {
            let item = item.map_err(|error| cache_io("Could not inspect a cache entry", error))?;
            let path = item.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return Err(cache_io("Could not inspect a cache entry", error));
                }
            };
            let Ok(entry) = serde_json::from_slice::<CacheEntry>(&bytes) else {
                remove_if_present(&path)?;
                continue;
            };
            if now.saturating_sub(entry.last_accessed_at) > self.limits.max_age.as_secs() {
                remove_if_present(&path)?;
                continue;
            }
            entries.push((entry.last_accessed_at, bytes.len() as u64, path));
        }
        entries.sort_by_key(|entry| entry.0);
        let mut total = entries.iter().map(|entry| entry.1).sum::<u64>();
        for (_, size, path) in entries {
            if total <= self.limits.max_bytes {
                break;
            }
            remove_if_present(&path)?;
            total = total.saturating_sub(size);
        }
        Ok(())
    }
}

fn valid_entry(identity: &ReviewMapCacheIdentity, entry: &CacheEntry) -> bool {
    entry.cache_identity == *identity
        && identity_matches_map(identity, &entry.map)
        && validate_exact_map(&entry.map).is_ok()
}

fn identity_matches_map(identity: &ReviewMapCacheIdentity, map: &ReviewMap) -> bool {
    identity.repository == map.identity.repository
        && identity.pull_request == map.identity.pull_request
        && identity.head_sha == map.identity.head_sha
        && identity.schema_version == map.schema_version
}

fn create_private_directory(path: &Path) -> Result<(), ReviewMapFailure> {
    std::fs::create_dir_all(path)
        .map_err(|error| cache_io("Could not create the Review Map cache", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| cache_io("Could not secure the Review Map cache", error))?;
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ReviewMapFailure> {
    let parent = path.parent().ok_or_else(|| {
        ReviewMapFailure::new(
            ReviewMapFailureCode::CacheUnavailable,
            "The Review Map cache path has no parent directory",
        )
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
        let mut file = options
            .open(&temporary)
            .map_err(|error| cache_io("Could not create a temporary cache entry", error))?;
        file.write_all(bytes)
            .map_err(|error| cache_io("Could not write a cache entry", error))?;
        file.sync_all()
            .map_err(|error| cache_io("Could not sync a cache entry", error))?;
        std::fs::rename(&temporary, path)
            .map_err(|error| cache_io("Could not replace a cache entry", error))?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| cache_io("Could not sync the cache directory", error))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn remove_if_present(path: &Path) -> Result<(), ReviewMapFailure> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(cache_io("Could not remove a cache entry", error)),
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cache_io(message: &'static str, error: std::io::Error) -> ReviewMapFailure {
    ReviewMapFailure::with_source(ReviewMapFailureCode::CacheUnavailable, message, error)
}

fn cache_encode(error: serde_json::Error) -> ReviewMapFailure {
    ReviewMapFailure::with_source(
        ReviewMapFailureCode::CacheUnavailable,
        "Could not encode a Review Map cache entry",
        error,
    )
}
