mod corpus;
mod metrics;

use std::io::{IsTerminal, Write};
use std::path::Path;

use ramo_core::review_map::{REVIEW_MAP_CLASSIFIER_VERSION, REVIEW_MAP_SCHEMA_VERSION};

use crate::ReviewMapFailure;
use crate::cli::BenchmarkCommand;
use crate::ollama::PROMPT_VERSION;

pub use corpus::{BenchmarkCase, BenchmarkManifest, CANDIDATE_MODELS};
pub use metrics::{BenchmarkRun, CandidateMeasurement, CompletionState};

pub async fn run_command(command: BenchmarkCommand) -> Result<(), ReviewMapFailure> {
    match command {
        BenchmarkCommand::Init {
            repo_path,
            pull_requests,
            recent,
            yes,
        } => init(&repo_path, pull_requests, recent, yes),
    }
}

fn init(
    repo_path: &Path,
    mut pull_requests: Vec<u64>,
    recent: Option<usize>,
    yes: bool,
) -> Result<(), ReviewMapFailure> {
    let repository_path = repo_path
        .canonicalize()
        .map_err(|error| benchmark_io("Could not resolve the benchmark repository path", error))?;
    let repository = gh_output(
        &repository_path,
        &[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "--jq",
            ".nameWithOwner",
        ],
    )?;
    if let Some(count) = recent {
        if !(6..=10).contains(&count) {
            return Err(invalid(
                "--recent must select between 6 and 10 pull requests",
            ));
        }
        let output = gh_output(
            &repository_path,
            &[
                "pr",
                "list",
                "--state",
                "all",
                "--limit",
                "100",
                "--json",
                "number,isDraft,updatedAt",
                "--jq",
                "sort_by(.updatedAt) | reverse | map(select(.isDraft == false)) | .[].number",
            ],
        )?;
        pull_requests.extend(
            output
                .lines()
                .filter_map(|line| line.trim().parse::<u64>().ok())
                .take(count),
        );
        println!(
            "Selected recent pull requests: {}",
            pull_requests
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
        if !yes && !confirm("Create this private benchmark corpus? [y/N] ")? {
            return Ok(());
        }
    }
    let manifest = BenchmarkManifest::new(
        repository_path.clone(),
        repository.trim().to_owned(),
        pull_requests,
        CANDIDATE_MODELS
            .iter()
            .map(|value| (*value).into())
            .collect(),
    )?;
    let path = repository_path.join(".ramo-benchmark/manifest.json");
    manifest.save(&path)?;
    println!("Created private benchmark manifest at {}", path.display());
    println!(
        "Candidates: {}\nContract: prompt {} / schema {} / classifier {}",
        manifest.candidates.join(", "),
        PROMPT_VERSION,
        REVIEW_MAP_SCHEMA_VERSION,
        REVIEW_MAP_CLASSIFIER_VERSION
    );
    Ok(())
}

fn gh_output(repository: &Path, arguments: &[&str]) -> Result<String, ReviewMapFailure> {
    let output = std::process::Command::new("gh")
        .current_dir(repository)
        .args(arguments)
        .output()
        .map_err(|error| benchmark_io("GitHub CLI is unavailable", error))?;
    if !output.status.success() {
        return Err(invalid("GitHub CLI could not resolve the benchmark corpus"));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| benchmark_io("GitHub CLI returned invalid text", error))
}

fn confirm(prompt: &str) -> Result<bool, ReviewMapFailure> {
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    print!("{prompt}");
    std::io::stdout()
        .flush()
        .map_err(|error| benchmark_io("Could not display benchmark confirmation", error))?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| benchmark_io("Could not read benchmark confirmation", error))?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub(crate) fn invalid(message: impl Into<String>) -> ReviewMapFailure {
    ReviewMapFailure::new(
        ramo_core::review_map::ReviewMapFailureCode::AnalysisFailed,
        message,
    )
}

pub(crate) fn benchmark_io(
    message: impl Into<String>,
    error: impl std::error::Error + Send + Sync + 'static,
) -> ReviewMapFailure {
    ReviewMapFailure::with_source(
        ramo_core::review_map::ReviewMapFailureCode::CacheUnavailable,
        message,
        error,
    )
}
