use super::*;
use beryl_model::GeneratedSourceIncarnation;

#[test]
fn distinct_original_incarnations_change_fingerprints_with_equal_dispatch_counters() {
    let first = MediaHome::with_source_incarnation(
        "generated-origin-first",
        Some(GeneratedSourceIncarnation::from_bytes([0x47; 32])),
    );
    let second = MediaHome::with_source_incarnation(
        "generated-origin-second",
        Some(GeneratedSourceIncarnation::from_bytes([0x48; 32])),
    );
    for home in [&first, &second] {
        home.frame(
            20,
            false,
            ProviderImageGenerationStatusV1::InProgress,
            None,
            None,
            6,
        );
    }
    let first_awaiting = first.proof(20);
    let second_awaiting = second.proof(20);
    assert_eq!(
        first_awaiting.execution_snapshot(),
        second_awaiting.execution_snapshot()
    );
    assert_eq!(
        first_awaiting.active_cas_turn(),
        second_awaiting.active_cas_turn()
    );
    assert_ne!(
        first_awaiting.source_fingerprint(),
        second_awaiting.source_fingerprint()
    );
    for home in [&first, &second] {
        home.frame(
            20,
            true,
            ProviderImageGenerationStatusV1::Completed,
            None,
            Some(ProviderTextV1::inline("C:\\original.png")),
            7,
        );
    }
    assert_eq!(
        first.proof(20).source_incarnation(),
        first_awaiting.source_incarnation()
    );
    assert_eq!(
        second.proof(20).source_incarnation(),
        second_awaiting.source_incarnation()
    );
    assert_ne!(
        first.proof(20).source_fingerprint(),
        second.proof(20).source_fingerprint()
    );
}

#[test]
fn missing_historical_incarnation_allows_only_lost_source_resolution() {
    let f = MediaHome::with_source_incarnation("generated-origin-absent", None);
    f.frame(
        20,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        6,
    );
    assert_eq!(f.proof(20).source_incarnation(), None);
    f.frame(
        20,
        true,
        ProviderImageGenerationStatusV1::Completed,
        None,
        Some(ProviderTextV1::inline("C:\\unowned.png")),
        7,
    );
    let proof = f.proof(20);
    assert_eq!(proof.source_incarnation(), None);
    let asset = AssetId::sha256_v1([3; 32], NonZeroU64::MIN);
    assert!(
        f.storage
            .generated_media_preparation(proof.clone(), asset, "image/png", None)
            .is_err()
    );
    for reason in [
        Reason::Missing,
        Reason::Changed,
        Reason::Oversized,
        Reason::Unsupported,
        Reason::Unreadable,
        Reason::ProducerFailed,
        Reason::MalformedOutput,
    ] {
        assert!(
            f.storage
                .unavailable_generated_media(proof.clone(), reason, timestamp(8))
                .is_err()
        );
    }
    assert_committed(f.unavailable(proof, Reason::LostSourceAuthority, 8));
    assert_eq!(f.proof(20).source_incarnation(), None);
    f.store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn source_fingerprint_appends_exact_optional_incarnation_after_event_digest() {
    for incarnation in [
        None,
        Some(GeneratedSourceIncarnation::from_bytes([0x47; 32])),
    ] {
        let f = MediaHome::with_source_incarnation("generated-origin-fingerprint", incarnation);
        for completed in [false, true] {
            f.frame(
                20,
                completed,
                if completed {
                    ProviderImageGenerationStatusV1::Completed
                } else {
                    ProviderImageGenerationStatusV1::InProgress
                },
                None,
                completed.then(|| ProviderTextV1::inline("C:\\original.png")),
                if completed { 7 } else { 6 },
            );
            let proof = f.proof(20);
            let event = source_events(&f.store, &f.storage, f.turn).pop().unwrap();
            let mut digest = Sha256::new();
            digest.update(b"syndic/generated-source-event/v1\0");
            digest.update(
                syndic_storage::test_faults::generated_media_source_event_payload_for_test(&event),
            );
            let mut fingerprint = Sha256::new();
            fingerprint.update(b"beryl/generated-media-source/v1\0");
            fingerprint.update([1, u8::from(completed)]);
            for id in [
                proof.resource_id().as_bytes(),
                proof.thread_id().as_bytes(),
                proof.turn_id().as_bytes(),
                proof.item_id().as_bytes(),
                proof.execution_snapshot().id().as_bytes(),
            ] {
                fingerprint.update(id);
            }
            fingerprint.update(event.sequence().get().to_le_bytes());
            fingerprint.update(digest.finalize());
            fingerprint.update([u8::from(incarnation.is_some())]);
            if let Some(owner) = incarnation {
                fingerprint.update(owner.as_bytes());
            }
            assert_eq!(
                proof.source_fingerprint(),
                <[u8; 32]>::from(fingerprint.finalize())
            );
        }
    }
}

#[test]
fn historical_source_incarnation_remains_authenticated_after_target_retirement() {
    let f = MediaHome::new("generated-origin-retired");
    f.frame(
        20,
        true,
        ProviderImageGenerationStatusV1::Completed,
        None,
        Some(ProviderTextV1::inline("C:\\original.png")),
        6,
    );
    let original = f.proof(20);
    admit_event(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.turn,
        &f.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        timestamp(7),
    );
    converge_and_release_terminal_history(&f.store, f.storage.clone(), f.thread, f.turn);
    assert_eq!(
        f.storage
            .input_gate(&f.store, f.thread, limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::Idle
    );
    let retired = f.proof(20);
    assert_eq!(retired.source_incarnation(), original.source_incarnation());
    assert_eq!(retired.source_fingerprint(), original.source_fingerprint());
    let asset = f.prepare(20);
    assert_committed(f.store.execute(f.publication(20, asset, 20)));
    f.store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn source_proof_and_explicit_validation_refuse_replaced_snapshot_incarnation() {
    let f = MediaHome::new("generated-origin-corruption");
    f.frame(
        20,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        6,
    );
    let snapshot = f.proof(20).execution_snapshot().id();
    assert_committed(execute(
        &f.store,
        syndic_storage::test_faults::replace_generated_source_owner_for_test(
            &f.storage,
            f.storage.revision(&f.store).unwrap(),
            snapshot,
            Some(GeneratedSourceIncarnation::from_bytes([0x48; 32])),
        ),
    ));
    assert!(
        f.storage
            .generated_media_source(&f.store, f.resource(20))
            .is_err()
    );
    assert!(
        f.store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
}
