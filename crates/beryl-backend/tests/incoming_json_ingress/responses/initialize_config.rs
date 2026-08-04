use beryl_backend::{
    BoundedResponseResult, ForegroundIngressError, InitializePlatform,
    lifecycle_test_support::{IncomingJsonExpectation, IncomingJsonTestOutcome},
};

use crate::support::{FRAGMENT_SIZES, assert_ingress_error, decode};

const REQUIRED_FEATURE_ORIGINS: &str = r#""origins":{"features.multi_agent_v2.enabled":{"name":{"type":"sessionFlags"},"version":"0"},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{"name":{"type":"sessionFlags"},"version":"0"}}"#;
const REQUIRED_FEATURE_CONFIG: &str =
    r#","features":{"multi_agent_v2":{"enabled":true,"expose_spawn_agent_model_overrides":true}}"#;

#[test]
fn initialize_retains_only_the_product_and_closed_platform_after_full_discard() {
    let home = format!("/tmp/{}", "h".repeat(48 * 1024));
    let input = format!(
        "{{\"id\":41,\"result\":{{\"userAgent\":\"beryl/0.146.0 client metadata\",\"codexHome\":\"{home}\",\"platformFamily\":\"unix\",\"platformOs\":\"linux\"}}}}"
    );

    for &fragment_bytes in FRAGMENT_SIZES {
        let decoded = decode(
            &input,
            fragment_bytes,
            IncomingJsonExpectation::Initialize { id: 41 },
        );
        match decoded.outcome {
            IncomingJsonTestOutcome::Response {
                id,
                result: BoundedResponseResult::Initialize(response),
            } => {
                assert_eq!(id, 41);
                assert_eq!(response.user_agent_product(), "beryl/0.146.0");
                assert_eq!(response.platform(), InitializePlatform::WslLinux);
            }
            other => panic!("unexpected initialize result: {other:?}"),
        }
        assert_eq!(decoded.expectation_after, IncomingJsonExpectation::Idle);
        assert_eq!(decoded.consumed_input_bytes, input.len());
        assert!(decoded.maximum_buffered_input_bytes <= fragment_bytes.max(1));
    }
}

#[test]
fn initialize_rejects_reordered_or_out_of_domain_required_facts_after_consumption() {
    let cases = [
        r#"{"id":7,"result":{"codexHome":"x","userAgent":"beryl/0.146.0","platformFamily":"windows","platformOs":"windows"}}"#.to_string(),
        r#"{"id":7,"result":{"userAgent":"beryl/0.146.0","codexHome":"x","platformFamily":"unix","platformOs":"windows"}}"#.to_string(),
        format!(
            "{{\"id\":7,\"result\":{{\"userAgent\":\"{}\",\"codexHome\":\"x\",\"platformFamily\":\"windows\",\"platformOs\":\"windows\"}}}}",
            "x".repeat(257)
        ),
    ];

    for input in cases {
        assert_ingress_error(
            decode(&input, 3, IncomingJsonExpectation::Initialize { id: 7 }),
            ForegroundIngressError::MalformedResponse,
            IncomingJsonExpectation::Poisoned,
            input.len(),
        );
    }
}

#[test]
fn config_read_retains_two_optional_identities_and_discards_incidental_values() {
    let incidental = "z".repeat(64 * 1024);
    let input = format!(
        "{{\"id\":42,\"result\":{{\"config\":{{\"model\":\"gpt-5\",\"opaque\":{{\"deep\":[\"{incidental}\"]}},\"model_reasoning_effort\":\"high\",\"tail\":true{REQUIRED_FEATURE_CONFIG}}},{REQUIRED_FEATURE_ORIGINS},\"layers\":[{{\"opaque\":\"{incidental}\"}}]}}}}"
    );

    for &fragment_bytes in FRAGMENT_SIZES {
        let decoded = decode(
            &input,
            fragment_bytes,
            IncomingJsonExpectation::ConfigRead { id: 42 },
        );
        match decoded.outcome {
            IncomingJsonTestOutcome::Response {
                id,
                result: BoundedResponseResult::ConfigRead(response),
            } => {
                assert_eq!(id, 42);
                assert_eq!(response.defaults().model(), Some("gpt-5"));
                assert_eq!(response.defaults().model_reasoning_effort(), Some("high"));
                assert!(response.defaults().proves_spawn_agent_model_overrides());
            }
            other => panic!("unexpected config result: {other:?}"),
        }
        assert_eq!(decoded.expectation_after, IncomingJsonExpectation::Idle);
        assert_eq!(decoded.consumed_input_bytes, input.len());
    }
}

