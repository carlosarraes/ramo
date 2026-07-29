use std::path::PathBuf;

use ramo_core::review_map::ReviewMapFailureCode;

use crate::ReviewMapFailure;

use super::SetupEnvironment;

pub struct TailscalePublisher {
    executable: PathBuf,
}

impl TailscalePublisher {
    pub fn new(executable: PathBuf) -> Self {
        Self { executable }
    }

    pub fn publish(
        &self,
        environment: &dyn SetupEnvironment,
        arguments: &[String],
    ) -> Result<(), ReviewMapFailure> {
        if arguments.last().map(String::as_str) != Some("http://127.0.0.1:47831") {
            return Err(ReviewMapFailure::new(
                ReviewMapFailureCode::ServerIncompatible,
                "Tailscale Serve must target the loopback ramo-server",
            ));
        }
        let output = environment
            .run(&self.executable, arguments)
            .map_err(|error| {
                ReviewMapFailure::with_source(
                    ReviewMapFailureCode::ServerIncompatible,
                    "Could not configure Tailscale Serve",
                    error,
                )
            })?;
        if output.success {
            Ok(())
        } else {
            Err(ReviewMapFailure::new(
                ReviewMapFailureCode::ServerIncompatible,
                "Tailscale Serve rejected the private endpoint",
            ))
        }
    }
}
