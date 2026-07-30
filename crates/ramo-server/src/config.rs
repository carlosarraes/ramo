use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::path::PathBuf;

use ramo_core::review_map::ReviewMapFailureCode;

use crate::ReviewMapFailure;

pub const DEFAULT_PORT: u16 = 47_831;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub bind_address: SocketAddr,
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub ollama_url: String,
    pub model: String,
    pub selected_model: Option<SelectedModelConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SelectedModelConfig {
    pub selected_model: String,
    pub model_digest: String,
    pub prompt_version: u32,
    pub benchmark_run_id: String,
}

impl ServerConfig {
    pub fn discover() -> Result<Self, ReviewMapFailure> {
        let config_root = dirs::config_dir().ok_or_else(|| {
            ReviewMapFailure::new(
                ReviewMapFailureCode::CacheUnavailable,
                "Could not resolve the user configuration directory",
            )
        })?;
        let state_root = dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .ok_or_else(|| {
                ReviewMapFailure::new(
                    ReviewMapFailureCode::CacheUnavailable,
                    "Could not resolve the user state directory",
                )
            })?;
        let cache_root = dirs::cache_dir().ok_or_else(|| {
            ReviewMapFailure::new(
                ReviewMapFailureCode::CacheUnavailable,
                "Could not resolve the user cache directory",
            )
        })?;

        let config_dir = config_root.join("ramo-server");
        let selected_model =
            load_selected_model_for_prompt(&config_dir, crate::ollama::PROMPT_VERSION)?;
        let model = selected_model.as_ref().map_or_else(
            || "qwen3:8b".into(),
            |selected| selected.selected_model.clone(),
        );
        Ok(Self {
            bind_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT),
            config_dir,
            state_dir: state_root.join("ramo-server"),
            cache_dir: cache_root.join("ramo-server/review-maps"),
            ollama_url: "http://127.0.0.1:11434".into(),
            model,
            selected_model,
        })
    }

    pub fn validate(&self) -> Result<(), ReviewMapFailure> {
        if !self.bind_address.ip().is_loopback() {
            return Err(ReviewMapFailure::new(
                ReviewMapFailureCode::ServerIncompatible,
                "The local review server must bind to a loopback address",
            ));
        }
        Ok(())
    }
}

pub fn load_selected_model(
    config_dir: &Path,
) -> Result<Option<SelectedModelConfig>, ReviewMapFailure> {
    let path = config_dir.join("selected-model.json");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ReviewMapFailure::with_source(
                ReviewMapFailureCode::CacheUnavailable,
                "Could not read the selected benchmark model",
                error,
            ));
        }
    };
    let selected = serde_json::from_slice::<SelectedModelConfig>(&bytes).map_err(|error| {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::CacheUnavailable,
            "Could not parse the selected benchmark model",
            error,
        )
    })?;
    if selected.selected_model.trim().is_empty()
        || selected.model_digest.trim().is_empty()
        || selected.benchmark_run_id.trim().is_empty()
    {
        return Err(ReviewMapFailure::new(
            ReviewMapFailureCode::CacheUnavailable,
            "Selected benchmark model configuration is incomplete",
        ));
    }
    Ok(Some(selected))
}

pub fn load_selected_model_for_prompt(
    config_dir: &Path,
    prompt_version: u32,
) -> Result<Option<SelectedModelConfig>, ReviewMapFailure> {
    Ok(load_selected_model(config_dir)?
        .filter(|selected| selected.prompt_version == prompt_version))
}

pub fn save_selected_model(
    config_dir: &Path,
    selected: &SelectedModelConfig,
) -> Result<(), ReviewMapFailure> {
    let bytes = serde_json::to_vec_pretty(selected).map_err(|error| {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::CacheUnavailable,
            "Could not serialize the selected benchmark model",
            error,
        )
    })?;
    std::fs::create_dir_all(config_dir).map_err(|error| {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::CacheUnavailable,
            "Could not create the server configuration directory",
            error,
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(config_dir, std::fs::Permissions::from_mode(0o700)).map_err(
            |error| {
                ReviewMapFailure::with_source(
                    ReviewMapFailureCode::CacheUnavailable,
                    "Could not protect the server configuration directory",
                    error,
                )
            },
        )?;
    }
    let path = config_dir.join("selected-model.json");
    let temporary = config_dir.join("selected-model.json.tmp");
    let result = (|| {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        #[cfg(windows)]
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        std::fs::rename(&temporary, &path)?;
        #[cfg(unix)]
        std::fs::File::open(config_dir)?.sync_all()?;
        Ok::<(), std::io::Error>(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(ReviewMapFailure::with_source(
            ReviewMapFailureCode::CacheUnavailable,
            "Could not atomically save the selected benchmark model",
            error,
        ));
    }
    Ok(())
}
