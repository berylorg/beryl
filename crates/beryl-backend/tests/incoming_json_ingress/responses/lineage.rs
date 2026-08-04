use beryl_backend::{
    BoundedResponseResult, ForegroundIngressError, ThreadActiveFlags, ThreadStatus,
    lifecycle_test_support::{
        IncomingJsonExpectation, IncomingJsonTestOutcome, IncomingJsonTestResult,
    },
};

use crate::support::{FRAGMENT_SIZES, decode};

#[test]
fn lineage_results_discard_additive_thread_flags_and_retain_bounded_metadata() {
    let history = "h".repeat(96 * 1024);
    let cases = [
        (
            IncomingJsonExpectation::ThreadStart { id: 31 },
            lineage_response(31, "start-id", r#"{"type":"idle"}"#, &history, false),
            "start",
        ),
        (
            IncomingJsonExpectation::ThreadResume { id: 32 },
            lineage_response(
                32,
                "resume-id",
                r#"{"type":"active","activeFlags":["waitingOnApproval","waitingOnUserInput"]}"#,
                &history,
                true,
            ),
            "resume",
        ),
        (
            IncomingJsonExpectation::ThreadFork { id: 33 },
            lineage_response(33, "fork-id", r#"{"type":"systemError"}"#, &history, false),
            "fork",
        ),
    ];

    for (expectation, input, family) in cases {
        for &fragment_bytes in FRAGMENT_SIZES {
            let result = decode(&input, fragment_bytes, expectation);
            let IncomingJsonTestOutcome::Response {
                id,
                result: decoded,
            } = result.outcome
            else {
                panic!(
                    "unexpected {family} lineage outcome at fragment size {fragment_bytes}: {:?}",
                    result.outcome
                );
            };
            assert_eq!(id, expectation_id(expectation));
            let response = match (family, decoded) {
                ("start", BoundedResponseResult::ThreadStart(response))
                | ("resume", BoundedResponseResult::ThreadResume(response))
                | ("fork", BoundedResponseResult::ThreadFork(response)) => response,
                (_, other) => panic!("unexpected lineage result: {other:?}"),
            };
            assert_eq!(response.thread_id().as_str(), format!("{family}-id"));
            assert_eq!(response.model(), Some("gpt-5.6"));
            assert_eq!(response.model_provider(), Some("openai"));
            assert_eq!(response.reasoning_effort(), Some("high"));
            match family {
                "start" => assert_eq!(response.status(), &ThreadStatus::Idle),
                "resume" => {
                    assert_eq!(
                        response.status().active_flags(),
                        Some(ThreadActiveFlags::new(true, true))
                    );
                    assert!(response.status().waiting_on_user_input());
                }
                "fork" => assert_eq!(response.status(), &ThreadStatus::SystemError),
                _ => unreachable!(),
            }
            assert_eq!(result.expectation_after, IncomingJsonExpectation::Idle);
            assert_eq!(result.consumed_input_bytes, input.len());
            assert!(result.maximum_buffered_input_bytes <= fragment_bytes.max(4));
        }
    }
}

#[test]
fn unknown_lineage_members_and_active_flags_are_structurally_discarded() {
    let incidental = "u".repeat(96 * 1024);
    let status = format!(
        r#"{{"futureStatusBefore":{{"payload":"{incidental}"}},"type":"active","futureStatusMiddle":[1,2,3],"activeFlags":["futureFlag","waitingOnUserInput"],"futureStatusAfter":null}}"#,
    );
    let input = lineage_response(39, "unknowns-id", &status, &incidental, false)
        .replacen(
            r#""extra":null"#,
            &format!(r#""extra":null,"futureThread":{{"payload":"{incidental}"}}"#),
            1,
        )
        .replacen(
            r#""model":"gpt-5.6","modelProvider":"openai""#,
            &format!(
                r#""model":"gpt-5.6","futureResult":{{"payload":"{incidental}"}},"modelProvider":"openai""#,
            ),
            1,
        );

    for &fragment_bytes in FRAGMENT_SIZES {
        let result = decode(
            &input,
            fragment_bytes,
            IncomingJsonExpectation::ThreadStart { id: 39 },
        );
        let IncomingJsonTestOutcome::Response {
            id,
            result: BoundedResponseResult::ThreadStart(response),
        } = result.outcome
        else {
            panic!(
                "unexpected unknown-member outcome at fragment size {fragment_bytes}: {:?}",
                result.outcome
            );
        };
        assert_eq!(id, 39);
        assert_eq!(response.thread_id().as_str(), "unknowns-id");
        assert_eq!(
            response.status().active_flags(),
            Some(ThreadActiveFlags::new(false, true))
        );
        assert!(response.status().waiting_on_user_input());
        assert_eq!(response.model(), Some("gpt-5.6"));
        assert_eq!(result.expectation_after, IncomingJsonExpectation::Idle);
        assert_eq!(result.consumed_input_bytes, input.len());
        assert!(result.maximum_buffered_input_bytes <= fragment_bytes.max(4));
    }
}

#[test]
fn malformed_lineage_fields_are_consumed_without_partial_publication() {
    let valid = lineage_response(41, "thread-id", r#"{"type":"idle"}"#, "history", false);
    let cases = [
        valid.replacen("\"model\":\"gpt-5.6\",", "", 1),
        valid.replacen(
            "\"model\":\"gpt-5.6\",\"modelProvider\":\"openai\"",
            "\"modelProvider\":\"openai\",\"model\":\"gpt-5.6\"",
            1,
        ),
        valid.replacen(
            "\"model\":\"gpt-5.6\"",
            "\"model\":\"gpt-5.6\",\"model\":\"gpt-5.6\"",
            1,
        ),
        valid.replacen("\"thread-id\"", &format!("\"{}\"", "i".repeat(257)), 1),
        valid.replacen(
            r#"{"type":"idle"}"#,
            r#"{"type":"active","activeFlags":["waitingOnApproval","waitingOnApproval"]}"#,
            1,
        ),
        valid.replacen(r#"{"type":"idle"}"#, r#"{"type":"active"}"#, 1),
        valid.replacen(
            r#"{"type":"idle"}"#,
            r#"{"activeFlags":[],"type":"active"}"#,
            1,
        ),
        valid.replacen(r#"{"type":"idle"}"#, r#"{"type":"futureStatus"}"#, 1),
    ];

    for input in cases {
        let result = decode(&input, 1, IncomingJsonExpectation::ThreadStart { id: 41 });
        assert_malformed(result, input.len());
    }
}

fn assert_malformed(result: IncomingJsonTestResult, input_len: usize) {
    assert!(matches!(
        result.outcome,
        IncomingJsonTestOutcome::IngressError(ForegroundIngressError::MalformedResponse)
    ));
    assert_eq!(result.expectation_after, IncomingJsonExpectation::Poisoned);
    assert_eq!(result.consumed_input_bytes, input_len);
}

fn expectation_id(expectation: IncomingJsonExpectation) -> u64 {
    match expectation {
        IncomingJsonExpectation::ThreadStart { id }
        | IncomingJsonExpectation::ThreadResume { id }
        | IncomingJsonExpectation::ThreadFork { id } => id,
        _ => unreachable!(),
    }
}

fn lineage_response(id: u64, thread_id: &str, status: &str, history: &str, resume: bool) -> String {
    let initial_turns_page = if resume {
        r#","initialTurnsPage":null,"turnsBackwardsCursor":null,"itemsBackwardsCursor":null"#
    } else {
        ""
    };
    format!(
        r#"{{"id":{id},"result":{{"thread":{{"id":"{thread_id}","extra":null,"sessionId":"session-id","forkedFromId":null,"parentThreadId":null,"preview":"{history}","ephemeral":false,"isPinned":true,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{status},"path":null,"cwd":"C:\\work","cliVersion":"0.146.0","source":"appServer","canAcceptDirectInput":false,"threadSource":null,"agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[{{"items":[{{"text":"{history}"}}]}}]}},"model":"gpt-5.6","modelProvider":"openai","serviceTier":null,"cwd":"C:\\work","runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user","sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":"high","multiAgentMode":"explicitRequestOnly"{initial_turns_page}}}}}"#,
    )
}
