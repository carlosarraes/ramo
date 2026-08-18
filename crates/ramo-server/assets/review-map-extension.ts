// Gives pi a schema-constrained way to answer the Review Map.
//
// pi has no `--json-schema` flag, so assistant text is never constrained. Tool-call arguments
// are, because the provider validates them against the tool's parameter schema. Registering a
// single tool and running pi with `--no-builtin-tools` therefore makes a validated object the
// only way for the model to respond.
//
// The schema is per-request (it pins path enums to the actual changed files), so it is read
// from RAMO_REVIEW_MAP_SCHEMA at load time. The result is written to RAMO_REVIEW_MAP_OUTPUT
// rather than stdout, so nothing can interleave with pi's own output.
import { readFileSync, writeFileSync } from "node:fs";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

export default function (pi: ExtensionAPI) {
  const schemaPath = process.env.RAMO_REVIEW_MAP_SCHEMA;
  const outputPath = process.env.RAMO_REVIEW_MAP_OUTPUT;
  if (!schemaPath || !outputPath) {
    throw new Error(
      "ramo review-map extension needs RAMO_REVIEW_MAP_SCHEMA and RAMO_REVIEW_MAP_OUTPUT",
    );
  }
  const parameters = JSON.parse(readFileSync(schemaPath, "utf8"));

  pi.registerTool({
    name: "submit_review_map",
    label: "Submit Review Map",
    description:
      "Submit the review map enrichment. Call this exactly once with the complete result. " +
      "This is the only way to answer; do not reply with prose.",
    parameters,
    async execute(_toolCallId, params) {
      writeFileSync(outputPath, JSON.stringify(params));
      return {
        content: [{ type: "text", text: "Review map submitted." }],
        details: {},
        // Nothing follows a submission, so end the agent loop rather than let the model
        // keep talking and burn tokens.
        terminate: true,
      };
    },
  });
}
