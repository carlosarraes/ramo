use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{io::Write, path::Path};

use base64::Engine;
use ramo_core::review_map::{ReviewMapFailureCode, ReviewMapFailureCode::PairingRejected};
use rand::RngCore;
use sha2::{Digest, Sha256};
use subtle::{Choice, ConstantTimeEq};

use crate::ReviewMapFailure;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClientCredential {
    pub client_id: String,
    pub token: String,
}

#[derive(Clone)]
pub struct ReviewMapClientTokenStore {
    inner: Arc<TokenStoreInner>,
}

struct TokenStoreInner {
    records: Mutex<Vec<ClientRecord>>,
    path: Option<std::path::PathBuf>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ClientRecord {
    client_id: String,
    label: String,
    created_at: u64,
    token_digest: [u8; 32],
}

impl ReviewMapClientTokenStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ReviewMapFailure> {
        let path = path.as_ref().to_owned();
        let records = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
                ReviewMapFailure::with_source(
                    ReviewMapFailureCode::ClientUnauthorized,
                    "The paired-client store is malformed",
                    error,
                )
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(token_io("Could not read the paired-client store", error)),
        };
        Ok(Self {
            inner: Arc::new(TokenStoreInner {
                records: Mutex::new(records),
                path: Some(path),
            }),
        })
    }

    pub fn issue(&self, label: impl Into<String>) -> Result<ClientCredential, ReviewMapFailure> {
        let token = format!("ramo_{}", random_url_token(32));
        let client_id = random_url_token(12);
        let mut records = self
            .inner
            .records
            .lock()
            .map_err(|_| auth_state_failure())?;
        records.push(ClientRecord {
            client_id: client_id.clone(),
            label: label.into(),
            created_at: unix_seconds(),
            token_digest: digest(&token),
        });
        self.persist(&records)?;
        Ok(ClientCredential { client_id, token })
    }

    pub fn authorize(&self, token: &str) -> bool {
        let candidate = digest(token);
        self.inner.records.lock().is_ok_and(|records| {
            let authorized = records.iter().fold(Choice::from(0), |authorized, record| {
                authorized | record.token_digest.ct_eq(&candidate)
            });
            bool::from(authorized)
        })
    }

    pub fn revoke(&self, client_id: &str) -> Result<bool, ReviewMapFailure> {
        let mut records = self
            .inner
            .records
            .lock()
            .map_err(|_| auth_state_failure())?;
        let before = records.len();
        records.retain(|record| record.client_id != client_id);
        let changed = records.len() != before;
        if changed {
            self.persist(&records)?;
        }
        Ok(changed)
    }

    pub fn client_count(&self) -> usize {
        self.inner.records.lock().map_or(0, |records| records.len())
    }

    fn persist(&self, records: &[ClientRecord]) -> Result<(), ReviewMapFailure> {
        let Some(path) = &self.inner.path else {
            return Ok(());
        };
        persist_records(path, records)
    }
}

impl Default for ReviewMapClientTokenStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(TokenStoreInner {
                records: Mutex::new(Vec::new()),
                path: None,
            }),
        }
    }
}

