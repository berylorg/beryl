#[path = "provider_observation_ingress/capture_edges.rs"]
mod capture_edges;
#[path = "provider_observation_ingress/fixtures.rs"]
mod fixtures;
#[path = "provider_observation_ingress/identities.rs"]
mod identities;
#[path = "provider_observation_ingress/matrix.rs"]
mod matrix;
#[path = "provider_observation_ingress/privacy.rs"]
mod privacy;
#[path = "provider_observation_ingress/support.rs"]
mod support;
#[path = "provider_observation_ingress/web_other.rs"]
mod web_other;

use beryl_backend::{
    ForegroundIngressError, ManagedBackendError, OrderedTurnStreamProgress,
    OrderedTurnStreamRejection, OrderedTurnStreamSubmitCause, ProviderField, ProviderItemKind,
    ProviderItemLifecycle, ProviderObservationAbandonReason, ProviderObservationBegin,
    ProviderObservationControl, ProviderObservationError, ProviderObservationSchemaError,
    ProviderScalar, ProviderValueContext,
};
use support::{
    SinkOptions, agent_started, drive, drive_truncated, fragments_for, lifecycle, mcp_item,
};

const ITEM_ID: ProviderValueContext = ProviderValueContext::Field(ProviderField::ItemId);
const AGENT_TEXT: ProviderValueContext =
    ProviderValueContext::Field(ProviderField::AgentMessageText);

#[test]
fn lifecycle_streams_exact_fragments_and_seals_inside_the_sink() {
    let text = "a page-bounded provider message with several fragments";
    let case = drive(agent_started(text), SinkOptions::with_page_capacity(7));
    assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
    assert_eq!(
        case.trace.begins,
        [ProviderObservationBegin::Item {
            lifecycle: ProviderItemLifecycle::Started,
            kind: ProviderItemKind::AgentMessage,
        }]
    );
    assert_eq!(fragments_for(&case.trace, ITEM_ID), b"item_1");
    assert_eq!(fragments_for(&case.trace, AGENT_TEXT), text.as_bytes());
    assert!(
        case.trace
            .controls
            .contains(&ProviderObservationControl::BeginField(AGENT_TEXT))
    );
    assert!(
        case.trace
            .controls
            .contains(&ProviderObservationControl::EndField(AGENT_TEXT))
    );
    assert_eq!(case.trace.seal_routes.len(), 1);
    assert_eq!(case.trace.seal_routes[0].thread_id().as_str(), "thread_1");
    assert_eq!(case.trace.seal_routes[0].turn_id().as_str(), "turn_1");
    assert!(case.trace.abandons.is_empty());
    assert_eq!(case.trace.leased_at_seal, [0]);
    assert_eq!(case.pool.available, 1);
    assert_eq!(case.pool.leased, 0);
}

#[test]
fn arbitrary_length_text_stays_page_bounded() {
    let text = "provider-text-".repeat(20_000);
    let case = drive(agent_started(&text), SinkOptions::with_page_capacity(64));
    assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
    assert_eq!(fragments_for(&case.trace, AGENT_TEXT), text.as_bytes());
    assert!(
        case.trace
            .fragments
            .iter()
            .all(|(_, fragment)| fragment.len() <= 64)
    );
    assert_eq!(case.pool.high_water, 1);
    assert_eq!(case.pool.available, 1);
}

#[test]
fn undersized_output_lease_is_rejected_and_abandoned_once() {
    let case = drive(agent_started("text"), SinkOptions::with_page_capacity(3));
    assert_sink_error(
        case.outcome.unwrap_err(),
        OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl),
    );
    assert_eq!(
        case.trace.abandons,
        [ProviderObservationAbandonReason::SinkRejected]
    );
    assert!(case.trace.seal_routes.is_empty());
    assert_eq!(case.trace.leased_at_abandon, [0]);
    assert_eq!(case.pool.available, 1);
    assert_eq!(case.pool.leased, 0);
}

#[test]
fn terminal_fragment_timeout_returns_exact_ownership_and_abandons_once() {
    let case = drive(
        agent_started("abcdefgh"),
        SinkOptions {
            page_capacity: 4,
            fail_context: Some(AGENT_TEXT),
            fragment_failure: Some(OrderedTurnStreamSubmitCause::Timeout),
        },
    );
    assert_sink_error(
        case.outcome.unwrap_err(),
        OrderedTurnStreamSubmitCause::Timeout,
    );
    assert_eq!(fragments_for(&case.trace, AGENT_TEXT), b"abcd");
    assert_eq!(
        case.trace.abandons,
        [ProviderObservationAbandonReason::Timeout]
    );
    assert_eq!(case.trace.leased_at_abandon, [0]);
    assert_eq!(case.pool.available, 1);
    assert_eq!(case.pool.leased, 0);
}

