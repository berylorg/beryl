use std::path::PathBuf;

use beryl_backend::{
    AgentMessageItem, ByteRange, HookPromptFragment, HookPromptItem, ImageDetail,
    ItemLifecycleTimestampMs, MemoryCitation, MemoryCitationEntry, PlanItem, ProtocolPhase,
    ReasoningItem, TextElement, ThreadItem, TurnStreamEvent, UserInput, UserMessageItem,
    parse_turn_stream_event,
};
use beryl_model::{CasItemId, CasThreadId, CasTurnId};
use serde_json::{Map, Value, json};

const THREAD_ID: &str = "thread-phase13";
const TURN_ID: &str = "turn-phase13";
const TIMESTAMP_MS: u64 = 1_752_689_600_123;

fn item_id(value: &str) -> CasItemId {
    CasItemId::new(value).expect("fixture item id")
}

fn thread_id() -> CasThreadId {
    CasThreadId::new(THREAD_ID).expect("fixture thread id")
}

fn turn_id() -> CasTurnId {
    CasTurnId::new(TURN_ID).expect("fixture turn id")
}

fn parse_item(value: Value) -> ThreadItem {
    serde_json::from_value(value).expect("pinned item fixture must parse")
}

fn parse_lifecycle(method: &str, item: &Value) -> TurnStreamEvent {
    let mut params = Map::from_iter([
        ("threadId".to_string(), json!(THREAD_ID)),
        ("turnId".to_string(), json!(TURN_ID)),
        ("item".to_string(), item.clone()),
    ]);
    let timestamp_field = match method {
        "item/started" => "startedAtMs",
        "item/completed" => "completedAtMs",
        _ => panic!("unexpected lifecycle method"),
    };
    params.insert(timestamp_field.to_string(), json!(TIMESTAMP_MS));
    parse_turn_stream_event(method, Some(Value::Object(params)))
        .expect("lifecycle notification must parse")
        .expect("lifecycle notification must be recognized")
}

fn assert_paired_item(item: Value, expected: ThreadItem) {
    assert_eq!(parse_item(item.clone()), expected);
    assert_eq!(
        parse_lifecycle("item/started", &item),
        TurnStreamEvent::ItemStarted {
            thread_id: thread_id(),
            turn_id: turn_id(),
            started_at_ms: ItemLifecycleTimestampMs::new(TIMESTAMP_MS),
            item: expected.clone(),
        }
    );
    assert_eq!(
        parse_lifecycle("item/completed", &item),
        TurnStreamEvent::ItemCompleted {
            thread_id: thread_id(),
            turn_id: turn_id(),
            completed_at_ms: ItemLifecycleTimestampMs::new(TIMESTAMP_MS),
            item: expected,
        }
    );
}