impl std::fmt::Debug for ReviewMapClientTokenStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let records = self.inner.records.lock().ok();
        let clients = records
            .as_deref()
            .map(|records| {
                records
                    .iter()
                    .map(|record| (&record.client_id, &record.label, record.created_at))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        formatter
            .debug_struct("ReviewMapClientTokenStore")
            .field("clients", &clients)
            .field("tokens", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone)]
pub struct PairingState {
    tokens: ReviewMapClientTokenStore,
    codes: Arc<Mutex<Vec<PairingRecord>>>,
    path: Option<Arc<std::path::PathBuf>>,
}

struct PairingRecord {
    digest: [u8; 32],
    expires_at: Instant,
}

impl PairingState {
    pub fn new(tokens: ReviewMapClientTokenStore) -> Self {
        Self {
            tokens,
            codes: Arc::new(Mutex::new(Vec::new())),
            path: None,
        }
    }

    pub fn open(tokens: ReviewMapClientTokenStore, path: impl AsRef<Path>) -> Self {
        Self {
            tokens,
            codes: Arc::new(Mutex::new(Vec::new())),
            path: Some(Arc::new(path.as_ref().to_owned())),
        }
    }

    pub fn issue(&self, lifetime: Duration) -> Result<String, ReviewMapFailure> {
        if let Some(path) = &self.path {
            let code = random_url_token(6);
            let now = unix_seconds();
            let mut records = read_pairing_records(path)?;
            records.retain(|record| record.expires_at > now);
            records.push(PersistedPairingRecord {
                digest: digest(&code),
                expires_at: now.saturating_add(lifetime.as_secs()),
            });
            persist_pairing_records(path, &records)?;
            return Ok(code);
        }
        self.issue_at(Instant::now(), lifetime)
    }

    pub fn issue_at(&self, now: Instant, lifetime: Duration) -> Result<String, ReviewMapFailure> {
        let code = random_url_token(6);
        let mut codes = self.codes.lock().map_err(|_| auth_state_failure())?;
        codes.retain(|record| record.expires_at > now);
        codes.push(PairingRecord {
            digest: digest(&code),
            expires_at: now + lifetime,
        });
        Ok(code)
    }

    pub fn exchange(
        &self,
        code: &str,
        label: impl Into<String>,
    ) -> Result<ClientCredential, ReviewMapFailure> {
        if let Some(path) = &self.path {
            let candidate = digest(code);
            let now = unix_seconds();
            let mut records = read_pairing_records(path)?;
            records.retain(|record| record.expires_at > now);
            let position =
                constant_time_position(records.iter().map(|record| &record.digest), &candidate);
            let Some(position) = position else {
                persist_pairing_records(path, &records)?;
                return Err(pairing_rejected());
            };
            records.swap_remove(position);
            persist_pairing_records(path, &records)?;
            return self.tokens.issue(label);
        }
        self.exchange_at(code, label, Instant::now())
    }

    pub fn exchange_at(
        &self,
        code: &str,
        label: impl Into<String>,
        now: Instant,
    ) -> Result<ClientCredential, ReviewMapFailure> {
        let candidate = digest(code);
        let mut records = self.codes.lock().map_err(|_| auth_state_failure())?;
        records.retain(|record| record.expires_at > now);
        let position =
            constant_time_position(records.iter().map(|record| &record.digest), &candidate);
        let Some(position) = position else {
            return Err(pairing_rejected());
        };
        records.swap_remove(position);
        drop(records);
        self.tokens.issue(label)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PersistedPairingRecord {
    digest: [u8; 32],
    expires_at: u64,
}

impl std::fmt::Debug for PairingState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PairingState")
            .field(
                "active_codes",
                &self.codes.lock().map_or(0, |codes| codes.len()),
            )
            .field("codes", &"[REDACTED]")
            .finish()
    }
}

fn random_url_token(bytes: usize) -> String {
    let mut value = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut value);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value)
}

fn digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn auth_state_failure() -> ReviewMapFailure {
    ReviewMapFailure::new(
        ReviewMapFailureCode::ClientUnauthorized,
        "The paired-client store is unavailable",
    )
}

fn constant_time_position<'a>(
    digests: impl IntoIterator<Item = &'a [u8; 32]>,
    candidate: &[u8; 32],
) -> Option<usize> {
    digests
        .into_iter()
        .enumerate()
        .fold(None, |found, (index, digest)| {
            if bool::from(digest.ct_eq(candidate)) {
                Some(index)
            } else {
                found
            }
        })
}

fn pairing_rejected() -> ReviewMapFailure {
    ReviewMapFailure::new(
        PairingRejected,
        "The pairing code is invalid, expired, or already used",
    )
}

fn read_pairing_records(path: &Path) -> Result<Vec<PersistedPairingRecord>, ReviewMapFailure> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
            ReviewMapFailure::with_source(
                PairingRejected,
                "The pairing-code store is malformed",
                error,
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(token_io("Could not read the pairing-code store", error)),
    }
}

fn persist_pairing_records(
    path: &Path,
    records: &[PersistedPairingRecord],
) -> Result<(), ReviewMapFailure> {
    persist_json(path, records, "pairing-code")
}

fn persist_records(path: &Path, records: &[ClientRecord]) -> Result<(), ReviewMapFailure> {
    persist_json(path, records, "paired-client")
}

fn persist_json<T: serde::Serialize>(
    path: &Path,
    records: &[T],
    kind: &'static str,
) -> Result<(), ReviewMapFailure> {
    let parent = path.parent().ok_or_else(|| {
        ReviewMapFailure::new(
            ReviewMapFailureCode::ClientUnauthorized,
            "The paired-client store path has no parent directory",
        )
    })?;
    std::fs::create_dir_all(parent)
        .map_err(|error| token_io("Could not create the paired-client directory", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| token_io("Could not secure the paired-client directory", error))?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec(records).map_err(|error| {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::ClientUnauthorized,
            format!("Could not encode the {kind} store"),
            error,
        )
    })?;
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|error| token_io("Could not create the paired-client store", error))?;
        file.write_all(&bytes)
            .map_err(|error| token_io("Could not write the paired-client store", error))?;
        file.sync_all()
            .map_err(|error| token_io("Could not sync the paired-client store", error))?;
        std::fs::rename(&temporary, path)
            .map_err(|error| token_io("Could not replace the paired-client store", error))?;
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| token_io("Could not sync the paired-client directory", error))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn token_io(message: &'static str, error: std::io::Error) -> ReviewMapFailure {
    ReviewMapFailure::with_source(ReviewMapFailureCode::ClientUnauthorized, message, error)
}
