#[allow(
    dead_code,
    reason = "shared ordered-sink support is compiled independently by sibling integration tests"
)]
#[path = "provider_observation_ingress/support.rs"]
mod provider_support;

use beryl_backend::{
    ForegroundIngressError, ManagedBackendError, OrderedTurnStreamProgress, ProviderEnumValue,
    ProviderField, ProviderItemKind, ProviderItemLifecycle, ProviderObservationAbandonReason,
    ProviderObservationBegin, ProviderObservationControl, ProviderObservationError,
    ProviderObservationSchemaError, ProviderScalar, ProviderStructuredPosition,
    ProviderValueContext,
};
use provider_support::{CaseResult, SinkOptions, Trace, drive, drive_fragmented, fragments_for};

const TIMESTAMP_MS: u64 = 1_752_689_600_123;
const ITEM_ID: ProviderValueContext = ProviderValueContext::Field(ProviderField::ItemId);
const OBSERVED_AT: ProviderValueContext =
    ProviderValueContext::Field(ProviderField::LifecycleObservedAt);
const IMAGE_STATUS: ProviderValueContext =
    ProviderValueContext::Field(ProviderField::ImageGenerationStatus);
const REVISED_PROMPT: ProviderValueContext =
    ProviderValueContext::Field(ProviderField::ImageGenerationRevisedPrompt);
const SAVED_PATH: ProviderValueContext =
    ProviderValueContext::Field(ProviderField::ImageGenerationSavedPath);

#[test]
fn image_result_is_absent_after_the_pinned_discriminators_for_every_supported_position() {
    let cases = [
        (
            "RESULT_FIRST_MUST_DISAPPEAR",
            lifecycle_notification(
                "item/completed",
                r#"{"type":"imageGeneration","result":"RESULT_FIRST_MUST_DISAPPEAR","savedPath":"C:\\runtime\\generated one.png","revisedPrompt":"line\n\"quoted\"","status":"completed","id":"image-first"}"#,
            ),
            "image-first",
            Some(&b"C:\\runtime\\generated one.png"[..]),
            Some(&b"line\n\"quoted\""[..]),
            false,
        ),
        (
            "RESULT_MIDDLE_MUST_DISAPPEAR",
            lifecycle_notification(
                "item/completed",
                r#"{"type":"imageGeneration","id":"image-middle","result":"RESULT_MIDDLE_MUST_DISAPPEAR","status":"completed","savedPath":""}"#,
            ),
            "image-middle",
            Some(&b""[..]),
            None,
            false,
        ),
        (
            "RESULT_LAST_MUST_DISAPPEAR",
            lifecycle_notification(
                "item/completed",
                r#"{"type":"imageGeneration","id":"image-last","status":"completed","revisedPrompt":null,"result":"RESULT_LAST_MUST_DISAPPEAR"}"#,
            ),
            "image-last",
            None,
            None,
            true,
        ),
    ];

    for (marker, message, item_id, saved_path, revised_prompt, prompt_is_null) in cases {
        let case = drive(message, SinkOptions::with_page_capacity(7));
        assert_image_case(
            &case,
            ProviderItemLifecycle::Completed,
            item_id,
            saved_path,
            revised_prompt,
        );
        assert_eq!(
            case.trace
                .controls
                .contains(&ProviderObservationControl::Scalar {
                    context: REVISED_PROMPT,
                    value: ProviderScalar::Null,
                }),
            prompt_is_null
        );
        assert_trace_omits(&case.trace, marker);
    }
}

