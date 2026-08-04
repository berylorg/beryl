//! Fixed-page reuse and release coverage for streamed dynamic-tool arguments.

use beryl_backend::{
    DynamicToolArgumentScalarKind as ScalarKind, DynamicToolCallAbandonReason,
    DynamicToolCallResponseDisposition,
    lifecycle_test_support::decode_provider_transport_loss_for_test,
};

use super::super::support::{diagnostics, sink_harness, trace_snapshot};
use super::start_success;

#[test]
fn multi_megabyte_argument_stream_recycles_one_fixed_page() {
    const PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
    let payload = "x".repeat(PAYLOAD_BYTES);
    let raw = format!(
        r#"{{"method":"item/tool/call","id":31,"params":{{"threadId":"t","turnId":"u","callId":"c","tool":"x","arguments":{{"payload":"{payload}"}}}}}}"#
    );
    let case = start_success(raw, false, 4 * 1024);
    let streamed_string_bytes = case
        .trace
        .dynamic_fragments
        .iter()
        .filter(|fragment| fragment.kind == ScalarKind::String)
        .map(|fragment| fragment.bytes.len())
        .sum::<usize>();

    assert_eq!(streamed_string_bytes, PAYLOAD_BYTES);
    assert!(case.call.is_sealed());
    assert_eq!(case.trace.dynamic_seals, 1);
    assert_eq!(case.trace.dynamic_leased_at_seal, [0]);
    assert_eq!(diagnostics(&case.pool).high_water, 1);
    assert_eq!(diagnostics(&case.pool).leased, 0);
}

#[test]
fn transport_loss_mid_argument_releases_page_before_abandonment() {
    let prefix = br#"{"method":"item/tool/call","id":32,"params":{"threadId":"t","turnId":"u","callId":"c","tool":"x","arguments":{"payload":"partial"#;
    let mut harness = sink_harness(5, None);
    let trace = harness.trace.clone();
    let pool = harness.pool.clone();
    assert!(decode_provider_transport_loss_for_test(prefix, 7, harness.sink.as_mut()).is_err());
    let call = harness.dynamic_calls.try_recv().unwrap();
    let trace = trace_snapshot(&trace);
    assert!(!call.is_sealed());
    assert_eq!(
        call.response_disposition(),
        DynamicToolCallResponseDisposition::ResponseRequired
    );
    assert_eq!(
        trace.dynamic_abandons,
        [DynamicToolCallAbandonReason::TransportLost]
    );
    assert_eq!(trace.dynamic_leased_at_abandon, [0]);
    assert_eq!(diagnostics(&pool).leased, 0);
}
