#![cfg(feature = "test-faults")]

use beryl_app::cas_projection::*;
use beryl_model::*;

fn harness() -> ContextObservationTestHarness {
    ContextObservationTestHarness::new(
        RuntimeId::from_bytes([79; 16]),
        CasProcessGeneration::new(79).unwrap(),
    )
}

fn register(
    harness: &mut ContextObservationTestHarness,
    byte: u8,
    model: Option<&str>,
) -> ContextProjectionTestHandle {
    harness.register(
        SyndicThreadId::from_bytes([byte; 16]),
        CasThreadId::new(format!("thread-{byte}")).unwrap(),
        model,
        BindingRevision::new(1).unwrap(),
    )
}

fn usage(byte: u8, input: i64) -> String {
    let breakdown = format!(
        r#"{{"totalTokens":5,"inputTokens":{input},"cachedInputTokens":0,"cacheWriteInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0}}"#
    );
    format!(
        r#"{{"method":"thread/tokenUsage/updated","params":{{"threadId":"thread-{byte}","turnId":"turn-{byte}","tokenUsage":{{"total":{breakdown},"last":{breakdown},"modelContextWindow":200}}}}}}"#
    )
}

#[test]
fn proven_checked_next_terminal_transition_preserves_the_original_observation() {
    let mut harness = harness();
    let mut projection = register(&mut harness, 1, Some("model"));
    harness.observe(&projection, &usage(1, 50));
    let original = harness.read(&projection).unwrap();
    harness.advance(&mut projection, BindingRevision::new(2).unwrap(), true);
    assert_eq!(harness.read(&projection), Some(original));
}

#[test]
fn unproven_revision_drift_and_skipped_revisions_clear_usage() {
    for proven in [false, true] {
        let mut harness = harness();
        let mut projection = register(&mut harness, 2, Some("model"));
        harness.observe(&projection, &usage(2, 50));
        let next = BindingRevision::new(2).unwrap();
        let revision = if proven {
            next.checked_next().unwrap()
        } else {
            next
        };
        harness.advance(&mut projection, revision, proven);
        assert!(harness.read(&projection).is_none());
    }
}

#[test]
fn unavailable_usage_replaces_previous_value_and_retirement_clears_the_session() {
    let mut harness = harness();
    let projection = register(&mut harness, 3, Some("model"));
    harness.observe(&projection, &usage(3, 50));
    assert!(harness.read(&projection).unwrap().usage().is_some());
    harness.observe(&projection, &usage(3, -1));
    assert!(harness.read(&projection).unwrap().usage().is_none());
    harness.retire();
    assert!(harness.read(&projection).is_none());
    assert_eq!(harness.interest(), (None, None, None));
}

#[test]
fn replacing_the_loaded_session_cannot_reuse_usage_or_quota() {
    let mut harness = harness();
    let projection = register(&mut harness, 4, Some("model"));
    harness.observe(&projection, &usage(4, 50));
    harness.observe(
        &projection,
        r#"{"method":"account/rateLimits/updated","params":{"rateLimits":{}}}"#,
    );
    harness.unregister(projection);
    let replacement = register(&mut harness, 4, Some("model"));
    assert!(harness.read(&replacement).is_none());
    assert!(harness.interest().2.is_none());
}

#[test]
fn all_loaded_projections_must_agree_and_membership_changes_invalidate_quota() {
    let mut harness = harness();
    let first = register(&mut harness, 5, Some("model"));
    let before = harness.interest();
    assert_eq!(before.1.as_deref(), Some("model"));
    harness.observe(
        &first,
        r#"{"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"model"}}}"#,
    );
    assert_eq!(harness.interest().2, before.0);
    let second = register(&mut harness, 6, Some("other-model"));
    let conflict = harness.interest();
    assert!(conflict.0 > before.0);
    assert!(conflict.1.is_none() && conflict.2.is_none());
    harness.unregister(second);
    assert_eq!(harness.interest().1.as_deref(), Some("model"));
    assert!(harness.interest().2.is_none());
}

#[test]
fn missing_empty_or_oversized_authenticated_models_make_interest_unavailable() {
    for model in [None, Some(""), Some("x".repeat(257).as_str())] {
        let mut harness = harness();
        let _first = register(&mut harness, 7, Some("model"));
        let _second = register(&mut harness, 8, model);
        assert!(harness.interest().1.is_none());
    }
}

#[test]
fn held_observation_rejects_retirement_replacement_and_identical_value_aba() {
    let mut harness = harness();
    let projection = register(&mut harness, 9, Some("model"));
    harness.observe(&projection, &usage(9, 50));
    let held = harness.hold(&projection);
    assert!(held());
    harness.observe(&projection, &usage(9, -1));
    harness.observe(&projection, &usage(9, 50));
    assert!(!held());
    let held = harness.hold(&projection);
    harness.unregister(projection);
    let replacement = register(&mut harness, 9, Some("model"));
    harness.observe(&replacement, &usage(9, 50));
    assert!(!held());
    let held = harness.hold(&replacement);
    harness.retire();
    assert!(!held());
}
