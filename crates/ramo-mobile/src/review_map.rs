use std::collections::HashSet;

use ramo_core::github::PullRequestSnapshot;
use ramo_core::review_map::{
    ClassifierConfig, PatchCoverage, ReviewFileKind, ReviewMap, ReviewMapInput, ReviewMapInputFile,
    ReviewMapStatus, build_review_map,
};

use crate::{
    MobileError, MobilePatchCoverage, MobileReviewFileKind, MobileReviewMap, MobileReviewMapFile,
    MobileReviewMapGroup, MobileReviewMapStatus,
};

pub(crate) fn mobile_review_map(
    snapshot: &PullRequestSnapshot,
) -> Result<MobileReviewMap, MobileError> {
    let input = ReviewMapInput {
        identity: ramo_core::review_map::ReviewMapIdentity {
            repository: snapshot.context.repository.clone(),
            pull_request: snapshot.context.number,
            base_sha: snapshot.context.base_revision.clone(),
            head_sha: snapshot.context.captured_revision.clone(),
        },
        files: snapshot
            .files
            .iter()
            .map(|file| ReviewMapInputFile {
                path: file.path.clone(),
                previous_path: file.previous_path.clone(),
                status: file.status.clone(),
                additions: file.additions,
                deletions: file.deletions,
                patch: file.patch.clone(),
                binary: file.binary,
            })
            .collect(),
        codeowners: None,
    };
    let map = build_review_map(&input, &ClassifierConfig::default())
        .map_err(|_| MobileError::Unexpected)?;
    Ok(project_map(&map, snapshot))
}

fn project_map(map: &ReviewMap, snapshot: &PullRequestSnapshot) -> MobileReviewMap {
    let viewed = snapshot
        .files
        .iter()
        .filter(|file| file.viewed)
        .map(|file| file.path.as_str())
        .collect::<HashSet<_>>();
    MobileReviewMap {
        schema_version: u64::from(map.schema_version),
        repository: map.identity.repository.clone(),
        number: map.identity.pull_request,
        base_sha: map.identity.base_sha.clone(),
        head_sha: map.identity.head_sha.clone(),
        status: status(map.status),
        file_count: map.totals.files as u64,
        additions: map.totals.additions as u64,
        deletions: map.totals.deletions as u64,
        authored_count: map.totals.authored as u64,
        test_count: map.totals.tests as u64,
        generated_count: map.totals.generated as u64,
        migration_count: map.totals.migrations as u64,
        documentation_count: map.totals.documentation as u64,
        groups: map.groups.iter().map(project_group).collect(),
        files: map
            .files
            .iter()
            .map(|file| MobileReviewMapFile {
                id: file.id.clone(),
                path: file.path.clone(),
                previous_path: file.previous_path.clone(),
                status: file.status.clone(),
                additions: file.additions as u64,
                deletions: file.deletions as u64,
                kind: kind(file.kind),
                owner: file.owner.clone(),
                coverage: coverage(file.coverage),
                summary: file.insight.as_ref().map(|insight| insight.summary.clone()),
                risk: file
                    .insight
                    .as_ref()
                    .and_then(|insight| insight.risk.clone()),
                recommended_order: file.recommended_order.map(|order| order as u64),
                viewed: viewed.contains(file.path.as_str()),
            })
            .collect(),
        analysis_model: map.analysis.as_ref().map(|analysis| analysis.model.clone()),
        analysis_completed_at: map
            .analysis
            .as_ref()
            .map(|analysis| analysis.completed_at.clone()),
    }
}

fn project_group(group: &ramo_core::review_map::ReviewMapGroup) -> MobileReviewMapGroup {
    MobileReviewMapGroup {
        id: group.id.clone(),
        label: group.label.clone(),
        kind: kind(group.kind),
        file_ids: group.file_ids.clone(),
        additions: group.additions as u64,
        deletions: group.deletions as u64,
        collapsed_by_default: group.collapsed_by_default,
        summary: group
            .insight
            .as_ref()
            .map(|insight| insight.summary.clone()),
        risk: group
            .insight
            .as_ref()
            .and_then(|insight| insight.risk.clone()),
        review_priority: group
            .insight
            .as_ref()
            .map(|insight| insight.review_priority as u64),
    }
}

