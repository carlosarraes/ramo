use crate::core::changeset::Changeset;
use crate::core::input::{ReviewInput, VcsId};
use crate::diff::parser::parse_unified_diff;
use crate::vcs::detect::{VcsDetection, select_vcs};
use crate::vcs::git::GitAdapter;
use crate::vcs::{VcsAdapter, VcsError, VcsLoadContext};

use super::{LoadContext, LoadError, LoadedReview, ReloadPlan};

pub(super) fn load(
    input: &ReviewInput,
    context: &LoadContext<'_>,
) -> Result<LoadedReview, LoadError> {
    let selected = select_vcs(context.cwd, context.config.vcs).unwrap_or_else(|| VcsDetection {
        id: VcsId::Git,
        repo_root: context.cwd.to_path_buf(),
    });
    let vcs_context = VcsLoadContext {
        cwd: context.cwd,
        config: context.config,
        runner: context.runner,
        git_executable: "git",
        jj_executable: "jj",
        sl_executable: "sl",
    };
    let patch = match selected.id {
        VcsId::Git => GitAdapter.load(input, &vcs_context)?,
        vcs => {
            return Err(VcsError::UnsupportedOperation {
                vcs,
                operation: input.kind(),
            }
            .into());
        }
    };
    let normalized = super::patch::normalize_patch_text(&patch.patch_text);
    let mut files = parse_unified_diff(&normalized);
    files.extend(patch.extra_files);
    Ok(LoadedReview {
        changeset: Changeset::new(patch.source_label, patch.title, files),
        reload_plan: ReloadPlan::Vcs {
            input: input.clone(),
            repo_root: patch.repo_root,
        },
    })
}
