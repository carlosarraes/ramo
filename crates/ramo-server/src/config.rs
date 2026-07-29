use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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

        Ok(Self {
            bind_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT),
            config_dir: config_root.join("ramo-server"),
            state_dir: state_root.join("ramo-server"),
            cache_dir: cache_root.join("ramo-server/review-maps"),
            ollama_url: "http://127.0.0.1:11434".into(),
            model: "qwen3:8b".into(),
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
