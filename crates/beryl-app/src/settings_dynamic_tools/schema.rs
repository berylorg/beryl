use serde_json::{Value, json};

pub(super) fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

pub(super) fn settings_update_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operations": {
                "type": "object",
                "properties": {
                    "contextCompactionTimeoutSeconds": {
                        "oneOf": [
                            {
                                "type": "integer",
                                "minimum": 1,
                                "maximum": 86400
                            },
                            {
                                "type": "string",
                                "maxLength": 32
                            }
                        ]
                    }
                },
                "additionalProperties": false
            },
            "notifications": {
                "type": "object",
                "properties": {
                    "endTurnSoundPath": {
                        "oneOf": [
                            { "type": "string" },
                            { "type": "null" }
                        ],
                        "description": "Absolute host path to a .wav end-turn sound, or null to clear it."
                    }
                },
                "additionalProperties": false
            },
            "agent": {
                "type": "object",
                "properties": {
                    "developerInstructions": {
                        "oneOf": [
                            { "type": "string" },
                            { "type": "null" }
                        ],
                        "description": "Replacement developer-instructions text, or null to clear it."
                    }
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    })
}
