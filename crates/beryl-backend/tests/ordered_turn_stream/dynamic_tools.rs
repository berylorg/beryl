use beryl_backend::{
    DynamicToolArgumentContainer as Container, DynamicToolArgumentControl as Control,
    DynamicToolArgumentScalarKind as ScalarKind, DynamicToolCall, DynamicToolCallAbandonReason,
    DynamicToolCallError, DynamicToolCallResponseDisposition, DynamicToolCallSchemaError,
    ManagedBackendError, OrderedTurnStreamProgress, OrderedTurnStreamSubmitCause,
    lifecycle_test_support::decode_provider_json_for_test,
};

use super::support::{
    DynamicRequestIdTrace, FailOn, Trace, diagnostics, sink_harness, trace_snapshot,
};

struct SuccessfulCase {
    call: DynamicToolCall,
    trace: Trace,
    pool: beryl_stream::PagePool,
}

fn start_success(raw: String, fragmented: bool, page_capacity: usize) -> SuccessfulCase {
    let mut harness = sink_harness(page_capacity, None);
    let trace = harness.trace.clone();
    let pool = harness.pool.clone();
    let fragment_bytes = if fragmented { 1 } else { 1024 };
    assert_eq!(
        decode_provider_json_for_test(raw.as_bytes(), fragment_bytes, harness.sink.as_mut())
            .unwrap(),
        OrderedTurnStreamProgress::Progress
    );
    let call = harness.dynamic_calls.try_recv().unwrap();
    SuccessfulCase {
        call,
        trace: trace_snapshot(&trace),
        pool,
    }
}

fn run_error(
    raw: String,
    failure: Option<(FailOn, OrderedTurnStreamSubmitCause)>,
) -> (ManagedBackendError, Trace, Option<DynamicToolCall>, usize) {
    let mut harness = sink_harness(5, failure);
    let trace = harness.trace.clone();
    let pool = harness.pool.clone();
    let error =
        decode_provider_json_for_test(raw.as_bytes(), 1, harness.sink.as_mut()).unwrap_err();
    let call = harness.dynamic_calls.try_recv().ok();
    let trace = trace_snapshot(&trace);
    let leased = diagnostics(&pool).leased;
    (error, trace, call, leased)
}

fn scalar_payloads(trace: &Trace) -> Vec<(ScalarKind, Vec<u8>)> {
    let mut scalars: Vec<(ScalarKind, Vec<u8>)> = Vec::new();
    for fragment in &trace.dynamic_fragments {
        if fragment.offset == 0 {
            scalars.push((fragment.kind, Vec::new()));
        }
        let (kind, bytes) = scalars.last_mut().expect("fragment starts at offset zero");
        assert_eq!(*kind, fragment.kind);
        assert_eq!(fragment.offset, bytes.len() as u64);
        bytes.extend_from_slice(&fragment.bytes);
    }
    scalars
}

