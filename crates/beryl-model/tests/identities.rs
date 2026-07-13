use std::str::FromStr;

use beryl_model::{
    BerylHomeId, CommandId, IdentityParseError, ResolutionIntentId, RuntimeId, SyndicDraftId,
    SyndicThreadId, SyndicTurnId, WindowId,
};

#[test]
fn identities_round_trip_through_text_and_serde() {
    let identity = SyndicThreadId::from_bytes([0xab; 16]);
    let text = "syndic_thread_abababababababababababababababab";

    assert_eq!(identity.to_string(), text);
    assert_eq!(SyndicThreadId::from_str(text), Ok(identity));
    assert_eq!(
        serde_json::to_string(&identity).unwrap(),
        format!("\"{text}\"")
    );
    assert_eq!(
        serde_json::from_str::<SyndicThreadId>(&format!("\"{text}\"")).unwrap(),
        identity
    );
}

#[test]
fn identity_types_have_distinct_text_prefixes() {
    let bytes = [7; 16];

    assert!(
        BerylHomeId::from_bytes(bytes)
            .to_string()
            .starts_with("home_")
    );
    assert!(
        WindowId::from_bytes(bytes)
            .to_string()
            .starts_with("window_")
    );
    assert!(
        RuntimeId::from_bytes(bytes)
            .to_string()
            .starts_with("runtime_")
    );
    assert!(
        CommandId::from_bytes(bytes)
            .to_string()
            .starts_with("command_")
    );
    assert!(
        SyndicTurnId::from_bytes(bytes)
            .to_string()
            .starts_with("syndic_turn_")
    );
    assert!(
        SyndicDraftId::from_bytes(bytes)
            .to_string()
            .starts_with("syndic_draft_")
    );
    assert!(
        ResolutionIntentId::from_bytes(bytes)
            .to_string()
            .starts_with("resolution_intent_")
    );
    assert_ne!(
        BerylHomeId::from_bytes(bytes).to_string(),
        WindowId::from_bytes(bytes).to_string()
    );
}

#[test]
fn malformed_or_oversized_identity_text_is_rejected() {
    assert_eq!(
        RuntimeId::from_str("root_00000000000000000000000000000000"),
        Err(IdentityParseError::WrongPrefix {
            expected: "runtime_"
        })
    );
    assert_eq!(
        RuntimeId::from_str("runtime_00"),
        Err(IdentityParseError::WrongLength {
            expected: 32,
            actual: 2
        })
    );
    assert_eq!(
        RuntimeId::from_str("runtime_0000000000000000000000000000000A"),
        Err(IdentityParseError::InvalidHex { index: 31 })
    );
    assert!(RuntimeId::from_str(&format!("runtime_{}", "0".repeat(34))).is_err());
}

#[test]
fn serde_validation_cannot_be_bypassed() {
    let malformed = "\"runtime_0000000000000000000000000000000A\"";
    assert!(serde_json::from_str::<RuntimeId>(malformed).is_err());
}
