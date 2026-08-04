use beryl_backend::{
    OrderedTurnStreamProgress, ProviderDeltaKind, ProviderEnumValue, ProviderItemKind,
    ProviderItemLifecycle, ProviderObservationAbandonReason, ProviderObservationBegin,
    ProviderObservationControl, ProviderObservationSchemaError, ProviderValueContext,
};

use super::fixtures::{delta_fixtures, item_fixtures, status_fixtures};
use super::support::{
    CaseResult, SinkOptions, agent_started, delta_message, drive, drive_fragmented, fragments_for,
    lifecycle,
};
use super::{AGENT_TEXT, assert_schema_error};

#[test]
fn all_provider_item_kinds_retain_each_structurally_valid_lifecycle() {
    assert_eq!(
        item_fixtures().map(|fixture| fixture.kind),
        [
            ProviderItemKind::HookPrompt,
            ProviderItemKind::AgentMessage,
            ProviderItemKind::Plan,
            ProviderItemKind::Reasoning,
            ProviderItemKind::CommandExecution,
            ProviderItemKind::FileChange,
            ProviderItemKind::McpToolCall,
            ProviderItemKind::DynamicToolCall,
            ProviderItemKind::CollabAgentToolCall,
            ProviderItemKind::SubAgentActivity,
            ProviderItemKind::WebSearch,
            ProviderItemKind::ImageView,
            ProviderItemKind::Sleep,
            ProviderItemKind::StandaloneImageGeneration,
            ProviderItemKind::EnteredReviewMode,
            ProviderItemKind::ExitedReviewMode,
            ProviderItemKind::ContextCompaction,
        ]
    );
    for fixture in item_fixtures() {
        let lifecycles = [
            ProviderItemLifecycle::Started,
            ProviderItemLifecycle::Completed,
        ];
        for lifecycle_kind in lifecycles {
            let completed = lifecycle_kind == ProviderItemLifecycle::Completed;
            let item = if completed {
                fixture.completed
            } else {
                fixture.started
            };
            let case = drive(
                lifecycle(item, completed),
                SinkOptions::with_page_capacity(16),
            );
            assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
            assert_eq!(
                case.trace.begins,
                [ProviderObservationBegin::Item {
                    lifecycle: lifecycle_kind,
                    kind: fixture.kind,
                }]
            );
            assert!(case.trace.abandons.is_empty());
            assert_eq!(case.pool.available, 1);
            assert_eq!(case.pool.leased, 0);
        }
    }
}

#[test]
fn status_bearing_items_reject_completed_in_progress_in_any_field_order() {
    for fixture in status_fixtures() {
        let context = ProviderValueContext::Field(fixture.field);
        let started = drive_fragmented(
            lifecycle(&fixture.item("inProgress", true), false),
            SinkOptions::with_page_capacity(7),
            1,
        );
        assert_status_accepted(
            &started,
            ProviderItemLifecycle::Started,
            fixture.kind,
            context,
            ProviderEnumValue::InProgress,
        );

        for terminal in fixture.terminal_statuses {
            let completed = drive_fragmented(
                lifecycle(&fixture.item(terminal, false), true),
                SinkOptions::with_page_capacity(7),
                3,
            );
            assert_status_accepted(
                &completed,
                ProviderItemLifecycle::Completed,
                fixture.kind,
                context,
                terminal_enum(terminal),
            );
        }

        for (status_first, fragment_bytes) in [(true, 2), (false, 7)] {
            let invalid = drive_fragmented(
                lifecycle(&fixture.item("inProgress", status_first), true),
                SinkOptions::with_page_capacity(7),
                fragment_bytes,
            );
            assert_schema_error(
                invalid.outcome.unwrap_err(),
                ProviderObservationSchemaError::InvalidLifecycle,
            );
            assert_eq!(
                invalid.trace.abandons,
                [ProviderObservationAbandonReason::SchemaFailure]
            );
            assert_eq!(invalid.pool.available, 1);
            assert_eq!(invalid.pool.leased, 0);
            assert!(invalid.trace.seal_routes.is_empty());
        }
    }
}

