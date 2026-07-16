use beryl_app::conversation_tools::ConversationToolRegistry;
use beryl_model::CasConversationToolProfileVersion;
use serde_json::json;

#[test]
fn canonical_registry_has_review_visible_wire_shape_and_profile() {
    let registry = ConversationToolRegistry::canonical();

    assert_eq!(
        serde_json::to_value(registry.specs()).unwrap(),
        json!([{
            "type": "namespace",
            "name": "beryl",
            "description": "Beryl-owned conversation tools.",
            "tools": [
                {
                    "type": "function",
                    "name": "yield",
                    "description": "Yield control to Beryl with one semantic lifecycle outcome after the current turn reaches a natural boundary. Beryl owns all stop, notification, compaction, and resume policy.",
                    "inputSchema": {
                        "type": "object",
                        "required": ["outcome"],
                        "properties": {
                            "outcome": {
                                "type": "string",
                                "enum": [
                                    "phase_needs_review",
                                    "blocked_needs_operator",
                                    "phase_continue",
                                    "plan_complete"
                                ]
                            }
                        },
                        "additionalProperties": false
                    },
                    "deferLoading": false
                },
                {
                    "type": "function",
                    "name": "resolve_branch_discussion",
                    "description": "Admit one resolution for the exact active branch discussion and schedule its durable handoff to the bound parent thread.",
                    "inputSchema": {
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
                    },
                    "deferLoading": false
                }
            ]
        }])
    );

    assert_eq!(
        registry.profile().version(),
        CasConversationToolProfileVersion::V1
    );
    assert_eq!(
        registry.profile().digest(),
        [
            0xef, 0xcd, 0x40, 0x92, 0xab, 0x77, 0xf6, 0x76, 0x3f, 0x12, 0xf5, 0x06, 0x7e, 0x44,
            0x1a, 0x87, 0xb4, 0xd6, 0x81, 0xd5, 0x3e, 0xb4, 0xa4, 0x0d, 0xfa, 0xcd, 0xbf, 0xfb,
            0x8b, 0xa7, 0x48, 0x71,
        ]
    );
}
