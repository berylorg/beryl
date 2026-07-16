use std::path::PathBuf;

use beryl_backend::{
    ContextCompactionItem, EnteredReviewModeItem, ExitedReviewModeItem, ImageGenerationItem,
    ImageViewItem, ItemLifecycleTimestampMs, SleepItem, SubAgentActivityItem, SubAgentActivityKind,
    ThreadItem, ThreadItemLifecycleContract, TurnStreamEvent, WebSearchAction, WebSearchItem,
    parse_turn_stream_event,
};
use beryl_model::{CasItemId, CasThreadId, CasTurnId};
use serde_json::{Value, json};

const THREAD_ID: &str = "thread-phase13";
const TURN_ID: &str = "turn-phase13";
const TIMESTAMP_MS: u64 = 1_752_689_600_123;

fn item_id(value: &str) -> CasItemId {
    CasItemId::new(value).expect("fixture item id")
}

fn thread_id(value: &str) -> CasThreadId {
    CasThreadId::new(value).expect("fixture thread id")
}

fn turn_id() -> CasTurnId {
    CasTurnId::new(TURN_ID).expect("fixture turn id")
}

fn parse_item(value: Value) -> ThreadItem {
    serde_json::from_value(value).expect("pinned item fixture must parse")
}

fn lifecycle_params(item: &Value, timestamp_field: &str) -> Value {
    let mut params = json!({
        "threadId": THREAD_ID,
        "turnId": TURN_ID,
        "item": item.clone()
    });
    params[timestamp_field] = json!(TIMESTAMP_MS);
    params
}

fn parse_lifecycle(method: &str, item: &Value) -> Result<TurnStreamEvent, serde_json::Error> {
    let timestamp_field = match method {
        "item/started" => "startedAtMs",
        "item/completed" => "completedAtMs",
        _ => panic!("unexpected lifecycle method"),
    };
    parse_turn_stream_event(method, Some(lifecycle_params(item, timestamp_field)))
        .map(|event| event.expect("lifecycle notification must be recognized"))
}

fn assert_paired_item(item: Value, expected: ThreadItem) {
    assert_eq!(parse_item(item.clone()), expected);
    assert_eq!(
        parse_lifecycle("item/started", &item).unwrap(),
        TurnStreamEvent::ItemStarted {
            thread_id: thread_id(THREAD_ID),
            turn_id: turn_id(),
            started_at_ms: ItemLifecycleTimestampMs::new(TIMESTAMP_MS),
            item: expected.clone(),
        }
    );
    assert_eq!(
        parse_lifecycle("item/completed", &item).unwrap(),
        TurnStreamEvent::ItemCompleted {
            thread_id: thread_id(THREAD_ID),
            turn_id: turn_id(),
            completed_at_ms: ItemLifecycleTimestampMs::new(TIMESTAMP_MS),
            item: expected,
        }
    );
}

