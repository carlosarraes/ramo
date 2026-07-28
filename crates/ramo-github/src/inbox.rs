use std::collections::HashMap;

use ramo_core::github::{InboxKind, InboxPage, PullRequestKey, PullRequestSummary};
use reqwest::StatusCode;

use crate::{GithubClient, GithubError};

const SEARCH_DOCUMENT: &str = "query SearchPullRequests($query: String!, $first: Int!, $after: String) { search(query: $query, type: ISSUE, first: $first, after: $after) { nodes { ... on PullRequest { id number title url updatedAt isDraft additions deletions changedFiles author { login } repository { nameWithOwner } } } pageInfo { endCursor hasNextPage } } }";
const TEAM_PERMISSION_WARNING: &str =
    "Team review requests need a token whose resource owner is that organization.";

#[derive(serde::Serialize)]
struct SearchVariables<'a> {
    query: String,
    first: usize,
    after: Option<&'a str>,
}

#[derive(serde::Deserialize)]
struct SearchData {
    search: SearchConnection,
}

#[derive(serde::Deserialize)]
struct SearchConnection {
    #[serde(default)]
    nodes: Vec<Option<SearchPullRequest>>,
    #[serde(rename = "pageInfo")]
    page_info: PageInfo,
}

#[derive(serde::Deserialize)]
struct PageInfo {
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
}

#[derive(serde::Deserialize)]
struct SearchPullRequest {
    id: String,
    number: u64,
    title: String,
    url: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
    #[serde(rename = "isDraft")]
    is_draft: bool,
    additions: usize,
    deletions: usize,
    #[serde(rename = "changedFiles")]
    changed_files: usize,
    author: Option<Login>,
    repository: RepositoryName,
}

#[derive(serde::Deserialize)]
struct Login {
    login: String,
}

#[derive(serde::Deserialize)]
struct RepositoryName {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
}

#[derive(serde::Deserialize)]
struct Team {
    slug: String,
    organization: Login,
}

impl GithubClient {
    pub fn list_inbox(
        &self,
        kind: InboxKind,
        after: Option<&str>,
    ) -> Result<InboxPage, GithubError> {
        let qualifier = match kind {
            InboxKind::ReviewRequests => "user-review-requested:@me",
            InboxKind::Authored => "author:@me",
        };
        let direct = self.search_pull_requests(qualifier, after)?;
        let end_cursor = direct.page_info.end_cursor.clone();
        let mut has_next_page = direct.page_info.has_next_page;
        let mut warnings = Vec::new();
        let mut items = HashMap::new();
        extend_unique(&mut items, direct.nodes);

        if kind == InboxKind::ReviewRequests {
            match self.accessible_teams()? {
                TeamAccess::Teams(teams) => {
                    for team in teams {
                        let qualifier = format!(
                            "team-review-requested:{}/{}",
                            team.organization.login, team.slug
                        );
                        let page = self.search_pull_requests(&qualifier, after)?;
                        has_next_page |= page.page_info.has_next_page;
                        extend_unique(&mut items, page.nodes);
                    }
                }
                TeamAccess::PermissionMissing => warnings.push(TEAM_PERMISSION_WARNING.into()),
            }
        }

        let mut items: Vec<_> = items.into_values().collect();
        items.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.node_id.cmp(&right.node_id))
        });
        Ok(InboxPage {
            items,
            end_cursor,
            has_next_page,
            warnings,
        })
    }

    fn search_pull_requests(
        &self,
        qualifier: &str,
        after: Option<&str>,
    ) -> Result<SearchConnection, GithubError> {
        let variables = SearchVariables {
            query: format!("is:open is:pr {qualifier}"),
            first: 20,
            after,
        };
        self.graphql::<SearchData, _>(SEARCH_DOCUMENT, variables)
            .map(|data| data.search)
    }

    fn accessible_teams(&self) -> Result<TeamAccess, GithubError> {
        let response = self
            .rest_request(
                reqwest::Method::GET,
                "/user/teams?per_page=100",
                "application/vnd.github+json",
            )
            .send()
            .map_err(GithubError::transport)?;
        if response.status() == StatusCode::FORBIDDEN
            && response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|value| value.to_str().ok())
                != Some("0")
        {
            return Ok(TeamAccess::PermissionMissing);
        }
        let response = Self::ensure_success(response)?;
        let teams = response.json().map_err(GithubError::decode)?;
        Ok(TeamAccess::Teams(teams))
    }
}

enum TeamAccess {
    Teams(Vec<Team>),
    PermissionMissing,
}

fn extend_unique(
    items: &mut HashMap<String, PullRequestSummary>,
    nodes: Vec<Option<SearchPullRequest>>,
) {
    for node in nodes.into_iter().flatten() {
        let item = PullRequestSummary {
            node_id: node.id,
            key: PullRequestKey {
                repository: node.repository.name_with_owner,
                number: node.number,
            },
            title: node.title,
            url: node.url,
            author_login: node.author.map_or_else(String::new, |author| author.login),
            updated_at: node.updated_at,
            is_draft: node.is_draft,
            additions: node.additions,
            deletions: node.deletions,
            changed_files: node.changed_files,
        };
        items.entry(item.node_id.clone()).or_insert(item);
    }
}