#[test]
fn message_item_variants_preserve_every_public_field_exactly() {
    let user = json!({
        "id": "user-rich",
        "type": "userMessage",
        "clientId": "composer-generation-7",
        "content": [
            {
                "type": "text",
                "text": "look at [diagram]",
                "text_elements": [
                    {"byteRange": {"start": 8, "end": 17}, "placeholder": "diagram.png"},
                    {"byteRange": {"start": 0, "end": 4}, "placeholder": null}
                ]
            },
            {"type": "image", "detail": "auto", "url": "data:image/png;base64,AA=="},
            {"type": "image", "detail": "low", "url": "https://example.invalid/low.png"},
            {"type": "localImage", "detail": "high", "path": "/runtime/high.png"},
            {"type": "localImage", "detail": "original", "path": "/runtime/original.png"},
            {"type": "skill", "name": "proof", "path": "/runtime/proof/SKILL.md"},
            {"type": "mention", "name": "connector", "path": "app://connector-1"}
        ]
    });
    assert_paired_item(
        user,
        ThreadItem::UserMessage(UserMessageItem {
            id: item_id("user-rich"),
            client_id: Some("composer-generation-7".to_string()),
            content: vec![
                UserInput::Text {
                    text: "look at [diagram]".to_string(),
                    text_elements: vec![
                        TextElement {
                            byte_range: ByteRange { start: 8, end: 17 },
                            placeholder: Some("diagram.png".to_string()),
                        },
                        TextElement {
                            byte_range: ByteRange { start: 0, end: 4 },
                            placeholder: None,
                        },
                    ],
                },
                UserInput::Image {
                    detail: Some(ImageDetail::Auto),
                    url: "data:image/png;base64,AA==".to_string(),
                },
                UserInput::Image {
                    detail: Some(ImageDetail::Low),
                    url: "https://example.invalid/low.png".to_string(),
                },
                UserInput::LocalImage {
                    detail: Some(ImageDetail::High),
                    path: PathBuf::from("/runtime/high.png"),
                },
                UserInput::LocalImage {
                    detail: Some(ImageDetail::Original),
                    path: PathBuf::from("/runtime/original.png"),
                },
                UserInput::Skill {
                    name: "proof".to_string(),
                    path: PathBuf::from("/runtime/proof/SKILL.md"),
                },
                UserInput::Mention {
                    name: "connector".to_string(),
                    path: "app://connector-1".to_string(),
                },
            ],
        }),
    );

    assert_paired_item(
        json!({
            "id": "hook-rich",
            "type": "hookPrompt",
            "fragments": [
                {"text": "first", "hookRunId": "hook-run-1"},
                {"text": "second", "hookRunId": "hook-run-2"}
            ]
        }),
        ThreadItem::HookPrompt(HookPromptItem {
            id: item_id("hook-rich"),
            fragments: vec![
                HookPromptFragment {
                    text: "first".to_string(),
                    hook_run_id: "hook-run-1".to_string(),
                },
                HookPromptFragment {
                    text: "second".to_string(),
                    hook_run_id: "hook-run-2".to_string(),
                },
            ],
        }),
    );

    assert_paired_item(
        json!({
            "id": "agent-rich",
            "type": "agentMessage",
            "text": "public answer",
            "phase": "final_answer",
            "memoryCitation": {
                "entries": [
                    {"path": "memory/one.md", "lineStart": 3, "lineEnd": 8, "note": "first"},
                    {"path": "memory/two.md", "lineStart": 13, "lineEnd": 21, "note": "second"}
                ],
                "threadIds": ["thread-memory-a", "thread-memory-b"]
            }
        }),
        ThreadItem::AgentMessage(AgentMessageItem {
            id: item_id("agent-rich"),
            text: "public answer".to_string(),
            phase: Some(ProtocolPhase::FinalAnswer),
            memory_citation: Some(MemoryCitation {
                entries: vec![
                    MemoryCitationEntry {
                        path: "memory/one.md".to_string(),
                        line_start: 3,
                        line_end: 8,
                        note: "first".to_string(),
                    },
                    MemoryCitationEntry {
                        path: "memory/two.md".to_string(),
                        line_start: 13,
                        line_end: 21,
                        note: "second".to_string(),
                    },
                ],
                thread_ids: vec!["thread-memory-a".to_string(), "thread-memory-b".to_string()],
            }),
        }),
    );

    assert_paired_item(
        json!({"id": "plan-rich", "type": "plan", "text": "1. inspect\n2. prove"}),
        ThreadItem::Plan(PlanItem {
            id: item_id("plan-rich"),
            text: "1. inspect\n2. prove".to_string(),
        }),
    );

    assert_paired_item(
        json!({
            "id": "reasoning-rich",
            "type": "reasoning",
            "summary": ["public ", "summary"],
            "content": ["private raw reasoning", "another private part"]
        }),
        ThreadItem::Reasoning(ReasoningItem {
            id: item_id("reasoning-rich"),
            summary: vec!["public ".to_string(), "summary".to_string()],
        }),
    );
}

#[test]
fn both_agent_message_phase_branches_are_typed() {
    for (wire, expected) in [
        ("commentary", ProtocolPhase::Commentary),
        ("final_answer", ProtocolPhase::FinalAnswer),
    ] {
        assert_eq!(
            parse_item(json!({
                "id": format!("agent-{wire}"),
                "type": "agentMessage",
                "text": wire,
                "phase": wire
            })),
            ThreadItem::AgentMessage(AgentMessageItem {
                id: item_id(&format!("agent-{wire}")),
                text: wire.to_string(),
                phase: Some(expected),
                memory_citation: None,
            })
        );
    }
}

