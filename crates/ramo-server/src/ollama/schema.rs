use serde_json::{Value, json};

pub fn enrichment_schema() -> Value {
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
                        "paths": { "type": "array", "items": { "type": "string" } }
                    }
                }
            },
            "files": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["path", "summary", "risk"],
                    "properties": {
                        "path": { "type": "string" },
                        "summary": { "type": "string", "maxLength": 400 },
                        "risk": { "type": ["string", "null"], "maxLength": 240 }
                    }
                }
            },
            "review_order": { "type": "array", "items": { "type": "string" } },
            "coverage": {
                "type": "object",
                "additionalProperties": false,
                "required": ["analyzed_paths", "truncated_paths", "metadata_only_paths", "binary_paths"],
                "properties": {
                    "analyzed_paths": { "type": "array", "items": { "type": "string" } },
                    "truncated_paths": { "type": "array", "items": { "type": "string" } },
                    "metadata_only_paths": { "type": "array", "items": { "type": "string" } },
                    "binary_paths": { "type": "array", "items": { "type": "string" } }
                }
            }
        }
    })
}
