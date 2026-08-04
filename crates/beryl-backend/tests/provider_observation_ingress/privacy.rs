use beryl_backend::{
    OrderedTurnStreamProgress, ProviderDeltaKind, ProviderField, ProviderObservationAbandonReason,
    ProviderObservationBegin, ProviderObservationControl, ProviderObservationSchemaError,
    ProviderScalar, ProviderValueContext,
};

use super::assert_schema_error;
use super::support::{
    CaseResult, SinkOptions, delta_message, drive, drive_fragmented, fragments_for, lifecycle,
};

const SUMMARY: ProviderValueContext = ProviderValueContext::Field(ProviderField::ReasoningSummary);
const ITEM_ID: ProviderValueContext = ProviderValueContext::Field(ProviderField::ItemId);
const CONTENT_INDEX: ProviderValueContext =
    ProviderValueContext::Field(ProviderField::DeltaContentIndex);

#[test]
fn reasoning_text_delta_discards_fragmented_and_large_required_wire_text() {
    let escaped = "PRIVATE-REASONING-\u{e9}-\u{1f642}-\\-\"-\n";
    for split in [1, 2, 3, 7, 31, 63] {
        let case = drive_fragmented(
            reasoning_delta(escaped, 9),
            SinkOptions::with_page_capacity(5),
            split,
        );
        assert_reasoning_delta_is_content_free(&case, 9);
    }

    let large = format!("PRIVATE-LARGE-{}", "reasoning-wire-only-".repeat(20_000));
    let case = drive_fragmented(
        reasoning_delta(&large, 17),
        SinkOptions::with_page_capacity(5),
        31,
    );
    assert_reasoning_delta_is_content_free(&case, 17);
    assert_eq!(case.pool.high_water, 1);
}

