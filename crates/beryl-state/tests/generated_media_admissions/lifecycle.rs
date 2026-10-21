use super::*;

#[test]
fn admission_read_distinguishes_concurrent_publication_from_disagreeing_facts() {
    let home = open("admission-read-concurrency");
    let target = target();
    let prepared = home.prepared(target);
    let publication = home.publication_command(&prepared);
    let blocked = home
        .faults
        .block_next(beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation);
    std::thread::scope(|scope| {
        let assets = home.state.assets();
        let store = &home.store;
        let reader = scope.spawn(move || assets.generated_media_admission(store, target));
        assert!(blocked.wait_until_reached(std::time::Duration::from_secs(10)));
        committed(home.store.execute(publication));
        blocked.release();
        assert!(matches!(
            reader.join().unwrap(),
            Err(GeneratedMediaError::ConcurrentChange)
        ));
    });
    assert_eq!(
        home.admission(target).lifecycle(),
        GeneratedMediaLifecycle::Published
    );
    let wrong = GeneratedMediaTarget::new(
        target.resource(),
        target.thread(),
        target.turn(),
        target.item(),
        [99; 32],
    );
    assert!(matches!(
        home.state
            .assets()
            .generated_media_admission(&home.store, wrong),
        Err(GeneratedMediaError::Rejected),
    ));
    home.store.close().unwrap();
}

#[test]
fn preparation_is_durable_through_fresh_handles_and_publication_is_a_dedicated_reference() {
    let home = open("lifecycle");
    let target = target();
    let prepared = home.prepared(target);
    assert_eq!(prepared.lifecycle(), GeneratedMediaLifecycle::Prepared);
    assert_eq!(prepared.revision().get(), 1);
    assert_eq!(home.peer_value(), Some(1));
    let metadata = home
        .state
        .assets()
        .metadata(&home.store, prepared.facts().asset())
        .unwrap()
        .unwrap();
    let fresh_state = beryl_state::BerylState::reacquire(&home.store).unwrap();
    let fresh = fresh_state.assets();
    assert_eq!(
        fresh
            .generated_media_admission(&home.store, target)
            .unwrap()
            .unwrap()
            .facts(),
        prepared.facts()
    );
    let receipt = committed(home.store.execute(home.publication_command(&prepared)));
    assert!(
        home.state
            .assets()
            .committed_revision(&home.store, &receipt)
            .unwrap()
            .is_some()
    );
    assert!(
        home.store
            .receipt_domain_revision(&receipt, &home.peer)
            .unwrap()
            .is_some()
    );
    assert_eq!(home.peer_value(), Some(2));
    let published = fresh
        .generated_media_admission(&home.store, target)
        .unwrap()
        .unwrap();
    assert_eq!(published.lifecycle(), GeneratedMediaLifecycle::Published);
    assert_eq!(published.revision().get(), 2);
    assert_eq!(published.facts(), prepared.facts());
    assert_eq!(
        home.state
            .assets()
            .metadata(&home.store, prepared.facts().asset())
            .unwrap()
            .unwrap(),
        metadata
    );
    home.store.close().unwrap();
}

#[test]
fn metadata_reuse_preserves_absent_dimensions_creation_and_revision() {
    let home = open("metadata-absent-dimensions");
    let asset = seed_metadata(&home, "image/png", None);
    let metadata = home
        .state
        .assets()
        .metadata(&home.store, asset)
        .unwrap()
        .unwrap();
    let prep = home.preparation(target());
    assert_eq!(prep.facts().dimensions(), None);
    let digest = asset_digest(1, target(), prep.facts());
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    prep.add_to(&mut command).unwrap();
    command
        .add_validation(home.peer.validation(
            home.peer_revision(),
            PeerGuard {
                pair: Some(pair::<1>(CommandParticipantKind::Mutation, digest)),
                reject: false,
            },
        ))
        .unwrap();
    committed(home.store.execute(command));
    let admission = home.admission(target());
    assert_eq!(admission.facts().dimensions(), None);
    committed(home.store.execute(home.publication_command(&admission)));
    assert_eq!(
        home.state
            .assets()
            .metadata(&home.store, asset)
            .unwrap()
            .unwrap(),
        metadata
    );
    home.store.close().unwrap();
}

