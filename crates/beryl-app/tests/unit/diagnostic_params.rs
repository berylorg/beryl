use super::*;
use serde_json::json;

#[test]
fn diagnostic_arguments_require_exact_typed_identity_and_bounded_observation() {
    let thread = beryl_model::SyndicThreadId::from_bytes([0xA2; 16]);
    assert_eq!(switch_thread(&json!({"threadId":thread})), Ok(thread));
    for value in [
        json!({}),
        json!({"threadId":null}),
        json!({"threadId":"invalid"}),
        json!({"threadId":"x".repeat(65)}),
        json!({"threadId":thread,"fallback":thread}),
    ] {
        assert_eq!(switch_thread(&value), Err(()));
    }
    assert_eq!(observation_limit(&json!({})), Ok(32));
    assert_eq!(observation_limit(&json!({"limit":0})), Ok(0));
    assert_eq!(observation_limit(&json!({"limit":64})), Ok(64));
    for value in [
        json!({"limit":65}),
        json!({"limit":-1}),
        json!({"limit":"4"}),
        json!({"limit":4,"body":true}),
    ] {
        assert_eq!(observation_limit(&value), Err(()));
    }
}
