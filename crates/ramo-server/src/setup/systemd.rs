use std::path::{Path, PathBuf};

use ramo_core::review_map::ReviewMapFailureCode;

use crate::ReviewMapFailure;

use super::{SetupEnvironment, write_private};

pub struct SystemdUserService {
    systemctl: PathBuf,
    unit_path: PathBuf,
}

impl SystemdUserService {
    pub fn new(systemctl: PathBuf, unit_path: PathBuf) -> Self {
        Self {
            systemctl,
            unit_path,
        }
    }

    pub fn install(
        &self,
        environment: &dyn SetupEnvironment,
        unit: &str,
    ) -> Result<(), ReviewMapFailure> {
        write_private(&self.unit_path, unit.as_bytes())?;
        self.run(environment, &["--user", "daemon-reload"])?;
        self.run(
            environment,
            &["--user", "enable", "--now", "ramo-server.service"],
        )
    }

    pub fn remove(&self, environment: &dyn SetupEnvironment) -> Result<(), ReviewMapFailure> {
        let _ = self.run(
            environment,
            &["--user", "disable", "--now", "ramo-server.service"],
        );
        match std::fs::remove_file(&self.unit_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ReviewMapFailure::with_source(
                    ReviewMapFailureCode::ServerIncompatible,
                    "Could not remove the ramo-server user service",
                    error,
                ));
            }
        }
        self.run(environment, &["--user", "daemon-reload"])
    }

    pub fn restore(
        &self,
        environment: &dyn SetupEnvironment,
        previous_unit: Option<&[u8]>,
    ) -> Result<(), ReviewMapFailure> {
        if let Some(previous_unit) = previous_unit {
            write_private(&self.unit_path, previous_unit)?;
            self.run(environment, &["--user", "daemon-reload"])?;
            self.run(environment, &["--user", "restart", "ramo-server.service"])
        } else {
            self.remove(environment)
        }
    }

    fn run(
        &self,
        environment: &dyn SetupEnvironment,
        arguments: &[&str],
    ) -> Result<(), ReviewMapFailure> {
        let arguments = arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect::<Vec<_>>();
        let output = environment
            .run(&self.systemctl, &arguments)
            .map_err(|error| {
                ReviewMapFailure::with_source(
                    ReviewMapFailureCode::ServerIncompatible,
                    "Could not run the systemd user service manager",
                    error,
                )
            })?;
        if output.success {
            Ok(())
        } else {
            Err(ReviewMapFailure::new(
                ReviewMapFailureCode::ServerIncompatible,
                "systemd could not configure the ramo-server user service",
            ))
        }
    }
}

pub fn unit_contents(executable: &Path, bind_address: &str) -> Result<String, ReviewMapFailure> {
    let executable = executable.to_string_lossy();
    if executable.contains(['\n', '\r', '"']) || bind_address != "127.0.0.1:47831" {
        return Err(ReviewMapFailure::new(
            ReviewMapFailureCode::ServerIncompatible,
            "Unsafe ramo-server service configuration",
        ));
    }
    Ok(format!(
        "[Unit]\nDescription=Ramo private local Review Map server\nAfter=network-online.target tailscaled.service\n\n[Service]\nType=simple\nExecStart={executable} serve\nRestart=on-failure\nRestartSec=3\nUMask=0077\nNoNewPrivileges=true\nPrivateTmp=true\n\n[Install]\nWantedBy=default.target\n"
    ))
}
