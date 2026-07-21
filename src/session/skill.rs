use std::fs;
use std::io;
use std::path::PathBuf;

const REVIEW_SKILL: &str = include_str!("ramo-review-SKILL.md");

pub fn review_skill_path() -> io::Result<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::data_dir)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "platform data directory is unavailable",
            )
        })?;
    Ok(base.join("ramo/skills/ramo-review/SKILL.md"))
}

pub fn materialize_review_skill() -> io::Result<PathBuf> {
    let path = review_skill_path()?;
    let parent = path.parent().expect("skill path has a parent");
    fs::create_dir_all(parent)?;
    if fs::read_to_string(&path).ok().as_deref() != Some(REVIEW_SKILL) {
        let temporary = parent.join(format!(".SKILL.md.{}.tmp", std::process::id()));
        fs::write(&temporary, REVIEW_SKILL)?;
        fs::rename(&temporary, &path)?;
    }
    Ok(path)
}