#[test]
fn config_read_accepts_null_defaults_and_rejects_aliases_or_bounds() {
    let terminal = format!(
        "{{\"id\":9,\"result\":{{\"config\":{{\"model\":null,\"model_reasoning_effort\":null{REQUIRED_FEATURE_CONFIG}}},{REQUIRED_FEATURE_ORIGINS}}}}}"
    );
    let decoded = decode(&terminal, 1, IncomingJsonExpectation::ConfigRead { id: 9 });
    match decoded.outcome {
        IncomingJsonTestOutcome::Response {
            result: BoundedResponseResult::ConfigRead(response),
            ..
        } => {
            assert_eq!(response.defaults().model(), None);
            assert_eq!(response.defaults().model_reasoning_effort(), None);
        }
        other => panic!("unexpected null config result: {other:?}"),
    }

    let malformed = [
        format!(
            "{{\"id\":9,\"result\":{{\"config\":{{\"model\":null,\"modelReasoningEffort\":\"high\"{REQUIRED_FEATURE_CONFIG}}},{REQUIRED_FEATURE_ORIGINS}}}}}"
        ),
        format!(
            "{{\"id\":9,\"result\":{{\"config\":{{\"model\":\"{}\",\"model_reasoning_effort\":null{REQUIRED_FEATURE_CONFIG}}},{REQUIRED_FEATURE_ORIGINS}}}}}",
            "m".repeat(257)
        ),
        format!(
            "{{\"id\":9,\"result\":{{\"config\":{{\"model_reasoning_effort\":\"high\",\"model\":\"gpt-5\"{REQUIRED_FEATURE_CONFIG}}},{REQUIRED_FEATURE_ORIGINS}}}}}"
        ),
    ];
    for input in malformed {
        assert_ingress_error(
            decode(&input, 2, IncomingJsonExpectation::ConfigRead { id: 9 }),
            ForegroundIngressError::MalformedResponse,
            IncomingJsonExpectation::Poisoned,
            input.len(),
        );
    }
}

#[test]
fn config_read_requires_true_feature_values_and_session_flags_origins() {
    let valid = format!(
        "{{\"id\":10,\"result\":{{\"config\":{{\"model\":null,\"model_reasoning_effort\":null{REQUIRED_FEATURE_CONFIG}}},{REQUIRED_FEATURE_ORIGINS}}}}}"
    );
    let malformed = [
        valid.replacen(REQUIRED_FEATURE_CONFIG, "", 1),
        valid.replacen("\"enabled\":true", "\"enabled\":false", 1),
        valid.replacen(
            "\"enabled\":true,\"expose_spawn_agent_model_overrides\":true",
            "\"expose_spawn_agent_model_overrides\":true,\"enabled\":true",
            1,
        ),
        valid.replacen("\"type\":\"sessionFlags\"", "\"type\":\"user\"", 1),
    ];
    for input in malformed {
        assert_ingress_error(
            decode(&input, 1, IncomingJsonExpectation::ConfigRead { id: 10 }),
            ForegroundIngressError::MalformedResponse,
            IncomingJsonExpectation::Poisoned,
            input.len(),
        );
    }
}

#[test]
fn config_read_discards_unrelated_feature_fields_without_weakening_the_proof() {
    let input = format!(
        "{{\"id\":11,\"result\":{{\"config\":{{\"model\":null,\"model_reasoning_effort\":null,\"features\":{{\"legacy\":false,\"multi_agent_v2\":{{\"before\":null,\"enabled\":true,\"middle\":[],\"expose_spawn_agent_model_overrides\":true,\"after\":{{}}}},\"tail\":true}}}},{REQUIRED_FEATURE_ORIGINS}}}}}"
    );
    let decoded = decode(&input, 1, IncomingJsonExpectation::ConfigRead { id: 11 });
    match decoded.outcome {
        IncomingJsonTestOutcome::Response {
            result: BoundedResponseResult::ConfigRead(response),
            ..
        } => assert!(response.defaults().proves_spawn_agent_model_overrides()),
        other => panic!("unexpected config result: {other:?}"),
    }
}
