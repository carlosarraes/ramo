use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Path, State, rejection::JsonRejection};
use axum::http::{Request, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use ramo_core::github::PullRequestKey;
use ramo_core::review_map::{REVIEW_MAP_SCHEMA_VERSION, ReviewMapFailureCode, ReviewMapStatus};

use crate::ReviewMapFailure;
use crate::analysis::{AnalysisCoordinator, AnalysisJobId, ResolveRequest, ResolveResult};

use super::auth::{ClientCredential, PairingState, ReviewMapClientTokenStore};
use super::wire::{ApiError, ReviewMapResponse};

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthStatus {
    pub schema_version: u16,
    pub service: bool,
    pub github: bool,
    pub ollama: bool,
    pub cache: bool,
    pub model: String,
    pub server_version: String,
}

impl HealthStatus {
    pub fn healthy(model: impl Into<String>) -> Self {
        Self {
            schema_version: REVIEW_MAP_SCHEMA_VERSION,
            service: true,
            github: true,
            ollama: true,
            cache: true,
            model: model.into(),
            server_version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

#[derive(Clone)]
pub struct ServerState {
    pub coordinator: AnalysisCoordinator,
    pub tokens: ReviewMapClientTokenStore,
    pub pairing: PairingState,
    pub health: HealthStatus,
}

pub fn build_router(state: ServerState) -> Router {
    let protected = Router::new()
        .route("/v1/review-maps", post(create_review_map))
        .route("/v1/review-maps/{job_id}", get(get_review_map))
        .route("/v1/review-maps/{job_id}/retry", post(retry_review_map))
        .route("/v1/clients/{client_id}", delete(revoke_client))
        .route_layer(middleware::from_fn_with_state(
            state.tokens.clone(),
            require_client,
        ));
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/pair/exchange", post(exchange_pairing_code))
        .merge(protected)
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .with_state(state)
        .layer(middleware::from_fn(log_request))
}

async fn health(State(state): State<ServerState>) -> Json<HealthStatus> {
    Json(state.health)
}

#[derive(serde::Deserialize)]
struct PairExchangeRequest {
    code: String,
    #[serde(default = "default_client_label")]
    label: String,
}

async fn exchange_pairing_code(
    State(state): State<ServerState>,
    payload: Result<Json<PairExchangeRequest>, JsonRejection>,
) -> Result<Json<ClientCredential>, ApiError> {
    let payload = payload.map_err(|_| ApiError::bad_request("Invalid pairing request"))?;
    state
        .pairing
        .exchange(&payload.code, payload.label.clone())
        .map(Json)
        .map_err(ApiError::from_failure)
}

#[derive(serde::Deserialize)]
struct CreateReviewMapRequest {
    #[serde(default = "current_schema_version")]
    schema_version: u16,
    repository: String,
    pull_request: u64,
    expected_head_sha: String,
}

async fn create_review_map(
    State(state): State<ServerState>,
    payload: Result<Json<CreateReviewMapRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let payload = payload.map_err(|_| ApiError::bad_request("Invalid Review Map request"))?;
    if payload.schema_version != REVIEW_MAP_SCHEMA_VERSION {
        return Err(ApiError::from_failure(ReviewMapFailure::new(
            ReviewMapFailureCode::ServerIncompatible,
            "The client and server use incompatible Review Map schemas",
        )));
    }
    if !valid_repository(&payload.repository)
        || payload.pull_request == 0
        || payload.expected_head_sha.trim().is_empty()
    {
        return Err(ApiError::bad_request(
            "repository must use owner/name form and include a PR and expected head SHA",
        ));
    }
    let result = state
        .coordinator
        .resolve(ResolveRequest {
            key: PullRequestKey {
                repository: payload.repository.clone(),
                number: payload.pull_request,
            },
            expected_head_sha: Some(payload.expected_head_sha.clone()),
        })
        .await
        .map_err(ApiError::from_failure)?;
    Ok((StatusCode::ACCEPTED, Json(ReviewMapResponse::from(result))).into_response())
}

async fn get_review_map(
    State(state): State<ServerState>,
    Path(job_id): Path<String>,
) -> Result<Response, ApiError> {
    let snapshot = state
        .coordinator
        .job(&AnalysisJobId(job_id))
        .await
        .ok_or_else(|| {
            ApiError::from_failure(ReviewMapFailure::new(
                ReviewMapFailureCode::PullRequestUnavailable,
                "The Review Map job was not found",
            ))
        })?;
    let status = if matches!(
        snapshot.map.status,
        ReviewMapStatus::Analyzing | ReviewMapStatus::Ready
    ) {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    let result = ResolveResult {
        job_id: snapshot.job_id,
        state: snapshot.state,
        map: snapshot.map,
    };
    Ok((status, Json(ReviewMapResponse::from(result))).into_response())
}

async fn retry_review_map(
    State(state): State<ServerState>,
    Path(job_id): Path<String>,
) -> Result<Response, ApiError> {
    let result = state
        .coordinator
        .retry(&AnalysisJobId(job_id))
        .await
        .map_err(ApiError::from_failure)?;
    Ok((StatusCode::ACCEPTED, Json(ReviewMapResponse::from(result))).into_response())
}

async fn revoke_client(
    State(state): State<ServerState>,
    Path(client_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if state
        .tokens
        .revoke(&client_id)
        .map_err(ApiError::from_failure)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::from_failure(ReviewMapFailure::new(
            ReviewMapFailureCode::ClientUnauthorized,
            "The paired client was not found",
        )))
    }
}

async fn require_client(
    State(tokens): State<ReviewMapClientTokenStore>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| tokens.authorize(token));
    if !authorized {
        return ApiError::from_failure(ReviewMapFailure::new(
            ReviewMapFailureCode::ClientUnauthorized,
            "A valid paired-client token is required",
        ))
        .into_response();
    }
    next.run(request).await
}

async fn log_request(request: Request<Body>, next: Next) -> Response {
    let started = Instant::now();
    let route = request.uri().path().to_owned();
    let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let response = next.run(request).await;
    tracing::info!(
        request_id,
        route = %route,
        status = response.status().as_u16(),
        duration_ms = started.elapsed().as_millis() as u64,
        "review server request"
    );
    response
}

async fn not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        ReviewMapFailure::new(
            ReviewMapFailureCode::PullRequestUnavailable,
            "The requested server route was not found",
        ),
    )
}

async fn method_not_allowed() -> ApiError {
    ApiError::bad_request("The HTTP method is not supported for this route")
}

fn valid_repository(repository: &str) -> bool {
    repository
        .split_once('/')
        .is_some_and(|(owner, name)| !owner.is_empty() && !name.is_empty() && !name.contains('/'))
}

fn default_client_label() -> String {
    "Ramo client".into()
}

fn current_schema_version() -> u16 {
    REVIEW_MAP_SCHEMA_VERSION
}
