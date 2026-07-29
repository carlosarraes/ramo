use globset::{Glob, GlobMatcher};

#[derive(Debug)]
struct Rule {
    matcher: GlobMatcher,
    owner: String,
}

#[derive(Debug, Default)]
pub struct CodeOwners {
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeOwnersError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for CodeOwnersError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid CODEOWNERS rule at line {}: {}",
            self.line, self.message
        )
    }
}

impl std::error::Error for CodeOwnersError {}

impl CodeOwners {
    pub fn parse(source: &str) -> Result<Self, CodeOwnersError> {
        let mut rules = Vec::new();
        for (index, raw_line) in source.lines().enumerate() {
            let line_number = index + 1;
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let pattern = fields.next().expect("non-empty line");
            let owner = fields.next().ok_or_else(|| CodeOwnersError {
                line: line_number,
                message: "rule must name at least one owner".into(),
            })?;
            let glob_pattern = normalize_pattern(pattern);
            let matcher = Glob::new(&glob_pattern)
                .map_err(|error| CodeOwnersError {
                    line: line_number,
                    message: error.to_string(),
                })?
                .compile_matcher();
            rules.push(Rule {
                matcher,
                owner: owner.to_owned(),
            });
        }
        Ok(Self { rules })
    }

    pub fn owner_for(&self, path: &str) -> Option<&str> {
        let normalized = path.replace('\\', "/");
        self.rules
            .iter()
            .rev()
            .find(|rule| rule.matcher.is_match(&normalized))
            .map(|rule| rule.owner.as_str())
    }
}

fn normalize_pattern(pattern: &str) -> String {
    let rooted = pattern.trim_start_matches('/');
    if rooted == "*" {
        return "**".into();
    }
    if let Some(directory) = rooted.strip_suffix('/') {
        return format!("{directory}/**");
    }
    if rooted.contains('/') {
        rooted.to_owned()
    } else {
        format!("**/{rooted}")
    }
}