#[test]
fn raw_reasoning_content_is_intentionally_excluded_from_normalized_equality() {
    let first = parse_item(json!({
        "id": "reasoning-exclusion",
        "type": "reasoning",
        "summary": ["retained"],
        "content": ["first private value"]
    }));
    let second = parse_item(json!({
        "id": "reasoning-exclusion",
        "type": "reasoning",
        "summary": ["retained"],
        "content": ["different private value", "still private"]
    }));

    assert_eq!(
        first,
        ThreadItem::Reasoning(ReasoningItem {
            id: item_id("reasoning-exclusion"),
            summary: vec!["retained".to_string()],
        })
    );
    assert_eq!(first, second);
}

fn parent_object_mut<'a>(value: &'a mut Value, pointer: &str) -> &'a mut Map<String, Value> {
    if pointer.is_empty() {
        value.as_object_mut().expect("fixture root object")
    } else {
        value
            .pointer_mut(pointer)
            .expect("fixture pointer")
            .as_object_mut()
            .expect("fixture nested object")
    }
}

fn assert_missing_and_null_normalize_equally(base: Value, pointer: &str, field: &str) {
    let mut missing = base.clone();
    parent_object_mut(&mut missing, pointer).remove(field);
    let mut explicit_null = base;
    parent_object_mut(&mut explicit_null, pointer).insert(field.to_string(), Value::Null);
    assert_eq!(parse_item(missing), parse_item(explicit_null));
}

#[test]
fn message_optional_fields_preserve_missing_and_null_semantics() {
    assert_missing_and_null_normalize_equally(
        json!({
            "id": "user-optional",
            "type": "userMessage",
            "clientId": "client-1",
            "content": []
        }),
        "",
        "clientId",
    );
    assert_missing_and_null_normalize_equally(
        json!({
            "id": "agent-optional",
            "type": "agentMessage",
            "text": "answer",
            "phase": "commentary",
            "memoryCitation": {"entries": [], "threadIds": []}
        }),
        "",
        "phase",
    );
    assert_missing_and_null_normalize_equally(
        json!({
            "id": "agent-optional",
            "type": "agentMessage",
            "text": "answer",
            "phase": "commentary",
            "memoryCitation": {"entries": [], "threadIds": []}
        }),
        "",
        "memoryCitation",
    );

    let user_inputs = json!({
        "id": "user-input-optionals",
        "type": "userMessage",
        "clientId": null,
        "content": [
            {
                "type": "text",
                "text": "marker",
                "text_elements": [
                    {"byteRange": {"start": 0, "end": 6}, "placeholder": "marker"}
                ]
            },
            {"type": "image", "detail": "low", "url": "https://example.invalid/image.png"},
            {"type": "localImage", "detail": "high", "path": "/runtime/image.png"}
        ]
    });
    assert_missing_and_null_normalize_equally(
        user_inputs.clone(),
        "/content/0/text_elements/0",
        "placeholder",
    );
    assert_missing_and_null_normalize_equally(user_inputs.clone(), "/content/1", "detail");
    assert_missing_and_null_normalize_equally(user_inputs, "/content/2", "detail");
}

#[test]
fn message_defaulted_collections_are_exactly_empty_when_missing() {
    let text_without_elements = parse_item(json!({
        "id": "user-default-elements",
        "type": "userMessage",
        "content": [{"type": "text", "text": "plain"}]
    }));
    assert_eq!(
        text_without_elements,
        ThreadItem::UserMessage(UserMessageItem {
            id: item_id("user-default-elements"),
            client_id: None,
            content: vec![UserInput::Text {
                text: "plain".to_string(),
                text_elements: Vec::new(),
            }],
        })
    );

    assert_eq!(
        parse_item(json!({"id": "reasoning-defaults", "type": "reasoning"})),
        ThreadItem::Reasoning(ReasoningItem {
            id: item_id("reasoning-defaults"),
            summary: Vec::new(),
        })
    );
}
