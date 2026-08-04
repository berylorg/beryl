use beryl_backend::{
    BoundedResponseResult, ForegroundIngressError, ThreadStatus,
    lifecycle_test_support::{
        IncomingJsonExpectation, IncomingJsonTestOutcome, IncomingJsonTestResult,
    },
};

use crate::support::{FRAGMENT_SIZES, decode};

#[test]
fn thread_read_discards_additive_thread_flags_and_retains_compact_metadata() {
    let incidental = "h".repeat(96 * 1_024);
    let source = format!(
        r#"{{"futureSource":{{"payload":"{incidental}"}},"subAgent":{{"futureVariant":null,"thread_spawn":{{"parent_thread_id":"parent","depth":1,"agent_path":"worker","futureSpawn":["{incidental}"],"agent_nickname":"Ada","agent_role":null}}}}}}"#,
    );
    let status = format!(
        r#"{{"futureStatus":{{"payload":"{incidental}"}},"type":"active","activeFlags":["futureFlag","waitingOnUserInput"]}}"#,
    );
    let input = thread_read_response(51, "thread-51", &status, &source, r#""Ada""#, &incidental);

    for &fragment_bytes in FRAGMENT_SIZES {
        let result = decode(
            &input,
            fragment_bytes,
            IncomingJsonExpectation::ThreadRead { id: 51 },
        );
        let IncomingJsonTestOutcome::Response {
            id,
            result: BoundedResponseResult::ThreadRead(metadata),
        } = result.outcome
        else {
            panic!(
                "unexpected thread/read outcome at fragment size {fragment_bytes}: {:?}",
                result.outcome
            );
        };
        assert_eq!(id, 51);
        assert_eq!(metadata.thread_id().as_str(), "thread-51");
        assert!(metadata.status().waiting_on_user_input());
        assert_eq!(metadata.model_provider(), "openai");
        assert_eq!(metadata.agent_nickname(), Some("Ada"));
        assert_eq!(result.expectation_after, IncomingJsonExpectation::Idle);
        assert_eq!(result.consumed_input_bytes, input.len());
        assert!(result.maximum_buffered_input_bytes <= fragment_bytes.max(4));
    }
}

#[test]
fn thread_read_accepts_an_absent_nested_path_and_equal_null_mirrors() {
    let cases = [
        (r#""appServer""#, r#""Top""#, Some("Top")),
        (
            r#"{"subAgent":{"thread_spawn":{"parent_thread_id":"parent","depth":1,"agent_path":null,"agent_nickname":null,"agent_role":null}}}"#,
            "null",
            None,
        ),
        (r#""appServer""#, "null", None),
    ];

    for (source, top_level, expected_nickname) in cases {
        let input = thread_read_response(
            52,
            "thread-52",
            r#"{"type":"notLoaded"}"#,
            source,
            top_level,
            "discarded",
        );
        let result = decode(&input, 1, IncomingJsonExpectation::ThreadRead { id: 52 });
        let IncomingJsonTestOutcome::Response {
            result: BoundedResponseResult::ThreadRead(metadata),
            ..
        } = result.outcome
        else {
            panic!("unexpected nickname-path outcome: {:?}", result.outcome);
        };
        assert_eq!(metadata.status(), &ThreadStatus::NotLoaded);
        assert_eq!(metadata.agent_nickname(), expected_nickname);
        assert_eq!(result.expectation_after, IncomingJsonExpectation::Idle);
        assert_eq!(result.consumed_input_bytes, input.len());
    }
}

#[test]
fn malformed_thread_read_fields_are_consumed_without_partial_publication() {
    let nested = r#"{"subAgent":{"thread_spawn":{"parent_thread_id":"parent","depth":1,"agent_path":null,"agent_nickname":"Ada","agent_role":null}}}"#;
    let valid = thread_read_response(
        53,
        "thread-53",
        r#"{"type":"idle"}"#,
        nested,
        r#""Ada""#,
        "discarded",
    );
    let cases = [
        valid.replacen(r#""modelProvider":"openai","#, "", 1),
        valid.replacen(
            r#""modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{"type":"idle"}"#,
            r#""status":{"type":"idle"},"createdAt":1,"updatedAt":2,"recencyAt":null,"modelProvider":"openai""#,
            1,
        ),
        valid.replacen(
            r#""agentNickname":"Ada""#,
            r#""agentNickname":"Ada","agentNickname":"Ada""#,
            1,
        ),
        valid.replacen("thread-53", &"i".repeat(257), 1),
        valid.replacen("\"openai\"", &format!("\"{}\"", "p".repeat(257)), 1),
        valid.replacen("\"Ada\"", &format!("\"{}\"", "n".repeat(1_025)), 1),
        valid.replacen(r#""agentNickname":"Ada""#, r#""agentNickname":"Other""#, 1),
        valid.replacen(r#""agentNickname":"Ada""#, r#""agentNickname":null"#, 1),
        valid.replacen(r#""agent_nickname":"Ada""#, r#""agent_nickname":null"#, 1),
        valid.replacen(
            r#""agent_nickname":"Ada""#,
            r#""agent_nickname":"Ada","agent_nickname":"Ada""#,
            1,
        ),
        valid.replacen(
            nested,
            r#"{"subAgent":{"thread_spawn":"invalid"}}"#,
            1,
        ),
        valid.replacen(nested, r#"{"subAgent":"invalid"}"#, 1),
        valid.replacen(r#","agent_nickname":"Ada""#, "", 1),
    ];

    for input in cases {
        let result = decode(&input, 1, IncomingJsonExpectation::ThreadRead { id: 53 });
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

fn thread_read_response(
    id: u64,
    thread_id: &str,
    status: &str,
    source: &str,
    agent_nickname: &str,
    incidental: &str,
) -> String {
    format!(
        r#"{{"id":{id},"result":{{"thread":{{"id":"{thread_id}","extra":null,"sessionId":"session-id","forkedFromId":null,"parentThreadId":null,"preview":"{incidental}","ephemeral":false,"isPinned":true,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{status},"path":null,"cwd":"C:\\work","cliVersion":"0.146.0","source":{source},"canAcceptDirectInput":false,"threadSource":null,"agentNickname":{agent_nickname},"agentRole":null,"gitInfo":null,"name":null,"turns":[{{"items":[{{"text":"{incidental}"}}]}}]}}}}}}"#,
    )
}