#[test]
fn metadata_reuse_preserves_matching_present_dimensions() {
    let home = open("metadata-present-dimensions");
    let asset = seed_metadata(&home, "image/png", Some(dimensions(20, 30)));
    let metadata = home
        .state
        .assets()
        .metadata(&home.store, asset)
        .unwrap()
        .unwrap();
    let admission = home.prepared(target());
    assert_eq!(admission.facts().dimensions(), Some(dimensions(20, 30)));
    assert_eq!(
        home.state
            .assets()
            .metadata(&home.store, asset)
            .unwrap()
            .unwrap(),
        metadata
    );
    home.store.close().unwrap();
}

#[test]
fn exact_preparation_and_publication_replays_have_no_effect() {
    let home = open("exact-replay");
    let admission = home.prepared(target());
    let revision = home.revision();
    let token = sidecar(&home.store, BYTES, "images");
    let bytes = facts(&token, Some(dimensions(20, 30)));
    assert!(matches!(
        home.state
            .assets()
            .prepare_generated_media(&home.store, revision, token, target(), bytes),
        Err(GeneratedMediaError::NoEffect)
    ));
    assert_eq!(home.revision(), revision);
    committed(home.store.execute(home.publication_command(&admission)));
    let revision = home.revision();
    assert!(matches!(
        home.state
            .assets()
            .publish_generated_media(&home.store, revision, &admission),
        Err(GeneratedMediaError::NoEffect)
    ));
    let published = home.admission(target());
    assert!(matches!(
        home.state
            .assets()
            .publish_generated_media(&home.store, revision, &published),
        Err(GeneratedMediaError::NoEffect)
    ));
    assert_eq!(home.revision(), revision);
    home.store.close().unwrap();
}

#[test]
fn unavailable_resolution_keeps_asset_domain_unaffected() {
    let home = open("absence");
    let revision = home.revision();
    let receipt = committed(home.store.execute(home.unavailable_command(
        target(),
        GeneratedMediaUnavailableReason::LostSourceAuthority,
    )));
    assert_eq!(home.peer_value(), Some(3));
    assert_eq!(home.revision(), revision);
    assert_eq!(
        home.state
            .assets()
            .committed_revision(&home.store, &receipt)
            .unwrap(),
        None
    );
    assert!(
        home.state
            .assets()
            .generated_media_admission(&home.store, target())
            .unwrap()
            .is_none()
    );
    home.store.close().unwrap();
}

#[test]
fn durable_prepared_evidence_resumes_after_reopen_without_an_old_token() {
    let home = open("restart");
    let old = home.prepared(target());
    let path = home.path.clone();
    home.store.close().unwrap();
    let reopened = open_at(&path);
    let fresh = reopened.admission(target());
    assert_eq!(fresh.facts(), old.facts());
    assert!(matches!(
        reopened
            .state
            .assets()
            .publish_generated_media(&reopened.store, reopened.revision(), &old),
        Err(GeneratedMediaError::Read(_))
    ));
    committed(reopened.store.execute(reopened.publication_command(&fresh)));
    assert_eq!(
        reopened.admission(target()).lifecycle(),
        GeneratedMediaLifecycle::Published
    );
    reopened.store.close().unwrap();
}

#[test]
fn each_participant_has_a_fixed_bounded_closure() {
    let preparation = GeneratedMediaCommandFootprint::preparation();
    let publication = GeneratedMediaCommandFootprint::publication();
    assert_eq!(preparation, publication);
    assert_eq!(preparation.reads().records(), 2);
    assert_eq!(preparation.writes().records(), 2);
    assert_eq!(preparation.writes().encoded_key_bytes(), 57);
    assert_eq!(preparation.writes().encoded_value_bytes(), 776);
    assert_eq!(preparation.reconciliation_records(), 2);
    let absence = GeneratedMediaCommandFootprint::absence();
    assert_eq!(absence.reads().records(), 1);
    assert_eq!(absence.reads().encoded_value_bytes(), 516);
    assert_eq!(absence.writes().records(), 0);
    assert_eq!(absence.reconciliation_records(), 0);
}
