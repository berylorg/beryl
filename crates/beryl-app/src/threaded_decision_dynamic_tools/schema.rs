use super::*;

pub(super) fn start_decision_branch_schema() -> Value {
    json!({
        "type": "object",
        "required": ["checklistItemIds"],
        "properties": {
            "checklistItemIds": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_DECISION_BRANCH_TOOL_ITEMS,
                "items": {
                    "type": "string",
                    "pattern": "^[a-z0-9_-]+$"
                },
                "description": "Explicit checklist-item semantic node ids to turn into queued decision branches."
            }
        },
        "additionalProperties": false,
        "examples": [{
            "checklistItemIds": ["choose_parser", "pick_storage_model"]
        }]
    })
}

pub(super) fn start_topic_decision_schema() -> Value {
    json!({
        "type": "object",
        "required": ["topicNodeId", "title"],
        "properties": {
            "topicNodeId": {
                "type": "string",
                "pattern": "^[a-z0-9_-]+$",
                "description": "Exact topic-capable semantic node id under which Beryl should create or reuse a decision checklist item."
            },
            "title": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_TOPIC_DECISION_ITEM_TITLE_CHARS,
                "description": "Concise title for the decision checklist item."
            },
            "summary": {
                "type": "string",
                "maxLength": MAX_TOPIC_DECISION_ITEM_SUMMARY_CHARS,
                "description": "Optional concise summary for the decision item. Omit or leave empty to let Beryl use a topic-derived summary."
            }
        },
        "additionalProperties": false,
        "examples": [{
            "topicNodeId": "architecture",
            "title": "Choose queue backend",
            "summary": "Decide which queue backend should power turn orchestration."
        }]
    })
}

pub(super) fn resolve_decision_branch_schema() -> Value {
    json!({
        "type": "object",
        "required": ["outcome", "summary", "handoffMessage"],
        "properties": {
            "outcome": {
                "type": "string",
                "enum": ["accepted", "rejected"],
                "description": "Resolution outcome for the active decision branch."
            },
            "summary": {
                "type": "string",
                "minLength": 1,
                "description": "Concise resolution summary to store with decision provenance."
            },
            "handoffMessage": {
                "type": "string",
                "minLength": 1,
                "description": "Message Beryl will send as a real user turn in the parent thread."
            }
        },
        "additionalProperties": false,
        "examples": [{
            "outcome": "accepted",
            "summary": "Use the database-backed queue.",
            "handoffMessage": "The child branch explored the options and recommends the database-backed queue because it preserves replay state across restarts."
        }]
    })
}