#[test]
fn huge_fragmented_result_is_discarded_before_bounded_page_retention() {
    let small_marker = "SMALL_RESULT_SECRET";
    let huge_marker = "HUGE_RESULT_SECRET_".repeat(150_000);
    let small = lifecycle_notification(
        "item/started",
        &format!(
            r#"{{"type":"imageGeneration","id":"image-page-bound","status":"inProgress","savedPath":"generated.png","result":"{small_marker}"}}"#
        ),
    );
    let huge = lifecycle_notification(
        "item/started",
        &format!(
            r#"{{"type":"imageGeneration","id":"image-page-bound","status":"inProgress","savedPath":"generated.png","result":"{huge_marker}"}}"#
        ),
    );
    let small = drive_fragmented(small, SinkOptions::with_page_capacity(31), 7);
    let huge = drive_fragmented(huge, SinkOptions::with_page_capacity(31), 7);

    assert_eq!(small.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
    assert_eq!(huge.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
    assert_eq!(huge.trace.controls, small.trace.controls);
    assert_eq!(huge.trace.fragments, small.trace.fragments);
    assert_eq!(small.trace.begins.len(), 1);
    assert_eq!(huge.trace.begins.len(), 1);
    assert_eq!(small.trace.seal_routes.len(), 1);
    assert_eq!(huge.trace.seal_routes.len(), 1);
    assert!(small.trace.abandons.is_empty());
    assert!(huge.trace.abandons.is_empty());
    assert_eq!(small.trace.leased_at_seal, [0]);
    assert_eq!(huge.trace.leased_at_seal, [0]);
    assert_trace_omits(&small.trace, small_marker);
    assert_trace_omits(&huge.trace, "HUGE_RESULT_SECRET");
    assert_eq!(small.pool.high_water, 1);
    assert_eq!(huge.pool.high_water, 1);
    assert_eq!(small.pool.available, 1);
    assert_eq!(huge.pool.available, 1);
    assert_eq!(small.pool.leased, 0);
    assert_eq!(huge.pool.leased, 0);
}

#[test]
fn non_image_item_result_remains_exact_on_the_same_lifecycle_path() {
    let marker = "MCP_RESULT_MUST_REMAIN";
    let item = format!(
        r#"{{"type":"mcpToolCall","id":"mcp-result","server":"fixture","tool":"inspect","status":"completed","arguments":{{"input":true}},"appContext":null,"mcpAppResourceUri":null,"pluginId":null,"result":{{"content":[{{"type":"text","text":"{marker}"}}],"structuredContent":{{"marker":"{marker}"}},"_meta":null}},"error":null,"durationMs":12}}"#
    );
    let case = drive(
        lifecycle_notification("item/completed", &item),
        SinkOptions::with_page_capacity(7),
    );
    assert_eq!(
        case.outcome.as_ref().unwrap(),
        &OrderedTurnStreamProgress::Progress
    );
    assert_eq!(
        case.trace.begins,
        [ProviderObservationBegin::Item {
            lifecycle: ProviderItemLifecycle::Completed,
            kind: ProviderItemKind::McpToolCall,
        }]
    );
    assert_eq!(fragments_for(&case.trace, ITEM_ID), b"mcp-result");
    assert_eq!(
        fragments_for(
            &case.trace,
            ProviderValueContext::Structured {
                root: ProviderField::McpResultContent,
                depth: 1,
                position: ProviderStructuredPosition::ObjectValue { entry: 1 },
            },
        ),
        marker.as_bytes()
    );
    assert_eq!(
        fragments_for(
            &case.trace,
            ProviderValueContext::Structured {
                root: ProviderField::McpStructuredContent,
                depth: 1,
                position: ProviderStructuredPosition::ObjectValue { entry: 0 },
            },
        ),
        marker.as_bytes()
    );
    assert!(
        case.trace
            .controls
            .contains(&ProviderObservationControl::Enum {
                context: ProviderValueContext::Field(ProviderField::McpStatus),
                value: ProviderEnumValue::Completed,
            })
    );
    assert_common_seal(&case);
}

#[test]
fn malformed_or_ambiguous_image_shapes_fail_closed_without_diagnostic_leakage() {
    let marker = "MALFORMED_RESULT_SECRET";
    let valid_item = format!(
        r#"{{"type":"imageGeneration","id":"image","status":"completed","result":"{marker}"}}"#
    );
    let cases = vec![
        lifecycle_notification(
            "item/completed",
            r#"{"type":"imageGeneration","id":"image","status":"completed"}"#,
        ),
        lifecycle_notification(
            "item/completed",
            &format!(
                r#"{{"type":"imageGeneration","id":"image","status":"completed","result":["{marker}"]}}"#
            ),
        ),
        lifecycle_notification(
            "item/completed",
            &format!(
                r#"{{"result":"{marker}","type":"imageGeneration","id":"image","status":"completed"}}"#
            ),
        ),
        lifecycle_notification(
            "item/completed",
            &format!(
                r#"{{"type":"imageGeneration","id":"image","status":"completed","result":"{marker}","result":"second"}}"#
            ),
        ),
        lifecycle_notification(
            "item/completed",
            &format!(
                r#"{{"type":"imageGeneration","id":"image","type":"imageGeneration","status":"completed","result":"{marker}"}}"#
            ),
        ),
        format!(
            r#"{{"method":"item/completed","method":"future/method","params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image"}}}}"#
        ),
        format!(
            r#"{{"method":"item/completed","params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image","item":{valid_item}}}}}"#
        ),
        format!(
            r#"{{"method":"item/completed","id":9,"params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image"}}}}"#
        ),
        format!(
            r#"{{"method":"item/completed","params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image"}},"params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image"}}}}"#
        ),
        format!(
            r#"{{"params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image"}},"method":"item/completed"}}"#
        ),
        format!(
            r#"{{"method":"item/completed","params":{{"threadId":"thread-image","item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS}}}}}"#
        ),
    ];
    let expected = [
        ProviderObservationSchemaError::MissingField,
        ProviderObservationSchemaError::WrongType,
        ProviderObservationSchemaError::UnknownOrLateVariant,
        ProviderObservationSchemaError::DuplicateField,
        ProviderObservationSchemaError::UnknownField,
        ProviderObservationSchemaError::EnvelopeShape,
        ProviderObservationSchemaError::UnknownField,
        ProviderObservationSchemaError::EnvelopeShape,
        ProviderObservationSchemaError::EnvelopeShape,
        ProviderObservationSchemaError::EnvelopeShape,
        ProviderObservationSchemaError::EnvelopeShape,
    ];

    for (index, (raw, expected)) in cases.into_iter().zip(expected).enumerate() {
        let case = drive(raw, SinkOptions::with_page_capacity(7));
        let error = case
            .outcome
            .expect_err("malformed image ingress must fail closed");
        if index == 9 {
            assert!(matches!(
                &error,
                ManagedBackendError::ForegroundIngress {
                    source: ForegroundIngressError::Quarantined,
                    ..
                }
            ));
        } else {
            assert_schema_error(&error, expected);
        }
        let display = error.to_string();
        let debug = format!("{error:?}");
        for rendered in [&display, &debug] {
            assert!(!rendered.contains(marker));
            assert!(!rendered.contains("data:image"));
        }
        if case.trace.begins.is_empty() {
            assert!(case.trace.abandons.is_empty());
        } else {
            assert_eq!(
                case.trace.abandons,
                [ProviderObservationAbandonReason::SchemaFailure]
            );
        }
        assert!(case.trace.seal_routes.is_empty());
        assert_eq!(case.pool.available, 1);
        assert_eq!(case.pool.leased, 0);
    }
}

fn assert_image_case(
    case: &CaseResult,
    lifecycle: ProviderItemLifecycle,
    item_id: &str,
    saved_path: Option<&[u8]>,
    revised_prompt: Option<&[u8]>,
) {
    assert_eq!(
        case.outcome.as_ref().unwrap(),
        &OrderedTurnStreamProgress::Progress
    );
    assert_eq!(
        case.trace.begins,
        [ProviderObservationBegin::Item {
            lifecycle,
            kind: ProviderItemKind::StandaloneImageGeneration,
        }]
    );
    assert_eq!(fragments_for(&case.trace, ITEM_ID), item_id.as_bytes());
    assert_eq!(
        fragments_for(&case.trace, SAVED_PATH),
        saved_path.unwrap_or_default()
    );
    assert_eq!(
        fragments_for(&case.trace, REVISED_PROMPT),
        revised_prompt.unwrap_or_default()
    );
    assert!(
        case.trace
            .controls
            .contains(&ProviderObservationControl::Enum {
                context: IMAGE_STATUS,
                value: match lifecycle {
                    ProviderItemLifecycle::Started => ProviderEnumValue::InProgress,
                    ProviderItemLifecycle::Completed => ProviderEnumValue::Completed,
                },
            })
    );
    assert!(
        case.trace
            .controls
            .contains(&ProviderObservationControl::Scalar {
                context: OBSERVED_AT,
                value: ProviderScalar::Unsigned(TIMESTAMP_MS),
            })
    );
    assert_common_seal(case);
}

fn assert_common_seal(case: &CaseResult) {
    assert_eq!(case.trace.seal_routes.len(), 1);
    assert_eq!(
        case.trace.seal_routes[0].thread_id().as_str(),
        "thread-image"
    );
    assert_eq!(case.trace.seal_routes[0].turn_id().as_str(), "turn-image");
    assert!(case.trace.abandons.is_empty());
    assert_eq!(case.trace.leased_at_seal, [0]);
    assert_eq!(case.pool.high_water, 1);
    assert_eq!(case.pool.available, 1);
    assert_eq!(case.pool.leased, 0);
}

fn assert_trace_omits(trace: &Trace, marker: &str) {
    assert!(trace.fragments.iter().all(|(_, bytes)| {
        !bytes
            .windows(marker.len())
            .any(|part| part == marker.as_bytes())
    }));
    let debug = format!("{trace:?}");
    assert!(!debug.contains(marker));
    assert!(!debug.contains("ImageResult"));
}

fn assert_schema_error(error: &ManagedBackendError, expected: ProviderObservationSchemaError) {
    match error {
        ManagedBackendError::ProviderObservation {
            source: ProviderObservationError::Schema(actual),
            ..
        } => assert_eq!(*actual, expected),
        other => panic!("expected provider schema error {expected:?}, got {other:?}"),
    }
}

fn lifecycle_notification(method: &str, item: &str) -> String {
    let timestamp = if method == "item/started" {
        format!(r#""startedAtMs":{TIMESTAMP_MS}"#)
    } else {
        format!(r#""completedAtMs":{TIMESTAMP_MS}"#)
    };
    format!(
        r#"{{"method":"{method}","params":{{"item":{item},"turnId":"turn-image",{timestamp},"threadId":"thread-image"}}}}"#
    )
}
