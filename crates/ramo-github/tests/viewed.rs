use httpmock::prelude::*;
use ramo_github::GithubClient;

fn client(server: &MockServer) -> GithubClient {
    GithubClient::with_endpoints(
        "token".into(),
        server.base_url(),
        format!("{}/graphql", server.base_url()),
    )
    .unwrap()
}

#[test]
fn viewed_state_uses_the_matching_graphql_mutation_and_exact_variables() {
    let server = MockServer::start();
    let mark = server.mock(|when, then| {
        when.method(POST)
            .path("/graphql")
            .body_includes("mutation MarkFileAsViewed")
            .body_includes("\"pullRequestId\":\"PR_node\"")
            .body_includes("\"path\":\"src/lib.rs\"");
        then.status(200).json_body_obj(
            &serde_json::json!({"data":{"markFileAsViewed":{"clientMutationId":null}}}),
        );
    });
    let unmark = server.mock(|when, then| {
        when.method(POST)
            .path("/graphql")
            .body_includes("mutation UnmarkFileAsViewed")
            .body_includes("\"pullRequestId\":\"PR_node\"")
            .body_includes("\"path\":\"src/lib.rs\"");
        then.status(200).json_body_obj(
            &serde_json::json!({"data":{"unmarkFileAsViewed":{"clientMutationId":null}}}),
        );
    });

    let client = client(&server);
    client
        .set_file_viewed("PR_node", "src/lib.rs", true)
        .unwrap();
    client
        .set_file_viewed("PR_node", "src/lib.rs", false)
        .unwrap();

    mark.assert();
    unmark.assert();
}
