use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const PROMPT_SOURCE: &str = include_str!("pi_prompt.md");

pub fn install(target: &str) -> io::Result<()> {
    validate_target(target)?;
    let home = dirs::home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not determine home directory",
        )
    })?;
    let path = install_at(&home)?;
    eprintln!("Installed native pdiff prompt to {}", path.display());
    eprintln!("Restart Pi to activate /pdiff.");
    Ok(())
}

pub fn uninstall(target: &str) -> io::Result<()> {
    validate_target(target)?;
    let home = dirs::home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not determine home directory",
        )
    })?;
    let path = prompt_path(&home);
    uninstall_at(&home)?;
    eprintln!("Removed native pdiff prompt from {}", path.display());
    Ok(())
}

pub fn install_at(home: &Path) -> io::Result<PathBuf> {
    let path = prompt_path(home);
    let directory = path
        .parent()
        .expect("the prompt path always has a parent directory");
    fs::create_dir_all(directory)?;
    let temporary = directory.join(format!(".pdiff.md.{}.tmp", std::process::id()));
    fs::write(&temporary, PROMPT_SOURCE)?;
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(&path)?;
    }
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(path)
}

pub fn uninstall_at(home: &Path) -> io::Result<()> {
    let path = prompt_path(home);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    if let Some(directory) = path.parent()
        && directory.exists()
        && fs::read_dir(directory)?.next().is_none()
    {
        fs::remove_dir(directory)?;
    }
    Ok(())
}

fn prompt_path(home: &Path) -> PathBuf {
    home.join(".pi/agent/prompts/pdiff.md")
}

fn validate_target(target: &str) -> io::Result<()> {
    if target == "pi" {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown integration target {target:?}; supported target: pi"),
        ))
    }
}
