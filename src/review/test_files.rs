use globset::{Glob, GlobSet, GlobSetBuilder};

const BUILTIN_PATTERNS: &[&str] = &[
    "test/**",
    "tests/**",
    "__tests__/**",
    "spec/**",
    "**/test/**",
    "**/tests/**",
    "**/__tests__/**",
    "**/spec/**",
    "test_*",
    "**/test_*",
    "*_test.*",
    "**/*_test.*",
    "*.test.*",
    "**/*.test.*",
    "*.spec.*",
    "**/*.spec.*",
];

pub(crate) struct TestFileMatcher {
    patterns: GlobSet,
}

impl TestFileMatcher {
    pub(crate) fn new(extra_patterns: &[String]) -> Result<Self, String> {
        let mut builder = GlobSetBuilder::new();
        for pattern in BUILTIN_PATTERNS
            .iter()
            .copied()
            .chain(extra_patterns.iter().map(String::as_str))
        {
            builder.add(
                Glob::new(pattern)
                    .map_err(|error| format!("invalid test file pattern {pattern:?}: {error}"))?,
            );
        }
        Ok(Self {
            patterns: builder.build().map_err(|error| error.to_string())?,
        })
    }

    pub(crate) fn is_match(&self, path: &str) -> bool {
        self.patterns.is_match(path)
    }
}

pub(crate) fn validate_test_file_pattern(pattern: &str) -> Result<(), String> {
    Glob::new(pattern)
        .map(|_| ())
        .map_err(|error| format!("invalid test_file_patterns entry {pattern:?}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::TestFileMatcher;

    #[test]
    fn builtins_match_test_directories_and_common_file_names() {
        let matcher = TestFileMatcher::new(&[]).unwrap();
        for path in [
            "test/api.rs",
            "tests/api.rs",
            "frontend/__tests__/button.tsx",
            "src/spec/parser.rb",
            "test_models.py",
            "src/models_test.go",
            "src/button.test.tsx",
            "src/button.spec.ts",
        ] {
            assert!(matcher.is_match(path), "{path}");
        }
        assert!(!matcher.is_match("src/contest.rs"));
        assert!(!matcher.is_match("src/latest_news.ts"));
    }

    #[test]
    fn custom_patterns_extend_builtins_and_invalid_patterns_fail() {
        let matcher = TestFileMatcher::new(&["qa/**/*.feature".into()]).unwrap();
        assert!(matcher.is_match("qa/auth/login.feature"));
        assert!(matcher.is_match("tests/auth.rs"));
        assert!(TestFileMatcher::new(&["[".into()]).is_err());
    }
}
