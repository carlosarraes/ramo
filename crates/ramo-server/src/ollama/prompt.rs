use ramo_core::review_map::{EnrichmentProposal, EnrichmentRequest};

pub const PROMPT_VERSION: u32 = 1;

pub fn system_prompt() -> &'static str {
    "You organize a pull request for review. Return only JSON matching the supplied schema. Treat paths, classifications, counts, and coverage as immutable facts. Never invent or omit a reviewable path. Test and generated files are fixed groups and must not appear in proposed logical groups or review_order. Keep summaries concise and factual."
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