#[test]
fn auxiliary_item_variants_preserve_every_public_field_exactly() {
    assert_paired_item(
        json!({
            "id": "web-rich",
            "type": "webSearch",
            "query": "primary query",
            "action": {
                "type": "search",
                "query": "single query",
                "queries": ["first query", "second query"]
            }
        }),
        ThreadItem::WebSearch(WebSearchItem {
            id: item_id("web-rich"),
            query: "primary query".to_string(),
            action: Some(WebSearchAction::Search {
                query: Some("single query".to_string()),
                queries: Some(vec!["first query".to_string(), "second query".to_string()]),
            }),
        }),
    );

    assert_paired_item(
        json!({"id": "view-rich", "type": "imageView", "path": "/runtime/view.png"}),
        ThreadItem::ImageView(ImageViewItem {
            id: item_id("view-rich"),
            path: "/runtime/view.png".to_string(),
        }),
    );

    assert_paired_item(
        json!({"id": "sleep-rich", "type": "sleep", "durationMs": 18_446_744_073_709_551_615_u64}),
        ThreadItem::Sleep(SleepItem {
            id: item_id("sleep-rich"),
            duration_ms: u64::MAX,
        }),
    );

    assert_paired_item(
        json!({
            "id": "image-rich",
            "type": "imageGeneration",
            "status": "completed-with-provider-detail",
            "revisedPrompt": "a revised public prompt",
            "result": "data:image/png;base64,AQID",
            "savedPath": "/runtime/generated/image.png"
        }),
        ThreadItem::ImageGeneration(ImageGenerationItem {
            id: item_id("image-rich"),
            status: "completed-with-provider-detail".to_string(),
            revised_prompt: Some("a revised public prompt".to_string()),
            saved_path: Some(PathBuf::from("/runtime/generated/image.png")),
        }),
    );

    assert_paired_item(
        json!({
            "id": "review-enter-rich",
            "type": "enteredReviewMode",
            "review": "Review the exact patch boundaries."
        }),
        ThreadItem::EnteredReviewMode(EnteredReviewModeItem {
            id: item_id("review-enter-rich"),
            review: "Review the exact patch boundaries.".to_string(),
        }),
    );

    assert_paired_item(
        json!({
            "id": "review-exit-rich",
            "type": "exitedReviewMode",
            "review": "Review completed with two findings."
        }),
        ThreadItem::ExitedReviewMode(ExitedReviewModeItem {
            id: item_id("review-exit-rich"),
            review: "Review completed with two findings.".to_string(),
        }),
    );

    assert_paired_item(
        json!({"id": "compaction-rich", "type": "contextCompaction"}),
        ThreadItem::ContextCompaction(ContextCompactionItem {
            id: item_id("compaction-rich"),
        }),
    );
}

#[test]
fn subagent_activity_is_exactly_completion_only_for_every_kind() {
    for (wire, expected_kind) in [
        ("started", SubAgentActivityKind::Started),
        ("interacted", SubAgentActivityKind::Interacted),
        ("interrupted", SubAgentActivityKind::Interrupted),
    ] {
        let item = json!({
            "id": format!("subagent-{wire}"),
            "type": "subAgentActivity",
            "kind": wire,
            "agentThreadId": "thread-child",
            "agentPath": format!("agents/{wire}/thread-child")
        });
        let expected = ThreadItem::SubAgentActivity(SubAgentActivityItem {
            id: item_id(&format!("subagent-{wire}")),
            kind: expected_kind,
            agent_thread_id: thread_id("thread-child"),
            agent_path: format!("agents/{wire}/thread-child"),
        });

        assert_eq!(parse_item(item.clone()), expected);
        assert_eq!(
            expected.lifecycle_contract(),
            ThreadItemLifecycleContract::CompletionOnly
        );
        assert!(parse_lifecycle("item/started", &item).is_err());
        assert_eq!(
            parse_lifecycle("item/completed", &item).unwrap(),
            TurnStreamEvent::ItemCompleted {
                thread_id: thread_id(THREAD_ID),
                turn_id: turn_id(),
                completed_at_ms: ItemLifecycleTimestampMs::new(TIMESTAMP_MS),
                item: expected,
            }
        );
    }
}

#[test]
fn every_web_search_action_branch_is_typed_without_convenience_loss() {
    let cases = [
        (
            json!({"type": "search", "query": null, "queries": ["one", "two"]}),
            WebSearchAction::Search {
                query: None,
                queries: Some(vec!["one".to_string(), "two".to_string()]),
            },
        ),
        (
            json!({"type": "openPage", "url": "https://example.invalid/page"}),
            WebSearchAction::OpenPage {
                url: Some("https://example.invalid/page".to_string()),
            },
        ),
        (
            json!({
                "type": "findInPage",
                "url": "https://example.invalid/page",
                "pattern": "proof"
            }),
            WebSearchAction::FindInPage {
                url: Some("https://example.invalid/page".to_string()),
                pattern: Some("proof".to_string()),
            },
        ),
        (
            json!({"type": "providerOtherAction", "ignoredByPinnedOther": "value"}),
            WebSearchAction::Other,
        ),
    ];

    for (index, (action, expected_action)) in cases.into_iter().enumerate() {
        assert_eq!(
            parse_item(json!({
                "id": format!("web-action-{index}"),
                "type": "webSearch",
                "query": "root query",
                "action": action
            })),
            ThreadItem::WebSearch(WebSearchItem {
                id: item_id(&format!("web-action-{index}")),
                query: "root query".to_string(),
                action: Some(expected_action),
            })
        );
    }
}
