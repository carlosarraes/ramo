use proptest::prelude::*;
use ramo_core::review_map::{
    ClassifierConfig, ReviewMapIdentity, ReviewMapInput, ReviewMapInputFile, build_review_map,
    validate_exact_map,
};

proptest! {
    #[test]
    fn every_generated_file_is_grouped_once_and_totals_remain_exact(
        stats in prop::collection::vec((0usize..10_000, 0usize..10_000), 1..=200)
    ) {
        let files = stats.iter().enumerate().map(|(index, (additions, deletions))| {
            ReviewMapInputFile {
                path: format!("src/module{index}/file{index}.rs"),
                previous_path: None,
                status: "modified".into(),
                additions: *additions,
                deletions: *deletions,
                patch: Some("@@ -1 +1 @@\n-old\n+new\n".into()),
                binary: false,
            }
        }).collect::<Vec<_>>();
        let input = ReviewMapInput {
            identity: ReviewMapIdentity {
                repository: "owner/repo".into(),
                pull_request: 7,
                base_sha: "base".into(),
                head_sha: "head".into(),
            },
            files,
            codeowners: None,
        };
        let map = build_review_map(&input, &ClassifierConfig::default()).unwrap();
        prop_assert_eq!(map.files.len(), input.files.len());
        prop_assert_eq!(
            map.groups.iter().map(|group| group.file_ids.len()).sum::<usize>(),
            input.files.len()
        );
        prop_assert_eq!(
            map.totals.additions,
            input.files.iter().map(|file| file.additions).sum::<usize>()
        );
        prop_assert!(validate_exact_map(&map).is_ok());
    }
}