fn status(value: ReviewMapStatus) -> MobileReviewMapStatus {
    match value {
        ReviewMapStatus::Ready => MobileReviewMapStatus::Ready,
        ReviewMapStatus::Analyzing => MobileReviewMapStatus::Analyzing,
        ReviewMapStatus::Enriched => MobileReviewMapStatus::Enriched,
        ReviewMapStatus::Stale => MobileReviewMapStatus::Stale,
        ReviewMapStatus::Unavailable => MobileReviewMapStatus::Unavailable,
        ReviewMapStatus::Failed => MobileReviewMapStatus::Failed,
    }
}

fn kind(value: ReviewFileKind) -> MobileReviewFileKind {
    match value {
        ReviewFileKind::Authored => MobileReviewFileKind::Authored,
        ReviewFileKind::Test => MobileReviewFileKind::Test,
        ReviewFileKind::Generated => MobileReviewFileKind::Generated,
        ReviewFileKind::Migration => MobileReviewFileKind::Migration,
        ReviewFileKind::Documentation => MobileReviewFileKind::Documentation,
        ReviewFileKind::Other => MobileReviewFileKind::Other,
    }
}

fn coverage(value: PatchCoverage) -> MobilePatchCoverage {
    match value {
        PatchCoverage::Full => MobilePatchCoverage::Full,
        PatchCoverage::Truncated => MobilePatchCoverage::Truncated,
        PatchCoverage::MetadataOnly => MobilePatchCoverage::MetadataOnly,
        PatchCoverage::Binary => MobilePatchCoverage::Binary,
    }
}

#[cfg(test)]
mod tests {
    use ramo_core::github::{ChangedFile, PullRequestSnapshot};
    use ramo_core::remote_review::PullRequestReviewContext;

    use crate::MobileReviewFileKind;

    #[test]
    fn mobile_review_map_preserves_exact_paths_counts_and_default_folds() {
        let snapshot = fixture();
        let map = super::mobile_review_map(&snapshot).unwrap();
        assert_eq!(
            (map.files.len(), map.additions, map.deletions),
            (4, 120, 18)
        );
        assert!(
            map.groups
                .iter()
                .find(|group| group.kind == MobileReviewFileKind::Test)
                .unwrap()
                .collapsed_by_default
        );
        assert_eq!(
            map.files
                .iter()
                .filter(|file| file.path == "src/lib.rs")
                .count(),
            1
        );
    }

    fn fixture() -> PullRequestSnapshot {
        let files = [
            ("src/lib.rs", 60, 10),
            ("src/api.rs", 30, 4),
            ("tests/test_api.rs", 20, 3),
            ("generated/client.rs", 10, 1),
        ]
        .into_iter()
        .map(|(path, additions, deletions)| ChangedFile {
            path: path.into(),
            previous_path: None,
            status: "modified".into(),
            additions,
            deletions,
            patch: Some("@@ -1 +1 @@\n-old\n+new".into()),
            viewed: path == "src/lib.rs",
            binary: false,
        })
        .collect();
        PullRequestSnapshot {
            node_id: "PR_7".into(),
            context: PullRequestReviewContext {
                repository: "owner/repo".into(),
                repository_url: "https://github.com/owner/repo".into(),
                number: 7,
                title: "Review map".into(),
                body: String::new(),
                url: "https://github.com/owner/repo/pull/7".into(),
                base_ref: "main".into(),
                base_revision: "base".into(),
                head_ref: "feature".into(),
                captured_revision: "head".into(),
                author_login: "author".into(),
                viewer_login: "reviewer".into(),
            },
            files,
        }
    }
}