#[test]
fn fragmented_canonical_call_forwards_exact_syntax_and_optional_namespace() {
    let raw = r#"{"method":"item/tool/call","id":88,"params":{"threadId":"thread_1","turnId":"turn_1","callId":"call_1","namespace":"beryl","tool":"lookup","arguments":{"alpha":"h\u00e9llo","count":-12.5e2,"flags":[true,null]}}}"#.to_string();
    let case = start_success(raw, true, 5);
    assert_eq!(case.call.request_id().as_i64(), Some(88));
    assert_eq!(case.call.thread_id().as_str(), "thread_1");
    assert_eq!(case.call.turn_id().as_str(), "turn_1");
    assert_eq!(case.call.call_id().as_str(), "call_1");
    assert_eq!(case.call.namespace(), Some("beryl"));
    assert_eq!(case.call.tool().as_str(), "lookup");
    assert!(case.call.is_sealed());
    assert_eq!(case.trace.dynamic_begins.len(), 1);
    assert_eq!(
        case.trace.dynamic_begins[0].request_id,
        DynamicRequestIdTrace::Integer(88)
    );
    assert_eq!(case.trace.dynamic_seals, 1);
    assert!(case.trace.dynamic_abandons.is_empty());
    assert_eq!(case.trace.dynamic_leased_at_seal, [0]);
    assert_eq!(diagnostics(&case.pool).high_water, 1);
    assert_eq!(diagnostics(&case.pool).leased, 0);
    assert_eq!(
        case.trace.dynamic_controls,
        vec![
            Control::ContainerStart(Container::Object),
            Control::ScalarStart(ScalarKind::ObjectName),
            Control::ScalarEnd(ScalarKind::ObjectName),
            Control::ScalarStart(ScalarKind::String),
            Control::ScalarEnd(ScalarKind::String),
            Control::ScalarStart(ScalarKind::ObjectName),
            Control::ScalarEnd(ScalarKind::ObjectName),
            Control::ScalarStart(ScalarKind::Number),
            Control::ScalarEnd(ScalarKind::Number),
            Control::ScalarStart(ScalarKind::ObjectName),
            Control::ScalarEnd(ScalarKind::ObjectName),
            Control::ContainerStart(Container::Array),
            Control::Boolean(true),
            Control::Null,
            Control::ContainerEnd(Container::Array),
            Control::ContainerEnd(Container::Object),
        ]
    );
    assert_eq!(
        scalar_payloads(&case.trace),
        vec![
            (ScalarKind::ObjectName, b"alpha".to_vec()),
            (ScalarKind::String, "héllo".as_bytes().to_vec()),
            (ScalarKind::ObjectName, b"count".to_vec()),
            (ScalarKind::Number, b"-12.5e2".to_vec()),
            (ScalarKind::ObjectName, b"flags".to_vec()),
        ]
    );
}

#[test]
fn string_request_id_and_absent_namespace_remain_exact() {
    let raw = r#"{"method":"item/tool/call","id":"request_1","params":{"threadId":"thread_1","turnId":"turn_1","callId":"call_1","tool":"lookup","arguments":[]}}"#.to_string();
    let case = start_success(raw, false, 7);
    assert_eq!(case.call.request_id().as_str(), Some("request_1"));
    assert_eq!(case.call.namespace(), None);
    assert_eq!(
        case.trace.dynamic_begins[0].request_id,
        DynamicRequestIdTrace::String("request_1".to_string())
    );
    assert_eq!(
        case.trace.dynamic_controls,
        [
            Control::ContainerStart(Container::Array),
            Control::ContainerEnd(Container::Array)
        ]
    );
    assert!(case.trace.dynamic_fragments.is_empty());
    assert!(case.call.is_sealed());
}

#[test]
fn envelope_reorder_duplicates_missing_fields_and_invalid_types_fail_closed() {
    let cases = [
        (
            r#"{"method":"item/tool/call","params":{"threadId":"t","turnId":"u","callId":"c","tool":"x","arguments":{}}}"#,
            DynamicToolCallSchemaError::DuplicateField,
        ),
        (
            r#"{"method":"item/tool/call","id":1,"params":{"turnId":"u","threadId":"t","callId":"c","tool":"x","arguments":{}}}"#,
            DynamicToolCallSchemaError::ReorderedField,
        ),
        (
            r#"{"method":"item/tool/call","id":1,"params":{"threadId":"t","threadId":"t2","turnId":"u","callId":"c","tool":"x","arguments":{}}}"#,
            DynamicToolCallSchemaError::DuplicateField,
        ),
        (
            r#"{"method":"item/tool/call","id":1,"params":{"threadId":"t","turnId":"u","callId":"c","arguments":{},"tool":"x"}}"#,
            DynamicToolCallSchemaError::ReorderedField,
        ),
        (
            r#"{"method":"item/tool/call","id":1,"params":{"threadId":"t","turnId":"u","callId":"c","arguments":{}}}"#,
            DynamicToolCallSchemaError::ReorderedField,
        ),
        (
            r#"{"method":"item/tool/call","id":1,"params":{"threadId":7,"turnId":"u","callId":"c","tool":"x","arguments":{}}}"#,
            DynamicToolCallSchemaError::WrongType,
        ),
        (
            r#"{"method":"item/tool/call","id":true,"params":{"threadId":"t","turnId":"u","callId":"c","tool":"x","arguments":{}}}"#,
            DynamicToolCallSchemaError::WrongType,
        ),
    ];
    for (raw, expected) in cases {
        let (error, trace, call, leased) = run_error(raw.to_string(), None);
        assert!(matches!(
            error,
            ManagedBackendError::DynamicToolCall {
                source: DynamicToolCallError::Schema(actual),
                ..
            } if actual == expected
        ));
        assert!(call.is_none());
        assert!(trace.dynamic_begins.is_empty());
        assert!(trace.dynamic_abandons.is_empty());
        assert_eq!(leased, 0);
    }
}

