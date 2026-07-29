use sha2::{Digest, Sha256};

/// Every input whose meaning can change a cached AI Review Map.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewMapCacheIdentity {
    pub repository: String,
    pub pull_request: u64,
    pub head_sha: String,
    pub model: String,
    pub model_digest: String,
    pub prompt_version: u32,
    pub schema_version: u16,
    pub classifier_version: u32,
    pub generation_parameters: Vec<(String, String)>,
}

/// Returns a stable SHA-256 key for one semantic Review Map generation input.
pub fn review_map_cache_key(identity: &ReviewMapCacheIdentity) -> String {
    let mut hasher = Sha256::new();

    hash_field(&mut hasher, identity.repository.as_bytes());
    hash_field(&mut hasher, &identity.pull_request.to_be_bytes());
    hash_field(&mut hasher, identity.head_sha.as_bytes());
    hash_field(&mut hasher, identity.model.as_bytes());
    hash_field(&mut hasher, identity.model_digest.as_bytes());
    hash_field(&mut hasher, &identity.prompt_version.to_be_bytes());
    hash_field(&mut hasher, &identity.schema_version.to_be_bytes());
    hash_field(&mut hasher, &identity.classifier_version.to_be_bytes());

    let mut parameters = identity.generation_parameters.clone();
    parameters.sort_unstable();
    hash_field(&mut hasher, &(parameters.len() as u64).to_be_bytes());
    for (name, value) in parameters {
        hash_field(&mut hasher, name.as_bytes());
        hash_field(&mut hasher, value.as_bytes());
    }

    format!("{:x}", hasher.finalize())
}

fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}
