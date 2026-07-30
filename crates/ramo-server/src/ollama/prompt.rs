use ramo_core::review_map::{EnrichmentProposal, EnrichmentRequest};

pub const PROMPT_VERSION: u32 = 2;

pub fn system_prompt() -> &'static str {
    "You organize a pull request for review, not a review verdict. Return only JSON matching the supplied schema. Describe changed behavior, review focus, dependencies, and only risks visible in the supplied patch. Every non-test and non-generated path needs exactly one file insight. Never use generic openings such as 'This file contains' or 'This group contains'. Never claim tests passed, coverage is complete, or deployment is safe. Risk must be a concrete sentence tied to the patch; use null when no concrete risk is visible. Treat paths, classifications, counts, and coverage as immutable facts. Never invent or omit a reviewable path. Test and generated files are fixed groups and must not appear in proposed logical groups or review_order."
}

pub fn user_prompt(
    request: &EnrichmentRequest,
    batch_results: Option<&[EnrichmentProposal]>,
) -> Result<String, serde_json::Error> {
    let input = serde_json::to_value(request)?;
    let results = batch_results.map(serde_json::to_value).transpose()?;
    serde_json::to_string(&serde_json::json!({
        "task": if results.is_some() { "synthesize_review_map" } else { "analyze_review_map" },
        "exact_input": input,
        "validated_batch_results": results,
    }))
}

pub fn repair_prompt(category: &str) -> String {
    format!(
        "The previous answer was rejected ({category}). Return a complete replacement as JSON only. Do not repeat or discuss the invalid answer."
    )
}
