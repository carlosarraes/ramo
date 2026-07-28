use serde::de::DeserializeOwned;

use crate::{GithubClient, GithubError, GithubErrorKind};

#[derive(serde::Serialize)]
struct GraphqlRequest<'a, T> {
    query: &'a str,
    variables: T,
}

#[derive(serde::Deserialize)]
struct GraphqlEnvelope<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphqlResponseError>,
}

#[derive(serde::Deserialize)]
struct GraphqlResponseError {
    message: String,
}

impl GithubClient {
    pub(crate) fn graphql<T, V>(&self, query: &str, variables: V) -> Result<T, GithubError>
    where
        T: DeserializeOwned,
        V: serde::Serialize,
    {
        let envelope: GraphqlEnvelope<T> = self.send_json(
            self.graphql_request()
                .json(&GraphqlRequest { query, variables }),
        )?;
        if !envelope.errors.is_empty() {
            let message = envelope
                .errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(GithubError::new(
                GithubErrorKind::Graphql,
                format!("GitHub GraphQL request failed: {message}"),
            ));
        }
        envelope.data.ok_or_else(|| {
            GithubError::new(
                GithubErrorKind::Decode,
                "GitHub GraphQL response had no data",
            )
        })
    }
}
