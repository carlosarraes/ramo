use std::fs;
use std::io;
use std::path::PathBuf;

const EXTENSION_SOURCE: &str = include_str!("pi_extension_src.ts");

fn resolve_dir(target: &str) -> io::Result<PathBuf> {
    if target != "pi" {
        eprintln!("Unknown target: {target}. Supported: pi");
        std::process::exit(1);
    }

    dirs::home_dir()
        .map(|h| h.join(".pi").join("agent").join("extensions").join("pdiff"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Could not determine home directory",
            )
        })
}

pub fn install(target: &str) -> io::Result<()> {
    let dir = resolve_dir(target)?;
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("index.ts"), EXTENSION_SOURCE)?;
    eprintln!("Installed pdiff extension to {}", dir.display());
    eprintln!("Restart pi to activate. Or run: pi -e {}", dir.display());
    Ok(())
}

pub fn uninstall(target: &str) -> io::Result<()> {
    let dir = resolve_dir(target)?;
    if !dir.exists() {
        eprintln!("pdiff extension not found at {}", dir.display());
        std::process::exit(0);
    }
    fs::remove_dir_all(&dir)?;
    eprintln!("Uninstalled pdiff extension from {}", dir.display());
    Ok(())
}
