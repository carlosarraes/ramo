use std::fs;
use std::path::{Path, PathBuf};

use crate::core::input::VcsId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcsDetection {
    pub id: VcsId,
    pub repo_root: PathBuf,
}

pub fn select_vcs(cwd: &Path, configured: Option<VcsId>) -> Option<VcsDetection> {
    let start = absolute(cwd);
    if let Some(id) = configured {
        return Some(VcsDetection {
            id,
            repo_root: detect_root(&start, id).unwrap_or(start),
        });
    }

    for ancestor in start.ancestors() {
        for id in [VcsId::Jj, VcsId::Sl, VcsId::Git] {
            if has_marker(ancestor, id) {
                return Some(VcsDetection {
                    id,
                    repo_root: ancestor.to_path_buf(),
                });
            }
        }
    }
    None
}

pub(crate) fn detect_root(cwd: &Path, id: VcsId) -> Option<PathBuf> {
    absolute(cwd)
        .ancestors()
        .find(|ancestor| has_marker(ancestor, id))
        .map(Path::to_path_buf)
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn has_marker(root: &Path, id: VcsId) -> bool {
    match id {
        VcsId::Git => root.join(".git").exists(),
        VcsId::Jj => root.join(".jj").exists(),
        VcsId::Sl => root.join(".sl").exists() || is_sapling_hg(root),
    }
}

fn is_sapling_hg(root: &Path) -> bool {
    fs::read_to_string(root.join(".hg/requires"))
        .map(|requirements| requirements.lines().any(|line| line == "treestate"))
        .unwrap_or(false)
}
