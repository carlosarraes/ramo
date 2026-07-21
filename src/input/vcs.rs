use crate::core::changeset::Changeset;
use crate::core::input::{ReviewInput, VcsId};
use crate::diff::model::{FileChangeKind, SourceSpec};
use crate::diff::parser::parse_unified_diff;
use crate::vcs::detect::{VcsDetection, select_vcs};
use crate::vcs::git::GitAdapter;
use crate::vcs::jj::JjAdapter;
use crate::vcs::sl::SaplingAdapter;
use crate::vcs::{SourceEndpoint, SourceEndpoints, VcsAdapter, VcsLoadContext};

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
        VcsId::Jj => JjAdapter.load(input, &vcs_context)?,
        VcsId::Sl => SaplingAdapter.load(input, &vcs_context)?,
    };
    let mut files = if patch.vcs == VcsId::Git {
        crate::vcs::git::parse_git_patch(&patch.patch_text)
    } else {
        let normalized = super::patch::normalize_patch_text(&patch.patch_text);
        parse_unified_diff(&normalized)
    };
    files.extend(patch.extra_files);
    if let Some(endpoints) = &patch.source_endpoints {
        apply_source_endpoints(&mut files, endpoints);
    }
    Ok(LoadedReview {
        changeset: Changeset::new(patch.source_label, patch.title, files),
        reload_plan: ReloadPlan::Vcs {
            input: input.clone(),
            repo_root: patch.repo_root,
            vcs: selected.id,
        },
        agent_context: crate::notes::AgentContextSource::None,
    })
}

fn apply_source_endpoints(files: &mut [crate::diff::model::DiffFile], endpoints: &SourceEndpoints) {
    for file in files.iter_mut().filter(|file| !file.is_untracked) {
        let old_path = file.previous_path.as_deref().unwrap_or(&file.path);
        file.old_source = if file.change_kind == FileChangeKind::Added {
            SourceSpec::None
        } else {
            source_spec(&endpoints.old, old_path)
        };
        file.new_source = if file.change_kind == FileChangeKind::Deleted {
            SourceSpec::None
        } else {
            source_spec(&endpoints.new, &file.path)
        };
    }
}

fn source_spec(endpoint: &SourceEndpoint, path: &str) -> SourceSpec {
    match endpoint {
        SourceEndpoint::None => SourceSpec::None,
        SourceEndpoint::Worktree { repo_root } => SourceSpec::File(repo_root.join(path)),
        SourceEndpoint::GitBlob {
            repo_root,
            reference,
        } => SourceSpec::GitBlob {
            repo_root: repo_root.clone(),
            reference: reference.clone(),
            path: path.into(),
        },
        SourceEndpoint::GitIndex { repo_root } => SourceSpec::GitIndex {
            repo_root: repo_root.clone(),
            path: path.into(),
        },
    }
}
