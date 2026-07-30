use ramo_core::review_map::{EnrichmentRequest, ReviewFileKind};
use serde_json::{Value, json};

pub fn enrichment_schema(request: &EnrichmentRequest) -> Value {
    let all_paths = request
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let flexible_paths = request
        .files
        .iter()
        .filter(|file| !matches!(file.kind, ReviewFileKind::Test | ReviewFileKind::Generated))
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let required_insight_count = flexible_paths.len();
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["groups", "files", "review_order", "coverage"],
        "properties": {
            "groups": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["label", "summary", "risk", "review_priority", "paths"],
                    "properties": {
                        "label": { "type": "string", "maxLength": 80 },
                        "summary": { "type": "string", "maxLength": 400 },
                        "risk": { "type": ["string", "null"], "maxLength": 240 },
                        "review_priority": { "type": "integer", "minimum": 0 },
                        "paths": {
                            "type": "array",
                            "uniqueItems": true,
                            "items": { "type": "string", "enum": flexible_paths }
                        }
                    }
                }
            },
            "files": {
                "type": "array",
                "minItems": required_insight_count,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["path", "summary", "risk"],
                    "properties": {
                        "path": { "type": "string", "enum": all_paths },
                        "summary": { "type": "string", "maxLength": 400 },
                        "risk": { "type": ["string", "null"], "maxLength": 240 }
                    }
                }
            },
            "review_order": {
                "type": "array",
                "uniqueItems": true,
                "items": { "type": "string", "enum": flexible_paths }
            },
            "coverage": {
                "type": "object",
                "additionalProperties": false,
                "required": ["analyzed_paths", "truncated_paths", "metadata_only_paths", "binary_paths"],
                "properties": {
                    "analyzed_paths": {
                        "type": "array",
                        "uniqueItems": true,
                        "items": { "type": "string", "enum": all_paths }
                    },
                    "truncated_paths": {
                        "type": "array",
                        "uniqueItems": true,
                        "items": { "type": "string", "enum": all_paths }
                    },
                    "metadata_only_paths": {
                        "type": "array",
                        "uniqueItems": true,
                        "items": { "type": "string", "enum": all_paths }
                    },
                    "binary_paths": {
                        "type": "array",
                        "uniqueItems": true,
                        "items": { "type": "string", "enum": all_paths }
                    }
                }
            }
        }
    })
}