#[test]
fn all_pinned_delta_methods_stream_and_seal() {
    assert_eq!(
        delta_fixtures().map(|(_, kind, _)| kind),
        [
            ProviderDeltaKind::AgentMessage,
            ProviderDeltaKind::Plan,
            ProviderDeltaKind::ReasoningSummaryPartAdded,
            ProviderDeltaKind::ReasoningSummaryText,
            ProviderDeltaKind::ReasoningTextObserved,
            ProviderDeltaKind::CommandExecutionOutput,
            ProviderDeltaKind::FileChangeOutput,
            ProviderDeltaKind::FileChangePatchUpdated,
            ProviderDeltaKind::McpToolCallProgress,
        ]
    );
    for (method, kind, payload) in delta_fixtures() {
        let case = drive(
            delta_message(method, payload),
            SinkOptions::with_page_capacity(8),
        );
        assert_eq!(
            case.outcome
                .unwrap_or_else(|error| panic!("{method} failed: {error:?}")),
            OrderedTurnStreamProgress::Progress
        );
        assert_eq!(
            case.trace.begins,
            [ProviderObservationBegin::Delta { kind }]
        );
        assert!(case.trace.abandons.is_empty());
        assert_eq!(case.pool.available, 1);
        assert_eq!(case.pool.leased, 0);
    }
}

#[test]
fn websocket_fragment_boundaries_preserve_provider_ingress() {
    for fragment_bytes in [1, 2, 3, 4, 7, 16, 31, 63] {
        let case = drive_fragmented(
            agent_started("fragmented"),
            SinkOptions::with_page_capacity(5),
            fragment_bytes,
        );
        assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
        assert_eq!(fragments_for(&case.trace, AGENT_TEXT), b"fragmented");
        assert_eq!(case.pool.available, 1);
        assert_eq!(case.pool.leased, 0);
    }
}

#[test]
fn schema_failures_are_closed_and_return_every_lease() {
    let failures = vec![
        (
            r#"{"type":"agentMessage","id":"item_1"}"#,
            ProviderObservationSchemaError::MissingField,
        ),
        (
            r#"{"type":"agentMessage","id":"item_1","id":"item_2","text":"text"}"#,
            ProviderObservationSchemaError::DuplicateField,
        ),
        (
            r#"{"type":"agentMessage","id":"item_1","text":"text","extra":null}"#,
            ProviderObservationSchemaError::UnknownField,
        ),
        (
            r#"{"type":"agentMessage","id":"item_1","text":0}"#,
            ProviderObservationSchemaError::WrongType,
        ),
        (
            r#"{"type":"unknownKind","id":"item_1"}"#,
            ProviderObservationSchemaError::UnknownOrLateVariant,
        ),
    ];
    for (item, expected) in failures {
        let case = drive(lifecycle(item, false), SinkOptions::with_page_capacity(8));
        assert_schema_error(case.outcome.unwrap_err(), expected);
        assert_eq!(
            case.trace.abandons.len(),
            usize::from(!case.trace.begins.is_empty())
        );
        assert_eq!(case.pool.available, 1);
        assert_eq!(case.pool.leased, 0);
        assert!(case.trace.seal_routes.is_empty());
    }

    let invalid_index = drive(
        delta_message(
            "item/reasoning/summaryTextDelta",
            r#""delta":"text","summaryIndex":-1"#,
        ),
        SinkOptions::with_page_capacity(8),
    );
    assert_schema_error(
        invalid_index.outcome.unwrap_err(),
        ProviderObservationSchemaError::InvalidIndex,
    );
    assert_eq!(
        invalid_index.trace.abandons,
        [ProviderObservationAbandonReason::SchemaFailure]
    );
    assert_eq!(invalid_index.pool.available, 1);
    assert_eq!(invalid_index.pool.leased, 0);
}

fn assert_status_accepted(
    case: &CaseResult,
    lifecycle: ProviderItemLifecycle,
    kind: ProviderItemKind,
    context: ProviderValueContext,
    value: ProviderEnumValue,
) {
    assert_eq!(
        case.outcome.as_ref().unwrap(),
        &OrderedTurnStreamProgress::Progress
    );
    assert_eq!(
        case.trace.begins,
        [ProviderObservationBegin::Item { lifecycle, kind }]
    );
    assert!(
        case.trace
            .controls
            .contains(&ProviderObservationControl::Enum { context, value })
    );
    assert!(case.trace.abandons.is_empty());
    assert_eq!(case.pool.available, 1);
    assert_eq!(case.pool.leased, 0);
}

fn terminal_enum(status: &str) -> ProviderEnumValue {
    match status {
        "completed" => ProviderEnumValue::Completed,
        "failed" => ProviderEnumValue::Failed,
        "declined" => ProviderEnumValue::Declined,
        other => panic!("unexpected terminal fixture status {other}"),
    }
}
