use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ramo_core::review_map::{
    REVIEW_MAP_SCHEMA_VERSION, ReviewMap, ReviewMapFailureCode, ReviewMapStatus,
};

use crate::ReviewMapFailure;
use crate::analysis::{AnalysisJobId, JobState, ResolveResult};

#[derive(Debug, serde::Serialize)]
pub struct ReviewMapResponse {
    pub schema_version: u16,
    pub job_id: AnalysisJobId,
    pub state: ReviewMapStatus,
    pub map: ReviewMap,
    pub failure: Option<ReviewMapFailure>,
}

impl From<ResolveResult> for ReviewMapResponse {
    fn from(result: ResolveResult) -> Self {
        let failure = match &result.state {
            JobState::Unavailable(failure) | JobState::Failed(failure) => Some(failure.clone()),
            _ => None,
        };
        Self {
            schema_version: REVIEW_MAP_SCHEMA_VERSION,
            job_id: result.job_id,
            state: result.map.status,
            map: result.map,
            failure,
        }
    }
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    failure: ReviewMapFailure,
}

#[derive(serde::Serialize)]
struct ErrorEnvelope {
    schema_version: u16,
    failure: ReviewMapFailure,
}

impl ApiError {
    pub fn new(status: StatusCode, failure: ReviewMapFailure) -> Self {
        Self { status, failure }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ReviewMapFailure::new(ReviewMapFailureCode::AnalysisFailed, message),
        )
    }

    pub fn from_failure(failure: ReviewMapFailure) -> Self {
        let status = match failure.code {
            ReviewMapFailureCode::ClientUnauthorized => StatusCode::UNAUTHORIZED,
            ReviewMapFailureCode::PairingRejected => StatusCode::UNAUTHORIZED,
            ReviewMapFailureCode::PullRequestUnavailable => StatusCode::NOT_FOUND,
            ReviewMapFailureCode::ResultStale => StatusCode::CONFLICT,
            ReviewMapFailureCode::ServerIncompatible => StatusCode::UPGRADE_REQUIRED,
            _ => StatusCode::SERVICE_UNAVAILABLE,
        };
        Self::new(status, failure)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorEnvelope {
                schema_version: REVIEW_MAP_SCHEMA_VERSION,
                failure: self.failure,
            }),
        )
            .into_response()
    }
}
