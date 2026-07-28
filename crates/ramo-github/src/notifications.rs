use std::collections::HashSet;

use ramo_core::github::{
    ConditionalCursor, PullRequestKey, ReviewNotification, ReviewNotificationPage,
};
use reqwest::StatusCode;
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};

use crate::{GithubClient, GithubError, GithubErrorKind};

const REST_ACCEPT: &str = "application/vnd.github+json";

impl GithubClient {
    pub fn review_notifications(
        &self,
        cursor: &ConditionalCursor,
    ) -> Result<ReviewNotificationPage, GithubError> {
        let mut request = self
            .rest_request(reqwest::Method::GET, "/notifications", REST_ACCEPT)
            .query(&[
                ("all", "false"),
                ("participating", "false"),
                ("per_page", "50"),
            ]);
        if let Some(etag) = &cursor.etag {
            request = request.header(IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = &cursor.last_modified {
            request = request.header(IF_MODIFIED_SINCE, last_modified);
        }
        let response = request.send().map_err(GithubError::transport)?;
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(ReviewNotificationPage {
                notifications: Vec::new(),
                cursor: cursor.clone(),
                not_modified: true,
            });
        }
        let next_cursor = ConditionalCursor {
            etag: header_string(response.headers(), ETAG),
            last_modified: header_string(response.headers(), LAST_MODIFIED),
        };
        let response = Self::ensure_success(response)?;
        let raw: Vec<RawNotification> = response.json().map_err(GithubError::decode)?;
        let mut seen = HashSet::new();
        let mut notifications = Vec::new();
        for notification in raw {
            if notification.reason != "review_requested"
                || notification.subject.kind != "PullRequest"
                || !seen.insert(notification.id.clone())
            {
                continue;
            }
            let Some(url) = notification.subject.url else {
                continue;
            };
            let path = url.strip_prefix(&self.rest_base).ok_or_else(|| {
                GithubError::new(
                    GithubErrorKind::Validation,
                    "GitHub notification subject used an unexpected API host",
                )
            })?;
            let pull: PullNumber =
                self.send_json(self.rest_request(reqwest::Method::GET, path, REST_ACCEPT))?;
            notifications.push(ReviewNotification {
                id: notification.id,
                key: PullRequestKey {
                    repository: notification.repository.full_name,
                    number: pull.number,
                },
                title: notification.subject.title,
                updated_at: notification.updated_at,
            });
        }
        Ok(ReviewNotificationPage {
            notifications,
            cursor: next_cursor,
            not_modified: false,
        })
    }
}

fn header_string(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

#[derive(serde::Deserialize)]
struct RawNotification {
    id: String,
    reason: String,
    updated_at: String,
    subject: RawSubject,
    repository: RawRepository,
}

#[derive(serde::Deserialize)]
struct RawSubject {
    title: String,
    #[serde(rename = "type")]
    kind: String,
    url: Option<String>,
}

#[derive(serde::Deserialize)]
struct RawRepository {
    full_name: String,
}

#[derive(serde::Deserialize)]
struct PullNumber {
    number: u64,
}
