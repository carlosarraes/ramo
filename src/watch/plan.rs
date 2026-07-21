use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::core::input::VcsId;
use crate::input::ReloadPlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    Hybrid,
    PollOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchTarget {
    Entries {
        directory: PathBuf,
        entries: Vec<PathBuf>,
    },
    Tree {
        directory: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchPlan {
    pub coverage: Coverage,
    pub targets: Vec<WatchTarget>,
}

impl WatchPlan {
    pub fn from_reload_plan(plan: &ReloadPlan, cwd: &Path) -> Option<Self> {
        match plan {
            ReloadPlan::None => None,
            ReloadPlan::Files { left, right, .. } => {
                Some(Self::for_files([left.as_path(), right.as_path()], cwd))
            }
            ReloadPlan::PatchFile { path } => Some(Self::for_files([path.as_path()], cwd)),
            ReloadPlan::Vcs { repo_root, vcs, .. } => Some(Self::for_vcs(repo_root.clone(), *vcs)),
        }
    }

    pub fn for_vcs(repo_root: PathBuf, vcs: VcsId) -> Self {
        match vcs {
            VcsId::Git => Self {
                coverage: Coverage::Hybrid,
                targets: vec![WatchTarget::Tree {
                    directory: repo_root,
                }],
            },
            VcsId::Jj | VcsId::Sl => Self {
                coverage: Coverage::PollOnly,
                targets: Vec::new(),
            },
        }
    }

    fn for_files<'a>(paths: impl IntoIterator<Item = &'a Path>, cwd: &Path) -> Self {
        let mut groups = BTreeMap::<PathBuf, Vec<PathBuf>>::new();
        for path in paths {
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            };
            let directory = absolute
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| cwd.to_path_buf());
            groups.entry(directory).or_default().push(absolute);
        }
        let targets = groups
            .into_iter()
            .map(|(directory, mut entries)| {
                entries.sort();
                entries.dedup();
                WatchTarget::Entries { directory, entries }
            })
            .collect();
        Self {
            coverage: Coverage::Hybrid,
            targets,
        }
    }
}