#[test]
fn terminal_fragment_capacity_full_and_cancellation_release_exact_ownership() {
    for (cause, abandon) in [
        (
            OrderedTurnStreamSubmitCause::CapacityFull,
            ProviderObservationAbandonReason::CapacityFull,
        ),
        (
            OrderedTurnStreamSubmitCause::Cancelled,
            ProviderObservationAbandonReason::Cancelled,
        ),
    ] {
        let case = drive(
            agent_started("abcdefgh"),
            SinkOptions {
                page_capacity: 4,
                fail_context: Some(AGENT_TEXT),
                fragment_failure: Some(cause),
            },
        );
        assert_sink_error(case.outcome.unwrap_err(), cause);
        assert_eq!(fragments_for(&case.trace, AGENT_TEXT), b"abcd");
        assert_eq!(case.trace.abandons, [abandon]);
        assert!(case.trace.seal_routes.is_empty());
        assert_eq!(case.trace.leased_at_abandon, [0]);
        assert_eq!(case.pool.high_water, 1);
        assert_eq!(case.pool.available, 1);
        assert_eq!(case.pool.leased, 0);
    }
}

#[test]
fn decoded_bytes_from_a_failing_parser_call_are_handed_off_before_error() {
    let message = String::from(
        r#"{"method":"item/started","params":{"item":{"type":"agentMessage","id":"item_1","text":"abc\q"},"threadId":"thread_1","turnId":"turn_1","startedAtMs":123}}"#,
    );
    let case = drive(message, SinkOptions::with_page_capacity(7));
    assert_schema_error(
        case.outcome.unwrap_err(),
        ProviderObservationSchemaError::InvalidString,
    );
    assert_eq!(fragments_for(&case.trace, AGENT_TEXT), b"abc");
    assert_eq!(
        case.trace.abandons,
        [ProviderObservationAbandonReason::SchemaFailure]
    );
    assert_eq!(case.trace.leased_at_abandon, [0]);
    assert_eq!(case.pool.available, 1);
}

#[test]
fn transport_loss_after_provider_begin_abandons_with_transport_taxonomy() {
    let message = agent_started(&"transport-loss".repeat(16));
    let text = message.find("transport-loss").unwrap();
    let case = drive_truncated(
        message,
        SinkOptions::with_page_capacity(7),
        text + "transport-loss".len(),
    );
    assert!(case.outcome.is_err());
    assert_eq!(case.trace.begins.len(), 1);
    assert_eq!(
        case.trace.abandons,
        [ProviderObservationAbandonReason::TransportLost]
    );
    assert_eq!(case.trace.leased_at_abandon, [0]);
    assert_eq!(case.pool.available, 1);
}

#[test]
fn dynamic_input_image_is_rejected_at_the_discriminant_before_payload() {
    let item = r#"{"type":"dynamicToolCall","id":"item_1","tool":"tool","arguments":{},"status":"completed","contentItems":[{"type":"inputImage","image_url":"PAYLOAD_MUST_NOT_ESCAPE"}]}"#;
    let case = drive(lifecycle(item, true), SinkOptions::with_page_capacity(8));
    assert_schema_error(
        case.outcome.unwrap_err(),
        ProviderObservationSchemaError::InlineImageRequiresAsset,
    );
    assert!(
        case.trace
            .fragments
            .iter()
            .all(|(_, bytes)| !bytes.windows(7).any(|part| part == b"PAYLOAD"))
    );
    assert_eq!(
        case.trace.abandons,
        [ProviderObservationAbandonReason::SchemaFailure]
    );
}

#[test]
fn mcp_content_rejects_dangerous_keys_before_a_safe_type() {
    let result = r#"{"content":[{"data":"secret","type":"text"}]}"#;
    let item = mcp_item("{}", Some(result));
    let case = drive(lifecycle(&item, true), SinkOptions::with_page_capacity(8));
    assert_schema_error(
        case.outcome.unwrap_err(),
        ProviderObservationSchemaError::InlineImageRequiresAsset,
    );
    assert!(
        case.trace
            .fragments
            .iter()
            .all(|(_, bytes)| !bytes.windows(6).any(|part| part == b"secret"))
    );
}

#[test]
fn mcp_content_rejects_image_type_and_accepts_data_after_safe_type() {
    let image = r#"{"content":[{"type":"image","data":"secret"}]}"#;
    let image_case = drive(
        lifecycle(&mcp_item("{}", Some(image)), true),
        SinkOptions::with_page_capacity(8),
    );
    assert_schema_error(
        image_case.outcome.unwrap_err(),
        ProviderObservationSchemaError::InlineImageRequiresAsset,
    );

    let safe = r#"{"content":[{"type":"text","data":"data:image/png;base64,ordinary"}]}"#;
    let safe_case = drive(
        lifecycle(&mcp_item("{}", Some(safe)), true),
        SinkOptions::with_page_capacity(9),
    );
    assert_eq!(
        safe_case.outcome.unwrap(),
        OrderedTurnStreamProgress::Progress
    );
    let bytes: Vec<u8> = safe_case
        .trace
        .fragments
        .iter()
        .flat_map(|(_, bytes)| bytes.iter().copied())
        .collect();
    assert!(
        bytes
            .windows(b"data:image/png;base64,ordinary".len())
            .any(|part| part == b"data:image/png;base64,ordinary")
    );
}

