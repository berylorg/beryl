use beryl_backend::{
    OrderedTurnStreamProgress, ProviderContainer, ProviderField, ProviderItemKind,
    ProviderItemLifecycle, ProviderObservationAbandonReason, ProviderObservationBegin,
    ProviderObservationControl, ProviderObservationSchemaError, ProviderValueContext,
};

use super::assert_schema_error;
use super::support::{CaseResult, SinkOptions, drive, drive_fragmented, lifecycle};

const ACTION: ProviderValueContext = ProviderValueContext::Field(ProviderField::WebSearchAction);
const ACTION_KIND: ProviderValueContext =
    ProviderValueContext::Field(ProviderField::WebSearchActionKind);

#[test]
fn observed_web_other_action_emits_closed_evidence_and_seals() {
    let action = r#"{"type":"providerOtherAction","ignoredByPinnedOther":"must-not-escape"}"#;
    let case = drive(web_search(action), SinkOptions::with_page_capacity(7));
    assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
    assert_eq!(
        case.trace.begins,
        [ProviderObservationBegin::Item {
            lifecycle: ProviderItemLifecycle::Completed,
            kind: ProviderItemKind::WebSearch,
        }]
    );
    assert_other_controls(&case.trace.controls);
    assert_payload_absent(&case.trace.fragments, b"must-not-escape");
    assert!(case.trace.abandons.is_empty());
    assert_eq!(case.trace.seal_routes.len(), 1);
    assert_eq!(case.pool.high_water, 1);
    assert_eq!(case.pool.available, 1);
    assert_eq!(case.pool.leased, 0);
}

#[test]
fn web_other_action_is_invariant_across_websocket_splits() {
    let action = r#"{"type":"providerOtherAction","ignoredByPinnedOther":{"nested":[1,true,null,"discard"]}}"#;
    for fragment_bytes in [1, 2, 3, 4, 7, 31, 63] {
        let case = drive_fragmented(
            web_search(action),
            SinkOptions::with_page_capacity(5),
            fragment_bytes,
        );
        assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
        assert_other_controls(&case.trace.controls);
        assert_payload_absent(&case.trace.fragments, b"discard");
        assert_eq!(case.pool.available, 1);
        assert_eq!(case.pool.leased, 0);
    }
}

#[test]
fn arbitrarily_large_web_other_payload_is_discarded_with_a_fixed_sink_working_set() {
    let discriminator = "unknown-action-".repeat(20_000);
    let field_name = "unknown-field-".repeat(20_000);
    let payload = "OTHER-PAYLOAD-SENTINEL-".repeat(20_000);
    let action = format!(
        "{{\"type\":\"{discriminator}\",\"{field_name}\":{{\"nested\":[0,1.25,-2e3,true,null,{{\"leaf\":\"{payload}\"}}]}}}}"
    );
    let case = drive(web_search(&action), SinkOptions::with_page_capacity(11));
    assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
    assert_other_controls(&case.trace.controls);
    assert_eq!(case.trace.controls.len(), 8);
    assert_payload_absent(&case.trace.fragments, b"OTHER-PAYLOAD-SENTINEL");
    assert_eq!(case.pool.high_water, 1);
    assert_eq!(case.pool.available, 1);
    assert_eq!(case.pool.leased, 0);
}

#[test]
fn web_other_remains_type_first_unique_depth_bounded_and_structurally_valid() {
    let accepted = format!(
        "{{\"type\":\"unknown\",\"payload\":{}}}",
        nested_arrays(128)
    );
    let accepted = drive(web_search(&accepted), SinkOptions::with_page_capacity(8));
    assert_eq!(
        accepted.outcome.unwrap(),
        OrderedTurnStreamProgress::Progress
    );
    assert_eq!(accepted.pool.leased, 0);

    let too_deep = format!(
        "{{\"type\":\"unknown\",\"payload\":{}}}",
        nested_arrays(129)
    );
    assert_closed_schema(
        drive(web_search(&too_deep), SinkOptions::with_page_capacity(8)),
        ProviderObservationSchemaError::StructuredDepthExceeded,
    );
    assert_closed_schema(
        drive(
            web_search(r#"{"payload":null,"type":"unknown"}"#),
            SinkOptions::with_page_capacity(8),
        ),
        ProviderObservationSchemaError::UnknownOrLateVariant,
    );
    assert_closed_schema(
        drive(
            web_search(r#"{"type":"unknown","type":"unknown-again"}"#),
            SinkOptions::with_page_capacity(8),
        ),
        ProviderObservationSchemaError::DuplicateField,
    );
    assert_closed_schema(
        drive(
            web_search(r#"{"type":""}"#),
            SinkOptions::with_page_capacity(8),
        ),
        ProviderObservationSchemaError::UnknownOrLateVariant,
    );
    assert_closed_schema(
        drive(
            web_search(r#"{"type":"search","targetDrift":null}"#),
            SinkOptions::with_page_capacity(8),
        ),
        ProviderObservationSchemaError::UnknownField,
    );
    assert_closed_schema(
        drive(
            web_search(r#"{"type":"unknown","payload":[1,]}"#),
            SinkOptions::with_page_capacity(8),
        ),
        ProviderObservationSchemaError::EnvelopeShape,
    );
}

fn web_search(action: &str) -> String {
    let item = format!(
        "{{\"type\":\"webSearch\",\"id\":\"item_1\",\"query\":\"query\",\"action\":{action}}}"
    );
    lifecycle(&item, true)
}

fn nested_arrays(depth: usize) -> String {
    format!("{}null{}", "[".repeat(depth), "]".repeat(depth))
}

fn assert_other_controls(controls: &[ProviderObservationControl]) {
    assert!(
        controls.contains(&ProviderObservationControl::BeginContainer {
            context: ACTION,
            container: ProviderContainer::Object,
        })
    );
    assert!(controls.contains(&ProviderObservationControl::Enum {
        context: ACTION_KIND,
        value: beryl_backend::ProviderEnumValue::Other,
    }));
    assert!(
        controls.contains(&ProviderObservationControl::EndContainer {
            context: ACTION,
            container: ProviderContainer::Object,
        })
    );
}

fn assert_payload_absent(fragments: &[(ProviderValueContext, Vec<u8>)], payload: &[u8]) {
    let emitted: Vec<u8> = fragments
        .iter()
        .flat_map(|(_, bytes)| bytes.iter().copied())
        .collect();
    assert!(!emitted.windows(payload.len()).any(|part| part == payload));
}

fn assert_closed_schema(case: CaseResult, expected: ProviderObservationSchemaError) {
    assert_schema_error(case.outcome.unwrap_err(), expected);
    assert_eq!(
        case.trace.abandons,
        [ProviderObservationAbandonReason::SchemaFailure]
    );
    assert_eq!(case.trace.leased_at_abandon, [0]);
    assert!(case.trace.seal_routes.is_empty());
    assert_eq!(case.pool.available, 1);
    assert_eq!(case.pool.leased, 0);
}
