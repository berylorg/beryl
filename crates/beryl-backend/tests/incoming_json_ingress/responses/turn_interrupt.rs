use beryl_backend::{
    JsonRpcErrorVerdict,
    lifecycle_test_support::{IncomingJsonExpectation, IncomingJsonTestOutcome},
};

use crate::support::{FRAGMENT_SIZES, decode};

#[test]
fn pinned_absent_data_interrupt_codes_are_pre_core_rejections() {
    for code in [-32_600, -32_603] {
        let input = format!(
            r#"{{"error":{{"code":{code},"message":"diagnostic is not authority"}},"id":71}}"#
        );
        for &fragment_bytes in FRAGMENT_SIZES {
            let decoded = decode(
                &input,
                fragment_bytes,
                IncomingJsonExpectation::TurnInterrupt { id: 71 },
            );
            assert!(matches!(
                decoded.outcome,
                IncomingJsonTestOutcome::Rejection {
                    code: actual,
                    data_was_present: false,
                    verdict: Some(JsonRpcErrorVerdict::RejectedBeforeCoreInterrupt),
                    ..
                } if actual == code
            ));
            assert_eq!(decoded.expectation_after, IncomingJsonExpectation::Idle);
            assert_eq!(decoded.consumed_input_bytes, input.len());
        }
    }
}

#[test]
fn interrupt_pre_core_verdict_rejects_data_and_unrecognized_codes() {
    let cases = [
        r#"{"error":{"code":-32600,"data":null,"message":"ignored"},"id":72}"#,
        r#"{"error":{"code":-32603,"data":{"opaque":true},"message":"ignored"},"id":72}"#,
        r#"{"error":{"code":-32601,"message":"ignored"},"id":72}"#,
        r#"{"error":{"code":-32000,"message":"ignored"},"id":72}"#,
    ];
    for input in cases {
        let decoded = decode(input, 1, IncomingJsonExpectation::TurnInterrupt { id: 72 });
        assert!(matches!(
            decoded.outcome,
            IncomingJsonTestOutcome::Rejection { verdict: None, .. }
        ));
        assert_eq!(decoded.expectation_after, IncomingJsonExpectation::Idle);
        assert_eq!(decoded.consumed_input_bytes, input.len());
    }
}
