use std::path::Path;

use super::ReviewFileKind;

const GENERATED_NAMES: &[&str] = &[
    "cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "poetry.lock",
    "uv.lock",
    "yarn.lock",
];
const DOCUMENT_EXTENSIONS: &[&str] = &["md", "mdx", "rst", "adoc"];

#[derive(Debug, Clone, Default)]
pub struct ClassifierConfig {
    additional_test_patterns: Vec<String>,
    additional_generated_patterns: Vec<String>,
}

impl ClassifierConfig {
    pub fn with_patterns(
        test_patterns: impl IntoIterator<Item = String>,
        generated_patterns: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            additional_test_patterns: test_patterns.into_iter().collect(),
            additional_generated_patterns: generated_patterns.into_iter().collect(),
        }
    }
}

pub fn classify_path(path: &str, patch: Option<&str>, config: &ClassifierConfig) -> ReviewFileKind {
    let normalized = path.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    let file_name = Path::new(&lower)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&lower);

    if generated_path(&lower, file_name)
        || config
            .additional_generated_patterns
            .iter()
            .any(|pattern| simple_pattern_matches(pattern, &lower))
        || patch.is_some_and(has_generated_marker)
    {
        return ReviewFileKind::Generated;
    }
    if test_path(&lower, file_name)
        || config
            .additional_test_patterns
            .iter()
            .any(|pattern| simple_pattern_matches(pattern, &lower))
    {
        return ReviewFileKind::Test;
    }
    if lower
        .split('/')
        .any(|part| part == "migrations" || part == "migration")
    {
        return ReviewFileKind::Migration;
    }
    if lower.split('/').any(|part| part == "docs" || part == "doc")
        || Path::new(&lower)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| DOCUMENT_EXTENSIONS.contains(&extension))
    {
        return ReviewFileKind::Documentation;
    }
    ReviewFileKind::Authored
}

fn generated_path(path: &str, file_name: &str) -> bool {
    GENERATED_NAMES.contains(&file_name)
        || path
            .split('/')
            .any(|part| matches!(part, "generated" | "dist" | "vendor"))
        || file_name.contains(".generated.")
        || file_name.ends_with("_generated.rs")
        || file_name.ends_with(".g.dart")
}

fn test_path(path: &str, file_name: &str) -> bool {
    path.split('/')
        .any(|part| matches!(part, "test" | "tests" | "__tests__" | "spec" | "specs"))
        || (file_name.starts_with("test_") && file_name.ends_with(".py"))
        || file_name.ends_with("_test.go")
        || [
            ".test.js",
            ".test.jsx",
            ".test.ts",
            ".test.tsx",
            ".spec.js",
            ".spec.jsx",
            ".spec.ts",
            ".spec.tsx",
        ]
        .iter()
        .any(|suffix| file_name.ends_with(suffix))
}

fn has_generated_marker(patch: &str) -> bool {
    let prefix = patch.get(..patch.len().min(2048)).unwrap_or(patch);
    let lower = prefix.to_ascii_lowercase();
    ["@generated", "generated file", "do not edit"]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn simple_pattern_matches(pattern: &str, path: &str) -> bool {
    globset::Glob::new(pattern)
        .map(|glob| glob.compile_matcher().is_match(path))
        .unwrap_or(false)
}
