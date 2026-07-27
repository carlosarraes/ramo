use std::io::Read;

use crate::core::changeset::Changeset;
use crate::core::input::ReviewInput;
use crate::diff::model::{DiffFile, FileChangeKind, SourceSpec};
use crate::diff::parser::parse_unified_diff;
use crate::github::GithubPullRequestSource;
use crate::remote_review::PullRequestReviewContext;

use super::{LoadContext, LoadError, LoadedPullRequest, LoadedReview, ReloadPlan};

pub(super) fn load(
    input: &ReviewInput,
    stdin: &mut dyn Read,
    load_context: &LoadContext<'_>,
    service: &mut dyn GithubPullRequestSource,
) -> Result<LoadedPullRequest, LoadError> {
    let ReviewInput::PullRequest {
        number,
        with_comments,
        options,
    } = input
    else {
        return Err(LoadError::UnsupportedInput(input.kind()));
    };
    let (agent_context, agent_source) = crate::notes::context::resolve_agent_context(
        options.agent_context.as_deref(),
        load_context.cwd,
        stdin,
        false,
    )?;
    let context = service.resolve_pr(*number)?;
    let diff = service.load_diff(*number)?;
    if diff.trim().is_empty() {
        return Err(LoadError::EmptyPullRequestDiff { number: *number });
    }
    let mut files = parse_unified_diff(&diff);
    if files.is_empty() {
        return Err(LoadError::InvalidPullRequestDiff { number: *number });
    }
    attach_remote_sources(&mut files, &context);
    let imported_threads = if *with_comments {
        service.load_review_threads(&context)?
    } else {
        Vec::new()
    };
    let mut changeset =
        Changeset::new(format!("GitHub PR #{number}"), context.title.clone(), files);
    if let Some(agent_context) = &agent_context {
        changeset.apply_agent_context(agent_context);
    }
    Ok(LoadedPullRequest {
        review: LoadedReview {
            changeset,
            reload_plan: ReloadPlan::None,
            agent_context: agent_source,
        },
        context,
        imported_threads,
    })
}

fn attach_remote_sources(files: &mut [DiffFile], context: &PullRequestReviewContext) {
    for file in files.iter_mut().filter(|file| !file.is_binary) {
        let old_path = file.previous_path.as_deref().unwrap_or(&file.path);
        file.old_source = if file.change_kind == FileChangeKind::Added {
            SourceSpec::None
        } else {
            SourceSpec::RemoteBlob {
                repository: context.repository.clone(),
                revision: context.base_revision.clone(),
                path: old_path.to_owned(),
            }
        };
        file.new_source = if file.change_kind == FileChangeKind::Deleted {
            SourceSpec::None
        } else {
            SourceSpec::RemoteBlob {
                repository: context.repository.clone(),
                revision: context.captured_revision.clone(),
                path: file.path.clone(),
            }
        };
    }
}
