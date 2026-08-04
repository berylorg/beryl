use super::*;

const CALL: &str = r#"{"method":"item/tool/call","id":61,"params":{"threadId":"thread","turnId":"turn","callId":"call","tool":"lookup","arguments":{"value":"payload"}}}"#;

#[test]
fn cancellation_before_begin_acknowledgement_retains_no_call_or_page() {
    let (error, trace, call, leased) = run_error(
        CALL.to_string(),
        Some((
            FailOn::DynamicBegin,
            OrderedTurnStreamSubmitCause::Cancelled,
        )),
    );

    assert!(matches!(
        error,
        ManagedBackendError::DynamicToolCall {
            source: DynamicToolCallError::Submit(OrderedTurnStreamSubmitCause::Cancelled),
            ..
        }
    ));
    assert!(call.is_none());
    assert_eq!(trace.dynamic_begins.len(), 1);
    assert!(trace.dynamic_abandons.is_empty());
    assert_eq!(leased, 0);
}

#[test]
fn cancellation_before_seal_abandons_the_exact_unsealed_call() {
    let (error, trace, call, leased) = run_error(
        CALL.to_string(),
        Some((FailOn::DynamicSeal, OrderedTurnStreamSubmitCause::Cancelled)),
    );

    assert!(matches!(
        error,
        ManagedBackendError::DynamicToolCall {
            source: DynamicToolCallError::Submit(OrderedTurnStreamSubmitCause::Cancelled),
            ..
        }
    ));
    let call = call.expect("begin was acknowledged before seal cancellation");
    assert!(!call.is_sealed());
    assert_eq!(trace.dynamic_seals, 1);
    assert_eq!(
        trace.dynamic_abandons,
        [DynamicToolCallAbandonReason::Cancelled]
    );
    assert_eq!(trace.dynamic_leased_at_abandon, [0]);
    assert_eq!(leased, 0);
}

#[test]
fn full_page_capacity_abandons_without_retaining_a_fragment() {
    let (error, trace, call, leased) = run_error(
        CALL.to_string(),
        Some((
            FailOn::DynamicAcquirePage,
            OrderedTurnStreamSubmitCause::CapacityFull,
        )),
    );

    assert!(matches!(
        error,
        ManagedBackendError::DynamicToolCall {
            source: DynamicToolCallError::Submit(OrderedTurnStreamSubmitCause::CapacityFull),
            ..
        }
    ));
    assert!(
        !call
            .expect("begin was acknowledged before page denial")
            .is_sealed()
    );
    assert!(trace.dynamic_fragments.is_empty());
    assert_eq!(
        trace.dynamic_abandons,
        [DynamicToolCallAbandonReason::CapacityFull]
    );
    assert_eq!(trace.dynamic_leased_at_abandon, [0]);
    assert_eq!(leased, 0);
}

#[test]
fn control_receiver_cancellation_abandons_before_scalar_allocation() {
    let (error, trace, call, leased) = run_error(
        CALL.to_string(),
        Some((
            FailOn::DynamicControl,
            OrderedTurnStreamSubmitCause::ReceiverLost,
        )),
    );

    assert!(matches!(
        error,
        ManagedBackendError::DynamicToolCall {
            source: DynamicToolCallError::Submit(OrderedTurnStreamSubmitCause::ReceiverLost),
            ..
        }
    ));
    assert!(
        !call
            .expect("begin was acknowledged before receiver loss")
            .is_sealed()
    );
    assert!(trace.dynamic_fragments.is_empty());
    assert_eq!(
        trace.dynamic_abandons,
        [DynamicToolCallAbandonReason::ReceiverLost]
    );
    assert_eq!(trace.dynamic_leased_at_abandon, [0]);
    assert_eq!(leased, 0);
}
