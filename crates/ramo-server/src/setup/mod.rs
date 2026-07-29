mod systemd;
mod tailscale;

use std::path::{Path, PathBuf};

use ramo_core::review_map::ReviewMapFailureCode;

use crate::ReviewMapFailure;

pub use systemd::SystemdUserService;
pub use tailscale::TailscalePublisher;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
}

pub trait SetupEnvironment: Send + Sync {
    fn operating_system(&self) -> &'static str;
    fn resolve_executable(&self, name: &str) -> Option<PathBuf>;
    fn run(&self, program: &Path, arguments: &[String]) -> Result<CommandOutput, std::io::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyReport {
    pub gh: PathBuf,
    pub ollama: PathBuf,
    pub tailscale: PathBuf,
    pub systemctl: PathBuf,
    pub magic_dns_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupPaths {
    pub server_executable: PathBuf,
    pub unit_path: PathBuf,
    pub endpoint_path: PathBuf,
}

impl SetupPaths {
    pub fn discover(server_executable: PathBuf) -> Result<Self, ReviewMapFailure> {
        let config = dirs::config_dir().ok_or_else(|| {
            ReviewMapFailure::new(
                ReviewMapFailureCode::ServerIncompatible,
                "Could not resolve the user configuration directory",
            )
        })?;
        Ok(Self {
            server_executable,
            unit_path: config.join("systemd/user/ramo-server.service"),
            endpoint_path: config.join("ramo-server/endpoint"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupPlan {
    pub server_executable: PathBuf,
    pub bind_address: String,
    pub systemd_unit: String,
    pub public_endpoint: String,
    pub tailscale_arguments: Vec<String>,
    pub dependencies: DependencyReport,
    pub unit_path: PathBuf,
    pub endpoint_path: PathBuf,
}

impl SetupPlan {
    pub fn render(&self) -> String {
        format!(
            "Ramo server setup plan\nServer: {}\nBind: {}\nUnit: {}\nEndpoint: {}\nTailscale: {} {}\n",
            self.server_executable.display(),
            self.bind_address,
            self.unit_path.display(),
            self.public_endpoint,
            self.dependencies.tailscale.display(),
            self.tailscale_arguments.join(" ")
        )
    }
}

pub fn build_setup_plan(
    environment: &dyn SetupEnvironment,
    paths: SetupPaths,
) -> Result<SetupPlan, ReviewMapFailure> {
    if environment.operating_system() != "linux" {
        return Err(ReviewMapFailure::new(
            ReviewMapFailureCode::ServerIncompatible,
            "Automatic ramo-server setup currently supports Linux only",
        ));
    }
    if !paths.server_executable.is_absolute() {
        return Err(ReviewMapFailure::new(
            ReviewMapFailureCode::ServerIncompatible,
            "ramo-server setup requires an absolute executable path",
        ));
    }
    let gh = resolve(environment, "gh")?;
    let ollama = resolve(environment, "ollama")?;
    let tailscale = resolve(environment, "tailscale")?;
    let systemctl = resolve(environment, "systemctl")?;

    run_checked(environment, &gh, &["auth", "status"])?;
    run_checked(environment, &ollama, &["list"])?;
    run_checked(environment, &systemctl, &["--user", "show-environment"])?;
    let tailscale_status = run_checked(environment, &tailscale, &["status", "--json"])?;
    let magic_dns_name = magic_dns_name(&tailscale_status.stdout)?;
    let bind_address = "127.0.0.1:47831".to_owned();
    let public_endpoint = format!("https://{magic_dns_name}");
    let tailscale_arguments = [
        "serve",
        "--bg",
        "--yes",
        "--https=443",
        "http://127.0.0.1:47831",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let systemd_unit = systemd::unit_contents(&paths.server_executable, &bind_address)?;
    Ok(SetupPlan {
        server_executable: paths.server_executable,
        bind_address,
        systemd_unit,
        public_endpoint,
        tailscale_arguments,
        dependencies: DependencyReport {
            gh,
            ollama,
            tailscale,
            systemctl,
            magic_dns_name,
        },
        unit_path: paths.unit_path,
        endpoint_path: paths.endpoint_path,
    })
}

pub fn apply_setup(
    environment: &dyn SetupEnvironment,
    plan: &SetupPlan,
    dry_run: bool,
) -> Result<String, ReviewMapFailure> {
    if dry_run {
        return Ok(plan.render());
    }
    let previous_unit = std::fs::read(&plan.unit_path).ok();
    let previous_endpoint = std::fs::read(&plan.endpoint_path).ok();
    write_private(&plan.endpoint_path, self_endpoint_bytes(plan))?;
    let service =
        SystemdUserService::new(plan.dependencies.systemctl.clone(), plan.unit_path.clone());
    if let Err(error) = service.install(environment, &plan.systemd_unit) {
        let _ = service.restore(environment, previous_unit.as_deref());
        let _ = restore_file(&plan.endpoint_path, previous_endpoint.as_deref());
        return Err(error);
    }
    let publisher = TailscalePublisher::new(plan.dependencies.tailscale.clone());
    if let Err(error) = publisher.publish(environment, &plan.tailscale_arguments) {
        let _ = service.restore(environment, previous_unit.as_deref());
        let _ = restore_file(&plan.endpoint_path, previous_endpoint.as_deref());
        return Err(error);
    }
    Ok(format!(
        "ramo-server is running privately at {}",
        plan.public_endpoint
    ))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemEnvironment;

impl SetupEnvironment for SystemEnvironment {
    fn operating_system(&self) -> &'static str {
        std::env::consts::OS
    }

    fn resolve_executable(&self, name: &str) -> Option<PathBuf> {
        std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .map(|directory| directory.join(name))
                .find(|candidate| candidate.is_file())
        })
    }

    fn run(&self, program: &Path, arguments: &[String]) -> Result<CommandOutput, std::io::Error> {
        let output = std::process::Command::new(program)
            .args(arguments)
            .output()?;
        Ok(CommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        })
    }
}

pub fn current_plan() -> Result<SetupPlan, ReviewMapFailure> {
    let executable = std::env::current_exe().map_err(|error| {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::ServerIncompatible,
            "Could not resolve the ramo-server executable",
            error,
        )
    })?;
    build_setup_plan(&SystemEnvironment, SetupPaths::discover(executable)?)
}

pub fn current_status() -> Result<String, ReviewMapFailure> {
    let plan = current_plan()?;
    let service = run_checked(
        &SystemEnvironment,
        &plan.dependencies.systemctl,
        &["--user", "is-active", "ramo-server.service"],
    )?;
    let tailscale = run_checked(
        &SystemEnvironment,
        &plan.dependencies.tailscale,
        &["serve", "status", "--json"],
    )?;
    Ok(format!(
        "Service: {}\nEndpoint: {}\nTailscale Serve: {}",
        service.stdout.trim(),
        plan.public_endpoint,
        if tailscale_serve_active(&tailscale.stdout) {
            "active"
        } else {
            "not configured"
        }
    ))
}

pub fn tailscale_serve_active(status_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(status_json)
        .ok()
        .and_then(|value| value.as_object().map(|object| !object.is_empty()))
        .unwrap_or(false)
}

pub fn issue_pairing_code() -> Result<String, ReviewMapFailure> {
    let config = crate::config::ServerConfig::discover()?;
    let executable = std::env::current_exe().map_err(|error| {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::ServerIncompatible,
            "Could not resolve the ramo-server executable",
            error,
        )
    })?;
    let paths = SetupPaths::discover(executable)?;
    let endpoint = std::fs::read_to_string(&paths.endpoint_path)
        .map_err(setup_io)?
        .trim()
        .to_owned();
    let pairing = crate::api::PairingState::open(
        crate::api::ReviewMapClientTokenStore::default(),
        config.state_dir.join("pairing.json"),
    );
    let code = pairing.issue(std::time::Duration::from_secs(300))?;
    let mut uri = reqwest::Url::parse("ramo://pair").map_err(|error| {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::ServerIncompatible,
            "Could not build the Ramo pairing link",
            error,
        )
    })?;
    uri.query_pairs_mut()
        .append_pair("endpoint", &endpoint)
        .append_pair("code", &code);
    let qr = qrcode::QrCode::new(uri.as_str().as_bytes())
        .map_err(|error| {
            ReviewMapFailure::with_source(
                ReviewMapFailureCode::ServerIncompatible,
                "Could not render the Ramo pairing QR code",
                error,
            )
        })?
        .render::<qrcode::render::unicode::Dense1x2>()
        .quiet_zone(true)
        .build();
    Ok(format!(
        "{qr}\nOpen on Android: {uri}\nEndpoint: {endpoint}\nCode: {code}\nExpires in 5 minutes."
    ))
}

fn resolve(
    environment: &dyn SetupEnvironment,
    name: &'static str,
) -> Result<PathBuf, ReviewMapFailure> {
    environment.resolve_executable(name).ok_or_else(|| {
        ReviewMapFailure::new(
            ReviewMapFailureCode::ServerIncompatible,
            format!("Required dependency '{name}' was not found in PATH"),
        )
    })
}

fn run_checked(
    environment: &dyn SetupEnvironment,
    program: &Path,
    arguments: &[&str],
) -> Result<CommandOutput, ReviewMapFailure> {
    let arguments = arguments
        .iter()
        .map(|argument| (*argument).to_owned())
        .collect::<Vec<_>>();
    let output = environment.run(program, &arguments).map_err(|error| {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::ServerIncompatible,
            format!("Could not run dependency '{}'", program.display()),
            error,
        )
    })?;
    if !output.success {
        return Err(ReviewMapFailure::new(
            ReviewMapFailureCode::ServerIncompatible,
            format!(
                "Dependency check failed for '{}'",
                program.file_name().unwrap_or_default().to_string_lossy()
            ),
        ));
    }
    Ok(output)
}

fn magic_dns_name(status: &str) -> Result<String, ReviewMapFailure> {
    let value = serde_json::from_str::<serde_json::Value>(status).map_err(|error| {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::ServerIncompatible,
            "Tailscale returned malformed status JSON",
            error,
        )
    })?;
    value["Self"]["DNSName"]
        .as_str()
        .map(|name| name.trim_end_matches('.').to_owned())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            ReviewMapFailure::new(
                ReviewMapFailureCode::ServerIncompatible,
                "Tailscale has no active MagicDNS name",
            )
        })
}

pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<(), ReviewMapFailure> {
    let parent = path.parent().ok_or_else(|| {
        ReviewMapFailure::new(
            ReviewMapFailureCode::ServerIncompatible,
            "A setup path has no parent directory",
        )
    })?;
    std::fs::create_dir_all(parent).map_err(setup_io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .map_err(setup_io)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true).mode(0o600);
        use std::io::Write;
        let mut file = options.open(path).map_err(setup_io)?;
        file.write_all(bytes).map_err(setup_io)?;
        file.sync_all().map_err(setup_io)?;
    }
    #[cfg(not(unix))]
    std::fs::write(path, bytes).map_err(setup_io)?;
    Ok(())
}

fn self_endpoint_bytes(plan: &SetupPlan) -> &[u8] {
    plan.public_endpoint.as_bytes()
}

fn restore_file(path: &Path, previous: Option<&[u8]>) -> Result<(), ReviewMapFailure> {
    if let Some(previous) = previous {
        write_private(path, previous)
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(setup_io(error)),
        }
    }
}

fn setup_io(error: std::io::Error) -> ReviewMapFailure {
    ReviewMapFailure::with_source(
        ReviewMapFailureCode::ServerIncompatible,
        "Could not write ramo-server setup files",
        error,
    )
}
