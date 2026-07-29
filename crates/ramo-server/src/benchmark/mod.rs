mod blind;
mod corpus;
mod metrics;
mod resources;
mod runner;

use std::io::{IsTerminal, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ramo_core::review_map::{REVIEW_MAP_CLASSIFIER_VERSION, REVIEW_MAP_SCHEMA_VERSION};

use crate::ReviewMapFailure;
use crate::cli::BenchmarkCommand;
use crate::ollama::PROMPT_VERSION;

pub use corpus::{BenchmarkCase, BenchmarkManifest, CANDIDATE_MODELS};
pub use metrics::{BenchmarkRun, CandidateMeasurement, CompletionState};
pub use runner::{BenchmarkAnalyzerFactory, BenchmarkRunner, OllamaBenchmarkAnalyzerFactory};

pub async fn run_command(command: BenchmarkCommand) -> Result<(), ReviewMapFailure> {
    match command {
        BenchmarkCommand::Init {
            repo_path,
            pull_requests,
            recent,
            yes,
        } => init(&repo_path, pull_requests, recent, yes),
        BenchmarkCommand::Run { manifest, yes } => run_benchmark(&manifest, yes).await,
        BenchmarkCommand::Judge { manifest } => judge(&manifest),
        BenchmarkCommand::Reveal { manifest } => reveal(&manifest),
    }
}

fn judge(manifest_path: &Path) -> Result<(), ReviewMapFailure> {
    let run_directory = benchmark_run_directory(manifest_path)?;
    let run = BenchmarkRun::load(&run_directory.join("run.json"))?;
    let mut session = BlindSession::open(&run_directory, &run)?;
    let judgments_path = run_directory.join("judgments.json");
    while let Some(payload) = session.next() {
        println!(
            "{}",
            serde_json::to_string_pretty(&payload)
                .map_err(|error| benchmark_io("Could not render blind comparison", error))?
        );
        let Some(judgment) = read_judgment()? else {
            session.save(&judgments_path)?;
            println!(
                "Saved {}/{} comparisons",
                session.completed(),
                session.total()
            );
            return Ok(());
        };
        session.submit(judgment)?;
        session.save(&judgments_path)?;
        println!(
            "Recorded {}/{} comparisons",
            session.completed(),
            session.total()
        );
    }
    println!("Blind judging complete. Run ramo server benchmark reveal.");
    Ok(())
}

fn reveal(manifest_path: &Path) -> Result<(), ReviewMapFailure> {
    let run_directory = benchmark_run_directory(manifest_path)?;
    let run = BenchmarkRun::load(&run_directory.join("run.json"))?;
    let session = BlindSession::open(&run_directory, &run)?;
    println!("Explicit benchmark identity reveal:");
    for (candidate, model) in session.reveal() {
        println!("  {candidate}: {model}");
    }
    Ok(())
}

fn benchmark_run_directory(manifest_path: &Path) -> Result<std::path::PathBuf, ReviewMapFailure> {
    manifest_path
        .parent()
        .map(|parent| parent.join("run"))
        .ok_or_else(|| invalid("Benchmark manifest path has no parent directory"))
}

fn read_judgment() -> Result<Option<BlindJudgment>, ReviewMapFailure> {
    println!(
        "Enter A grouping accuracy order risks noise, then B's five scores, then A/B/tie; or q to save:"
    );
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|error| benchmark_io("Could not read blind judgment", error))?;
    if line.trim().eq_ignore_ascii_case("q") {
        return Ok(None);
    }
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 11 {
        return Err(invalid(
            "A blind judgment requires ten scores and one overall choice",
        ));
    }
    let scores = fields[..10]
        .iter()
        .map(|value| value.parse::<u8>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| benchmark_io("Blind scores must be integers from 1 to 5", error))?;
    let dimensions = |offset: usize| DimensionScores {
        grouping: scores[offset],
        accuracy: scores[offset + 1],
        order: scores[offset + 2],
        risks: scores[offset + 3],
        noise: scores[offset + 4],
    };
    let overall = match fields[10].to_ascii_lowercase().as_str() {
        "a" => BlindChoice::CandidateA,
        "b" => BlindChoice::CandidateB,
        "tie" => BlindChoice::Tie,
        _ => return Err(invalid("Overall blind choice must be A, B, or tie")),
    };
    Ok(Some(BlindJudgment {
        candidate_a: dimensions(0),
        candidate_b: dimensions(5),
        overall,
    }))
}

async fn run_benchmark(manifest_path: &Path, yes: bool) -> Result<(), ReviewMapFailure> {
    let manifest = BenchmarkManifest::load(manifest_path)?;
    ensure_models(&manifest.candidates, yes)?;
    let benchmark_root = manifest_path
        .parent()
        .ok_or_else(|| invalid("Benchmark manifest path has no parent directory"))?;
    let run_directory = benchmark_root.join("run");
    let run_path = run_directory.join("run.json");
    let mut run = if run_path.is_file() {
        let run = BenchmarkRun::load(&run_path)?;
        if !run.is_compatible_with(&manifest) {
            return Err(invalid(
                "Existing benchmark run does not match this manifest; archive it before starting a new run",
            ));
        }
        run
    } else {
        BenchmarkRun::new(
            format!(
                "run-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            ),
            &manifest,
            42,
        )
    };
    let factory = Arc::new(OllamaBenchmarkAnalyzerFactory::new(
        "http://127.0.0.1:11434",
        Duration::from_secs(120),
    ));
    let runner = BenchmarkRunner::new(
        Arc::new(crate::github::GithubPullRequestProvider::new()),
        factory,
        run_directory.clone(),
    );
    runner.run(&manifest, &mut run).await?;
    println!(
        "Benchmark run {} contains {} measurements at {}",
        run.run_id,
        run.measurements.len(),
        run_directory.display()
    );
    Ok(())
}

fn ensure_models(candidates: &[String], yes: bool) -> Result<(), ReviewMapFailure> {
    let output = std::process::Command::new("ollama")
        .arg("list")
        .output()
        .map_err(|error| benchmark_io("Ollama is unavailable", error))?;
    if !output.status.success() {
        return Err(invalid("Ollama could not list installed models"));
    }
    let installed = String::from_utf8(output.stdout)
        .map_err(|error| benchmark_io("Ollama returned an invalid model list", error))?
        .lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect::<std::collections::HashSet<_>>();
    let missing = candidates
        .iter()
        .filter(|candidate| {
            !installed.contains(candidate.as_str())
                && !installed.contains(format!("{candidate}:latest").as_str())
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    println!("Missing benchmark models:");
    for model in &missing {
        println!("  {model} ({})", expected_download_size(model));
    }
    if !yes && !confirm("Pull these models sequentially? [y/N] ")? {
        return Err(invalid("Benchmark model download was not approved"));
    }
    for model in missing {
        let status = std::process::Command::new("ollama")
            .args(["pull", model])
            .status()
            .map_err(|error| benchmark_io("Could not start Ollama model download", error))?;
        if !status.success() {
            return Err(invalid(format!("Ollama could not pull model '{model}'")));
        }
    }
    Ok(())
}

fn expected_download_size(model: &str) -> &'static str {
    match model {
        "qwen3:8b" => "about 5.2 GB",
        "qwen3-coder:30b" => "about 18 GB",
        "qwen2.5-coder:7b" => "about 4.7 GB",
        _ => "size reported by Ollama",
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
pub use blind::{
    BlindCandidateOutput, BlindChoice, BlindJudgment, BlindPayload, BlindSession, DimensionScores,
};