#[test]
fn structured_depth_accepts_128_levels_and_rejects_129() {
    let at_limit = nested_arrays(128);
    let accepted = drive(
        lifecycle(&mcp_item(&at_limit, None), true),
        SinkOptions::with_page_capacity(16),
    );
    assert_eq!(
        accepted.outcome.unwrap(),
        OrderedTurnStreamProgress::Progress
    );
    assert!(accepted.trace.abandons.is_empty());

    let over_limit = nested_arrays(129);
    let rejected = drive(
        lifecycle(&mcp_item(&over_limit, None), true),
        SinkOptions::with_page_capacity(16),
    );
    assert_schema_error(
        rejected.outcome.unwrap_err(),
        ProviderObservationSchemaError::StructuredDepthExceeded,
    );
    assert_eq!(
        rejected.trace.abandons,
        [ProviderObservationAbandonReason::SchemaFailure]
    );
}

#[test]
fn structured_numbers_preserve_integer_fraction_and_exponent_classes() {
    let arguments = "[0,1.25,-2.5e3]";
    let case = drive(
        lifecycle(&mcp_item(arguments, None), true),
        SinkOptions::with_page_capacity(16),
    );
    assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
    let scalars: Vec<ProviderScalar> = case
        .trace
        .controls
        .iter()
        .filter_map(|control| match control {
            ProviderObservationControl::Scalar { value, .. } => Some(*value),
            _ => None,
        })
        .collect();
    assert!(scalars.contains(&ProviderScalar::Unsigned(0)));
    assert!(
        scalars.iter().any(
            |value| matches!(value, ProviderScalar::FiniteFloat(value) if value.get() == 1.25)
        )
    );
    assert!(scalars.iter().any(
        |value| matches!(value, ProviderScalar::FiniteFloat(value) if value.get() == -2500.0)
    ));
}

#[test]
fn target_method_after_the_classification_prefix_enters_content_free_quarantine() {
    let message = format!(
        "{{{}\"jsonrpc\":\"2.0\",\"method\":\"item/started\",\"params\":{{\"item\":{{\"type\":\"agentMessage\",\"id\":\"item_1\",\"text\":\"late\"}},\"threadId\":\"thread_1\",\"turnId\":\"turn_1\",\"startedAtMs\":123}}}}",
        " ".repeat(2048)
    );
    let case = drive(message, SinkOptions::with_page_capacity(8));
    assert!(matches!(
        case.outcome.unwrap_err(),
        ManagedBackendError::ForegroundIngress {
            source: ForegroundIngressError::Quarantined,
            ..
        }
    ));
    assert!(case.trace.begins.is_empty());
    assert!(case.trace.abandons.is_empty());
}

#[test]
fn pinned_method_params_notification_without_jsonrpc_is_accepted() {
    let message = String::from(
        r#"{"method":"item/started","params":{"item":{"type":"agentMessage","id":"item_1","text":"pinned"},"threadId":"thread_1","turnId":"turn_1","startedAtMs":123}}"#,
    );
    let case = drive(message, SinkOptions::with_page_capacity(8));
    assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
    assert!(case.trace.abandons.is_empty());
}

#[test]
fn missing_trailing_route_abandons_unpublished_content() {
    let message = String::from(
        r#"{"method":"item/started","params":{"item":{"type":"agentMessage","id":"item_1","text":"unpublished"},"threadId":"thread_1","startedAtMs":123}}"#,
    );
    let case = drive(message, SinkOptions::with_page_capacity(8));
    assert_schema_error(
        case.outcome.unwrap_err(),
        ProviderObservationSchemaError::MissingOrMalformedRoute,
    );
    assert_eq!(fragments_for(&case.trace, AGENT_TEXT), b"unpublished");
    assert_eq!(
        case.trace.abandons,
        [ProviderObservationAbandonReason::MissingOrMalformedRoute]
    );
    assert_eq!(case.trace.leased_at_abandon, [0]);
    assert!(case.trace.seal_routes.is_empty());
}

fn nested_arrays(depth: usize) -> String {
    format!("{}null{}", "[".repeat(depth), "]".repeat(depth))
}

fn assert_schema_error(error: ManagedBackendError, expected: ProviderObservationSchemaError) {
    match error {
        ManagedBackendError::ProviderObservation {
            source: ProviderObservationError::Schema(actual),
            ..
        } => assert_eq!(actual, expected),
        other => panic!("expected provider schema error {expected:?}, got {other:?}"),
    }
}

fn assert_sink_error(error: ManagedBackendError, expected: OrderedTurnStreamSubmitCause) {
    match error {
        ManagedBackendError::ProviderObservation {
            source: ProviderObservationError::Submit(actual),
            ..
        } => assert_eq!(actual, expected),
        other => panic!("expected provider sink error {expected:?}, got {other:?}"),
    }
}
