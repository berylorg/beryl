use beryl_backend::{
    ApprovalRequestKind, DynamicToolCallOutputContentItem, DynamicToolCallResponse,
    ThreadUnsubscribeResponse, ThreadUnsubscribeStatus, TurnStartOptions,
};
use serde_json::json;

#[test]
fn approval_kinds_expose_exact_interruption_policy_without_payload() {
    assert!(ApprovalRequestKind::CommandExecution.denial_response_interrupts_turn());
    assert!(ApprovalRequestKind::FileChange.denial_response_interrupts_turn());
    assert!(ApprovalRequestKind::Permissions.separate_interruption_required());
}

#[test]
fn dynamic_tool_call_response_serializes_text_content() {
    let response = DynamicToolCallResponse::success_text("{\"ok\":true}");
    let serialized = serde_json::to_value(response).unwrap();

    assert_eq!(
        serialized,
        json!({
            "success": true,
            "contentItems": [
                {
                    "type": "inputText",
                    "text": "{\"ok\":true}"
                }
            ]
        })
    );
}

#[test]
fn dynamic_tool_call_response_serializes_image_content() {
    let response =
        DynamicToolCallResponse::failure(vec![DynamicToolCallOutputContentItem::image_url(
            "file:///tmp/preview.png",
        )]);
    let serialized = serde_json::to_value(response).unwrap();

    assert_eq!(
        serialized,
        json!({
            "success": false,
            "contentItems": [
                {
                    "type": "inputImage",
                    "imageUrl": "file:///tmp/preview.png"
                }
            ]
        })
    );
}

#[test]
fn thread_unsubscribe_response_deserializes_status() {
    let response: ThreadUnsubscribeResponse = serde_json::from_value(json!({
        "status": "unsubscribed"
    }))
    .unwrap();

    assert_eq!(response.status, ThreadUnsubscribeStatus::Unsubscribed);
}

#[test]
fn turn_start_options_preserve_model_and_reasoning_overrides() {
    let options = TurnStartOptions::default()
        .with_model("gpt-5.5")
        .with_reasoning_effort("high");

    assert_eq!(options.model(), Some("gpt-5.5"));
    assert_eq!(options.reasoning_effort(), Some("high"));
}

#[test]
fn turn_start_options_ignore_empty_overrides() {
    let options = TurnStartOptions::default()
        .with_model("")
        .with_reasoning_effort("");

    assert_eq!(options.model(), None);
    assert_eq!(options.reasoning_effort(), None);
}
