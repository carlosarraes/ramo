use std::io::Read;

use crate::core::changeset::Changeset;
use crate::core::input::ReviewInput;
use crate::diff::parser::parse_unified_diff;
use crate::github::GithubPullRequestSource;

use super::{LoadContext, LoadError, LoadedPullRequest, LoadedReview, ReloadPlan};

pub(super) fn load(
    input: &ReviewInput,
    stdin: &mut dyn Read,
    load_context: &LoadContext<'_>,
    service: &mut dyn GithubPullRequestSource,
) -> Result<LoadedPullRequest, LoadError> {
    let ReviewInput::PullRequest {
        number, options, ..
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
    let files = parse_unified_diff(&diff);
    if files.is_empty() {
        return Err(LoadError::InvalidPullRequestDiff { number: *number });
    }
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
    })
}
