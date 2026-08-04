use beryl_backend::{
    BoundedResponseResult, DefaultReasoningEffort, ForegroundIngressError, ReasoningEffort,
    lifecycle_test_support::{IncomingJsonExpectation, IncomingJsonTestOutcome},
};

use crate::support::{FRAGMENT_SIZES, assert_ingress_error, decode};

fn record(
    id: &str,
    supported_reasoning_efforts: &str,
    default_reasoning_effort: &str,
    incidental: &str,
) -> String {
    format!(
        "{{\"id\":\"{id}\",\"opaqueBeforeModel\":{{\"nested\":[\"{incidental}\"]}},\"model\":\"model-{id}\",\"displayName\":\"Model {id}\",\"description\":\"{incidental}\",\"hidden\":false,\"supportedReasoningEfforts\":{supported_reasoning_efforts},\"defaultReasoningEffort\":\"{default_reasoning_effort}\",\"inputModalities\":[\"text\"],\"isDefault\":true,\"opaqueTail\":null}}"
    )
}

fn model_page(input: &str, fragment_bytes: usize) -> Box<beryl_backend::ModelPage> {
    let decoded = decode(
        input,
        fragment_bytes,
        IncomingJsonExpectation::ModelList { id: 51 },
    );
    assert_eq!(decoded.expectation_after, IncomingJsonExpectation::Idle);
    assert_eq!(decoded.consumed_input_bytes, input.len());
    assert!(decoded.maximum_buffered_input_bytes <= fragment_bytes.max(1));
    match decoded.outcome {
        IncomingJsonTestOutcome::Response {
            id: 51,
            result: BoundedResponseResult::ModelList(page),
        } => page,
        other => panic!("unexpected model page result: {other:?}"),
    }
}

#[test]
fn model_page_normalizes_all_effort_shapes_and_discards_large_incidental_values() {
    let incidental = "z".repeat(32 * 1024);
    let records = [
        record(
            "string-shape",
            r#"["low","future-effort","high"]"#,
            "high",
            &incidental,
        ),
        record(
            "record-shape",
            &format!(
                r#"[{{"reasoningEffort":"minimal","description":"{incidental}"}},{{"reasoningEffort":"ultra","metadata":{{"large":"{incidental}"}}}},{{"reasoningEffort":"future-effort"}}]"#
            ),
            "future-default",
            "",
        ),
        record(
            "map-shape",
            &format!(
                r#"{{"none":{{"description":"{incidental}"}},"xhigh":true,"future-effort":{{"opaque":"{incidental}"}}}}"#
            ),
            "none",
            "",
        ),
    ];
    let input = format!(
        "{{\"id\":51,\"result\":{{\"data\":[{}],\"nextCursor\":\"next-page\"}}}}",
        records.join(",")
    );

    for &fragment_bytes in FRAGMENT_SIZES {
        let page = model_page(&input, fragment_bytes);
        assert_eq!(page.len(), 3);
        assert_eq!(page.next_cursor(), Some("next-page"));
        let mut records = page.records();

        let string = records.next().expect("string-shape record");
        assert_eq!(string.id(), "string-shape");
        assert_eq!(string.model(), "model-string-shape");
        assert_eq!(string.display_name(), "Model string-shape");
        assert!(!string.hidden());
        assert!(string.is_default());
        assert!(
            string
                .supported_reasoning_efforts()
                .contains(ReasoningEffort::Low)
        );
        assert!(
            string
                .supported_reasoning_efforts()
                .contains(ReasoningEffort::High)
        );
        assert_eq!(
            string.default_reasoning_effort(),
            DefaultReasoningEffort::High
        );

        let record = records.next().expect("record-shape record");
        assert!(
            record
                .supported_reasoning_efforts()
                .contains(ReasoningEffort::Minimal)
        );
        assert!(
            record
                .supported_reasoning_efforts()
                .contains(ReasoningEffort::Ultra)
        );
        assert_eq!(
            record.default_reasoning_effort(),
            DefaultReasoningEffort::Other
        );

        let map = records.next().expect("map-shape record");
        assert!(
            map.supported_reasoning_efforts()
                .contains(ReasoningEffort::None)
        );
        assert!(
            map.supported_reasoning_efforts()
                .contains(ReasoningEffort::XHigh)
        );
        assert_eq!(map.default_reasoning_effort(), DefaultReasoningEffort::None);
        assert!(records.next().is_none());
    }
}

