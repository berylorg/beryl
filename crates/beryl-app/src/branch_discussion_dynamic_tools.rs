use beryl_backend::DynamicToolFunctionSpec;
use serde_json::json;

/// Tool name used for conversational branch-discussion resolution.
pub const RESOLVE_BRANCH_DISCUSSION_TOOL: &str = "resolve_branch_discussion";

/// Returns the feature-owned branch-discussion tool definitions.
pub fn branch_discussion_dynamic_tool_specs() -> Vec<DynamicToolFunctionSpec> {
    vec![DynamicToolFunctionSpec::new(
        RESOLVE_BRANCH_DISCUSSION_TOOL,
        "Admit one resolution for the exact active branch discussion and schedule its durable handoff to the bound parent thread.",
        json!({
            "type": "object",
            "required": ["resolution"],
            "properties": {
                "resolution": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 65536,
                    "description": "The complete resolution to hand back to the discussion's bound parent thread."
                }
            },
            "additionalProperties": false
        }),
    )
    .with_defer_loading(false)]
}