#[test]
fn reasoning_text_delta_requires_one_json_string_delta() {
    let missing = drive_fragmented(
        delta_message("item/reasoning/textDelta", r#""contentIndex":0"#),
        SinkOptions::with_page_capacity(5),
        2,
    );
    assert_private_delta_failure(&missing);
    assert_schema_error(
        missing.outcome.unwrap_err(),
        ProviderObservationSchemaError::MissingField,
    );

    for value in ["null", "false", "0", "{}", "[]"] {
        let payload = format!("\"delta\":{value},\"contentIndex\":0");
        let case = drive_fragmented(
            delta_message("item/reasoning/textDelta", &payload),
            SinkOptions::with_page_capacity(5),
            3,
        );
        assert_private_delta_failure(&case);
        assert_schema_error(
            case.outcome.unwrap_err(),
            ProviderObservationSchemaError::WrongType,
        );
    }

    let malformed = drive_fragmented(
        delta_message(
            "item/reasoning/textDelta",
            r#""delta":"PRIVATE\q","contentIndex":0"#,
        ),
        SinkOptions::with_page_capacity(5),
        1,
    );
    assert_private_delta_failure(&malformed);
    assert_schema_error(
        malformed.outcome.unwrap_err(),
        ProviderObservationSchemaError::InvalidString,
    );
}

#[test]
fn private_reasoning_content_emits_no_bytes_or_controls() {
    let private = format!("PRIVATE-REASONING-{}", "é🙂secret".repeat(200));
    let public_only = reasoning(r#"["public-é🙂"]"#, None);
    let expected = drive(
        lifecycle(&public_only, true),
        SinkOptions::with_page_capacity(5),
    );
    assert_eq!(
        expected.outcome.unwrap(),
        OrderedTurnStreamProgress::Progress
    );

    for split in [1, 2, 3, 7, 31, 63] {
        let with_private = reasoning(
            r#"["public-é🙂"]"#,
            Some(&format!("[{}]", serde_json::to_string(&private).unwrap())),
        );
        let case = drive_fragmented(
            lifecycle(&with_private, true),
            SinkOptions::with_page_capacity(5),
            split,
        );
        assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
        assert_eq!(
            case.trace.controls, expected.trace.controls,
            "split {split}"
        );
        assert_eq!(
            fragments_for(&case.trace, ITEM_ID),
            fragments_for(&expected.trace, ITEM_ID),
            "split {split}"
        );
        assert_eq!(fragments_for(&case.trace, SUMMARY), "public-é🙂".as_bytes());
        assert!(
            case.trace
                .fragments
                .iter()
                .all(|(context, _)| *context == ITEM_ID || *context == SUMMARY)
        );
        let emitted: Vec<u8> = case
            .trace
            .fragments
            .iter()
            .flat_map(|(_, bytes)| bytes.iter().copied())
            .collect();
        assert!(!emitted.windows(17).any(|part| part == b"PRIVATE-REASONING"));
        assert_eq!(case.pool.high_water, 1);
        assert_eq!(case.pool.leased, 0);
    }
}

#[test]
fn private_reasoning_content_still_validates_shape_and_uniqueness() {
    assert_closed(
        reasoning(r#"["public"]"#, Some("[1]")),
        ProviderObservationSchemaError::WrongType,
    );
    let duplicate =
        r#"{"type":"reasoning","id":"item_1","summary":["public"],"content":[],"content":[]}"#;
    assert_closed(
        duplicate.to_string(),
        ProviderObservationSchemaError::DuplicateField,
    );
    let malformed =
        r#"{"type":"reasoning","id":"item_1","summary":["public"],"content":["private",]}"#;
    assert_closed(
        malformed.to_string(),
        ProviderObservationSchemaError::EnvelopeShape,
    );
}

fn reasoning(summary: &str, content: Option<&str>) -> String {
    format!(
        "{{\"type\":\"reasoning\",\"id\":\"item_1\",\"summary\":{summary}{}}}",
        content
            .map(|content| format!(",\"content\":{content}"))
            .unwrap_or_default()
    )
}

fn assert_closed(item: String, expected: ProviderObservationSchemaError) {
    let case: CaseResult = drive(lifecycle(&item, true), SinkOptions::with_page_capacity(5));
    assert_schema_error(case.outcome.unwrap_err(), expected);
    assert_eq!(
        case.trace.abandons,
        [ProviderObservationAbandonReason::SchemaFailure]
    );
    assert_eq!(case.trace.leased_at_abandon, [0]);
    assert_eq!(case.pool.leased, 0);
}

fn reasoning_delta(delta: &str, content_index: u64) -> String {
    delta_message(
        "item/reasoning/textDelta",
        &format!(
            "\"delta\":{},\"contentIndex\":{content_index}",
            serde_json::to_string(delta).unwrap()
        ),
    )
}

fn assert_reasoning_delta_is_content_free(case: &CaseResult, content_index: u64) {
    assert_eq!(
        case.outcome.as_ref().unwrap(),
        &OrderedTurnStreamProgress::Progress
    );
    assert_eq!(
        case.trace.begins,
        [ProviderObservationBegin::Delta {
            kind: ProviderDeltaKind::ReasoningTextObserved,
        }]
    );
    assert_eq!(fragments_for(&case.trace, ITEM_ID), b"item_1");
    assert_eq!(
        case.trace.controls,
        [
            ProviderObservationControl::BeginField(ITEM_ID),
            ProviderObservationControl::EndField(ITEM_ID),
            ProviderObservationControl::Scalar {
                context: CONTENT_INDEX,
                value: ProviderScalar::Unsigned(content_index),
            },
        ]
    );
    assert!(
        case.trace
            .fragments
            .iter()
            .all(|(context, _)| *context == ITEM_ID)
    );
    assert!(case.trace.abandons.is_empty());
    assert_eq!(case.pool.available, 1);
    assert_eq!(case.pool.leased, 0);
}

fn assert_private_delta_failure(case: &CaseResult) {
    if let Err(error) = &case.outcome {
        assert!(!format!("{error:?}").contains("PRIVATE"));
    }
    assert!(case.trace.fragments.iter().all(|(context, bytes)| {
        *context == ITEM_ID && !bytes.windows(7).any(|part| part == b"PRIVATE")
    }));
    assert!(case.trace.controls.iter().all(|control| {
        !matches!(
            control,
            ProviderObservationControl::BeginField(ProviderValueContext::Field(
                ProviderField::DeltaText
            )) | ProviderObservationControl::EndField(ProviderValueContext::Field(
                ProviderField::DeltaText
            ))
        )
    }));
    assert_eq!(
        case.trace.abandons,
        [ProviderObservationAbandonReason::SchemaFailure]
    );
    assert_eq!(case.trace.leased_at_abandon, [0]);
    assert_eq!(case.pool.available, 1);
    assert_eq!(case.pool.leased, 0);
}