#[test]
fn late_identity_and_depth_failure_abandon_after_returning_the_only_page() {
    let late = r#"{"method":"item/tool/call","id":1,"params":{"threadId":"t","turnId":"u","callId":"c","tool":"x","arguments":{"value":"abcdef"},"callId":"late"}}"#;
    let (error, trace, call, leased) = run_error(late.to_string(), None);
    assert!(matches!(
        error,
        ManagedBackendError::DynamicToolCall {
            source: DynamicToolCallError::Schema(DynamicToolCallSchemaError::DuplicateField),
            ..
        }
    ));
    let call = call.expect("selection occurred before arguments");
    assert!(!call.is_sealed());
    assert_eq!(
        call.response_disposition(),
        DynamicToolCallResponseDisposition::ResponseRequired
    );
    assert_eq!(
        trace.dynamic_abandons,
        [DynamicToolCallAbandonReason::SchemaFailure]
    );
    assert_eq!(trace.dynamic_leased_at_abandon, [0]);
    assert_eq!(leased, 0);

    let mut arguments = String::new();
    for _ in 0..129 {
        arguments.push('[');
    }
    for _ in 0..129 {
        arguments.push(']');
    }
    let raw = format!(
        r#"{{"method":"item/tool/call","id":1,"params":{{"threadId":"t","turnId":"u","callId":"c","tool":"x","arguments":{arguments}}}}}"#
    );
    let (error, trace, call, leased) = run_error(raw, None);
    assert!(matches!(
        error,
        ManagedBackendError::DynamicToolCall {
            source: DynamicToolCallError::Schema(
                DynamicToolCallSchemaError::StructuredDepthExceeded
            ),
            ..
        }
    ));
    assert!(!call.unwrap().is_sealed());
    assert_eq!(
        trace.dynamic_abandons,
        [DynamicToolCallAbandonReason::SchemaFailure]
    );
    assert_eq!(leased, 0);
}

#[test]
fn fragment_cancellation_returns_page_and_abandons_exact_call() {
    let raw = r#"{"method":"item/tool/call","id":1,"params":{"threadId":"t","turnId":"u","callId":"c","tool":"x","arguments":{"value":"abcdefghijk"}}}"#.to_string();
    let (error, trace, call, leased) = run_error(
        raw,
        Some((FailOn::Fragment, OrderedTurnStreamSubmitCause::Cancelled)),
    );
    assert!(matches!(
        error,
        ManagedBackendError::DynamicToolCall {
            source: DynamicToolCallError::Submit(OrderedTurnStreamSubmitCause::Cancelled),
            ..
        }
    ));
    assert!(!call.unwrap().is_sealed());
    assert_eq!(
        trace.dynamic_abandons,
        [DynamicToolCallAbandonReason::Cancelled]
    );
    assert_eq!(trace.dynamic_leased_at_abandon, [0]);
    assert_eq!(leased, 0);
}

#[path = "dynamic_tools/failures.rs"]
mod failures;
#[path = "dynamic_tools/pages.rs"]
mod pages;
