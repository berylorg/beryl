use beryl_backend::{
    BoundedResponseResult, CompatibilityProbe, CompatibilityProbeResult, EmptyAcknowledgement,
    ForegroundIngressError, JsonRpcErrorVerdict, ThreadUnsubscribeStatus,
    lifecycle_test_support::{IncomingJsonExpectation, IncomingJsonTestOutcome},
};

use crate::support::{FRAGMENT_SIZES, assert_ingress_error, decode};

fn compatibility_success(probe: CompatibilityProbe, incidental: &str) -> String {
    match probe {
        CompatibilityProbe::ConfigRead => format!(
            r#"{{"config":{{"model":"gpt-5","model_reasoning_effort":"high","features":{{"multi_agent_v2":{{"enabled":true,"expose_spawn_agent_model_overrides":true}}}}}},"origins":{{"features.multi_agent_v2.enabled":{{"name":{{"type":"sessionFlags"}},"version":"0"}},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{{"name":{{"type":"sessionFlags"}},"version":"0"}},"opaque":"{incidental}"}}}}"#
        ),
        CompatibilityProbe::ModelList => r#"{"data":[],"nextCursor":null}"#.to_string(),
        CompatibilityProbe::ThreadCompactStart
        | CompatibilityProbe::ThreadInjectItems
        | CompatibilityProbe::TurnInterrupt => r#"{}"#.to_string(),
        CompatibilityProbe::ThreadFork => format!(
            r#"{{"thread":{{"opaque":"{incidental}"}},"model":[{{"opaque":"{incidental}"}}],"modelProvider":null,"serviceTier":null,"cwd":"/tmp","runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":null,"sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":"high","multiAgentMode":false}}"#
        ),
        CompatibilityProbe::ThreadResume => format!(
            r#"{{"thread":{{"opaque":"{incidental}"}},"model":[{{"opaque":"{incidental}"}}],"modelProvider":null,"serviceTier":null,"cwd":"/tmp","runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":null,"sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":"high","multiAgentMode":false,"initialTurnsPage":{{"opaque":"{incidental}"}},"turnsBackwardsCursor":null,"itemsBackwardsCursor":null}}"#
        ),
        CompatibilityProbe::ThreadRollback => {
            format!(r#"{{"thread":{{"opaque":"{incidental}"}}}}"#)
        }
        CompatibilityProbe::ThreadUnsubscribe => r#"{"status":"notSubscribed"}"#.to_string(),
        CompatibilityProbe::TurnStart => {
            format!(r#"{{"turn":{{"opaque":"{incidental}"}}}}"#)
        }
        CompatibilityProbe::TurnSteer => r#"{"turnId":"turn-1"}"#.to_string(),
    }
}

fn decode_compatibility_success(
    probe: CompatibilityProbe,
    result: &str,
    fragment_bytes: usize,
) -> CompatibilityProbeResult {
    let input = format!(r#"{{"id":61,"result":{result}}}"#);
    let decoded = decode(
        &input,
        fragment_bytes,
        IncomingJsonExpectation::Compatibility { id: 61, probe },
    );
    assert_eq!(decoded.expectation_after, IncomingJsonExpectation::Idle);
    assert_eq!(decoded.consumed_input_bytes, input.len());
    assert!(decoded.maximum_buffered_input_bytes <= fragment_bytes.max(1));
    match decoded.outcome {
        IncomingJsonTestOutcome::Response {
            id: 61,
            result: BoundedResponseResult::Compatibility(result),
        } => result,
        other => panic!("unexpected compatibility result for {probe:?}: {other:?}"),
    }
}

#[test]
fn every_compatibility_success_family_is_purpose_distinct_under_fragmentation() {
    let incidental = "x".repeat(48 * 1024);
    for probe in CompatibilityProbe::ALL {
        let result_json = compatibility_success(probe, &incidental);
        for &fragment_bytes in FRAGMENT_SIZES {
            let result = decode_compatibility_success(probe, &result_json, fragment_bytes);
            assert_eq!(result.probe(), probe);
            match (probe, result) {
                (CompatibilityProbe::ConfigRead, CompatibilityProbeResult::ConfigRead(config)) => {
                    assert_eq!(config.defaults().model(), Some("gpt-5"));
                    assert!(config.defaults().proves_spawn_agent_model_overrides());
                }
                (CompatibilityProbe::ModelList, CompatibilityProbeResult::ModelList(page)) => {
                    assert!(page.is_empty());
                    assert_eq!(page.next_cursor(), None);
                }
                (
                    CompatibilityProbe::ThreadUnsubscribe,
                    CompatibilityProbeResult::ThreadUnsubscribe(status),
                ) => assert_eq!(status, ThreadUnsubscribeStatus::NotSubscribed),
                (expected, CompatibilityProbeResult::UnexpectedMutatingSuccess(actual)) => {
                    assert_eq!(actual, expected);
                }
                (_, other) => panic!("wrong purpose-specific compatibility result: {other:?}"),
            }
        }
    }
}

#[test]
fn ordinary_empty_acknowledgements_and_unsubscribe_statuses_remain_distinct() {
    let acknowledgements = [
        (
            IncomingJsonExpectation::ThreadCompactStart { id: 62 },
            EmptyAcknowledgement::ThreadCompactStart,
        ),
        (
            IncomingJsonExpectation::ThreadInjectItems { id: 62 },
            EmptyAcknowledgement::ThreadInjectItems,
        ),
        (
            IncomingJsonExpectation::ThreadBackgroundTerminalsClean { id: 62 },
            EmptyAcknowledgement::ThreadBackgroundTerminalsClean,
        ),
        (
            IncomingJsonExpectation::TurnInterrupt { id: 62 },
            EmptyAcknowledgement::TurnInterrupt,
        ),
    ];
    let input = r#"{"id":62,"result":{}}"#;
    for (expectation, expected) in acknowledgements {
        for &fragment_bytes in FRAGMENT_SIZES {
            let decoded = decode(input, fragment_bytes, expectation);
            assert!(matches!(
                decoded.outcome,
                IncomingJsonTestOutcome::Response {
                    id: 62,
                    result: BoundedResponseResult::EmptyAcknowledgement(actual),
                } if actual == expected
            ));
            assert_eq!(decoded.expectation_after, IncomingJsonExpectation::Idle);
            assert_eq!(decoded.consumed_input_bytes, input.len());
        }
    }

    let statuses = [
        ("notLoaded", ThreadUnsubscribeStatus::NotLoaded),
        ("notSubscribed", ThreadUnsubscribeStatus::NotSubscribed),
        ("unsubscribed", ThreadUnsubscribeStatus::Unsubscribed),
    ];
    for (wire, expected) in statuses {
        let input = format!(r#"{{"id":62,"result":{{"status":"{wire}"}}}}"#);
        for &fragment_bytes in FRAGMENT_SIZES {
            let decoded = decode(
                &input,
                fragment_bytes,
                IncomingJsonExpectation::ThreadUnsubscribe { id: 62 },
            );
            assert!(matches!(
                decoded.outcome,
                IncomingJsonTestOutcome::Response {
                    id: 62,
                    result: BoundedResponseResult::ThreadUnsubscribe(actual),
                } if actual == expected
            ));
            assert_eq!(decoded.expectation_after, IncomingJsonExpectation::Idle);
            assert_eq!(decoded.consumed_input_bytes, input.len());
        }
    }
}

#[test]
fn exact_invalid_request_rejection_recognizes_only_mutating_compatibility_probes() {
    for probe in CompatibilityProbe::ALL {
        let input = r#"{"error":{"code":-32600,"message":"invalid request"},"id":63}"#;
        for &fragment_bytes in FRAGMENT_SIZES {
            let decoded = decode(
                input,
                fragment_bytes,
                IncomingJsonExpectation::Compatibility { id: 63, probe },
            );
            let verdict = match decoded.outcome {
                IncomingJsonTestOutcome::Rejection {
                    code: -32600,
                    data_was_present: false,
                    verdict,
                    ..
                } => verdict,
                other => panic!("unexpected compatibility rejection for {probe:?}: {other:?}"),
            };
            let mutating = matches!(
                probe,
                CompatibilityProbe::ThreadCompactStart
                    | CompatibilityProbe::ThreadFork
                    | CompatibilityProbe::ThreadInjectItems
                    | CompatibilityProbe::ThreadResume
                    | CompatibilityProbe::ThreadRollback
                    | CompatibilityProbe::TurnInterrupt
                    | CompatibilityProbe::TurnStart
                    | CompatibilityProbe::TurnSteer
            );
            assert_eq!(
                verdict,
                mutating.then_some(JsonRpcErrorVerdict::CompatibilityProbeRecognized { probe })
            );
            assert_eq!(decoded.expectation_after, IncomingJsonExpectation::Idle);
            assert_eq!(decoded.consumed_input_bytes, input.len());
        }
    }
}

#[test]
fn compatibility_rejection_verdict_requires_exact_code_and_absent_data() {
    let cases = [
        (
            r#"{"error":{"code":-32600,"data":null,"message":"invalid request"},"id":64}"#,
            true,
        ),
        (
            r#"{"error":{"code":-32601,"message":"method not found"},"id":64}"#,
            false,
        ),
    ];
    for (input, data_was_present) in cases {
        let decoded = decode(
            input,
            1,
            IncomingJsonExpectation::Compatibility {
                id: 64,
                probe: CompatibilityProbe::TurnStart,
            },
        );
        match decoded.outcome {
            IncomingJsonTestOutcome::Rejection {
                data_was_present: actual,
                verdict,
                ..
            } => {
                assert_eq!(actual, data_was_present);
                assert_eq!(verdict, None);
            }
            other => panic!("unexpected rejection result: {other:?}"),
        }
        assert_eq!(decoded.expectation_after, IncomingJsonExpectation::Idle);
        assert_eq!(decoded.consumed_input_bytes, input.len());
    }
}

#[test]
fn every_compatibility_success_schema_fails_closed_after_full_consumption() {
    let cases = [
        (
            CompatibilityProbe::ConfigRead,
            r#"{"origins":{},"config":{"model":null,"model_reasoning_effort":null}}"#,
        ),
        (
            CompatibilityProbe::ModelList,
            r#"{"nextCursor":null,"data":[]}"#,
        ),
        (CompatibilityProbe::ThreadCompactStart, r#"{"extra":true}"#),
        (
            CompatibilityProbe::ThreadFork,
            r#"{"model":null,"thread":{}}"#,
        ),
        (CompatibilityProbe::ThreadInjectItems, r#"[]"#),
        (CompatibilityProbe::ThreadResume, r#"{"thread":{}}"#),
        (CompatibilityProbe::ThreadRollback, r#"{"thread":[]}"#),
        (
            CompatibilityProbe::ThreadUnsubscribe,
            r#"{"status":"unknown"}"#,
        ),
        (CompatibilityProbe::TurnInterrupt, r#"{"extra":null}"#),
        (CompatibilityProbe::TurnStart, r#"{"turn":[]}"#),
        (CompatibilityProbe::TurnSteer, r#"{"turnId":7}"#),
    ];

    for (probe, result) in cases {
        let input = format!(r#"{{"id":65,"result":{result}}}"#);
        assert_ingress_error(
            decode(
                &input,
                2,
                IncomingJsonExpectation::Compatibility { id: 65, probe },
            ),
            ForegroundIngressError::MalformedResponse,
            IncomingJsonExpectation::Poisoned,
            input.len(),
        );
    }
}
