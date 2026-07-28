use crate::{GithubClient, GithubError};

const MARK_VIEWED: &str = "mutation MarkFileAsViewed($pullRequestId: ID!, $path: String!) { markFileAsViewed(input: { pullRequestId: $pullRequestId, path: $path }) { clientMutationId } }";
const UNMARK_VIEWED: &str = "mutation UnmarkFileAsViewed($pullRequestId: ID!, $path: String!) { unmarkFileAsViewed(input: { pullRequestId: $pullRequestId, path: $path }) { clientMutationId } }";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewedVariables<'a> {
    pull_request_id: &'a str,
    path: &'a str,
}

impl GithubClient {
    pub fn set_file_viewed(
        &self,
        pull_request_id: &str,
        path: &str,
        viewed: bool,
    ) -> Result<(), GithubError> {
        let query = if viewed { MARK_VIEWED } else { UNMARK_VIEWED };
        let _: serde_json::Value = self.graphql(
            query,
            ViewedVariables {
                pull_request_id,
                path,
            },
        )?;
        Ok(())
    }
}