#[test]
fn model_page_accepts_64_records_and_rejects_the_65th_after_full_consumption() {
    let records = (0..64)
        .map(|index| record(&format!("m{index}"), r#"["medium"]"#, "medium", ""))
        .collect::<Vec<_>>();
    let input = format!(
        "{{\"id\":51,\"result\":{{\"data\":[{}],\"nextCursor\":null}}}}",
        records.join(",")
    );
    let page = model_page(&input, 7);
    assert_eq!(page.len(), 64);
    assert_eq!(page.next_cursor(), None);

    let mut over_capacity = records;
    over_capacity.push(record("m64", r#"["medium"]"#, "medium", ""));
    let input = format!(
        "{{\"id\":51,\"result\":{{\"data\":[{}],\"nextCursor\":null}}}}",
        over_capacity.join(",")
    );
    assert_ingress_error(
        decode(&input, 5, IncomingJsonExpectation::ModelList { id: 51 }),
        ForegroundIngressError::MalformedResponse,
        IncomingJsonExpectation::Poisoned,
        input.len(),
    );
}

#[test]
fn model_page_enforces_cursor_and_retained_text_bounds() {
    let cursor = "c".repeat(1024);
    let bounded = format!(
        "{{\"id\":51,\"result\":{{\"data\":[{}],\"nextCursor\":\"{cursor}\"}}}}",
        record(&"i".repeat(250), r#"["low"]"#, "low", "")
    );
    let page = model_page(&bounded, 31);
    assert_eq!(page.next_cursor(), Some(cursor.as_str()));

    let malformed = [
        format!(
            "{{\"id\":51,\"result\":{{\"data\":[],\"nextCursor\":\"{}\"}}}}",
            "c".repeat(1025)
        ),
        format!(
            "{{\"id\":51,\"result\":{{\"data\":[{}],\"nextCursor\":null}}}}",
            record(&"i".repeat(257), r#"["low"]"#, "low", "")
        ),
        format!(
            "{{\"id\":51,\"result\":{{\"data\":[{}],\"nextCursor\":null}}}}",
            record("id", r#"["low"]"#, "low", &"x".repeat(1025))
                .replace("Model id", &"d".repeat(1025))
        ),
    ];
    for input in malformed {
        assert_ingress_error(
            decode(&input, 13, IncomingJsonExpectation::ModelList { id: 51 }),
            ForegroundIngressError::MalformedResponse,
            IncomingJsonExpectation::Poisoned,
            input.len(),
        );
    }
}

#[test]
fn model_page_rejects_missing_reordered_or_malformed_required_facts() {
    let valid = record("id", r#"["low"]"#, "low", "");
    let malformed_records = [
        valid.replace("\"model\":\"model-id\",", ""),
        valid.replace(
            "\"id\":\"id\",\"opaqueBeforeModel\":{\"nested\":[\"\"]},\"model\":\"model-id\"",
            "\"model\":\"model-id\",\"id\":\"id\"",
        ),
        valid.replace(
            "\"defaultReasoningEffort\":\"low\"",
            "\"defaultReasoningEffort\":\"\"",
        ),
        valid.replace("\"hidden\":false", "\"hidden\":null"),
        valid.replace(
            r#""supportedReasoningEfforts":["low"]"#,
            r#""supportedReasoningEfforts":[""]"#,
        ),
    ];

    for record in malformed_records {
        let input = format!("{{\"id\":51,\"result\":{{\"data\":[{record}],\"nextCursor\":null}}}}");
        assert_ingress_error(
            decode(&input, 3, IncomingJsonExpectation::ModelList { id: 51 }),
            ForegroundIngressError::MalformedResponse,
            IncomingJsonExpectation::Poisoned,
            input.len(),
        );
    }
}
