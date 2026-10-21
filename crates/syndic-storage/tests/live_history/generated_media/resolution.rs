use super::*;

#[test]
fn durable_preparation_cannot_be_discarded_by_unavailable_resolution() {
    let f = MediaHome::new("generated-prepared-race");
    f.frame(
        20,
        true,
        ProviderImageGenerationStatusV1::Completed,
        None,
        Some(ProviderTextV1::inline("C:\\out.png")),
        6,
    );
    let before = f.proof(20);
    let asset = f.prepare(20);
    assert!(matches!(
        f.unavailable(f.proof(20), Reason::LostSourceAuthority, 7),
        CommandOutcome::NotCommitted { .. }
    ));
    let admission = f
        .state
        .assets()
        .generated_media_admission(&f.store, MediaHome::target(&before))
        .unwrap()
        .unwrap();
    assert_eq!(admission.lifecycle(), GeneratedMediaLifecycle::Prepared);
    assert_committed(f.store.execute(f.publication(20, asset, 7)));
    let published = f.proof(20);
    assert_eq!(published.source_fingerprint(), before.source_fingerprint());
    assert_eq!(
        published.resource().backing(),
        ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::Asset(asset))
    );
    assert!(
        f.storage
            .pending_generated_media_page(
                &f.store,
                &f.storage.begin_generated_media_sweep(&f.store).unwrap(),
                256,
                65_536
            )
            .unwrap()
            .entries()
            .is_empty()
    );
    assert_eq!(
        f.state
            .assets()
            .generated_media_admission(&f.store, MediaHome::target(&before))
            .unwrap()
            .unwrap()
            .lifecycle(),
        GeneratedMediaLifecycle::Published
    );
    f.store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn generated_mutation_refuses_missing_or_mismatched_peer_and_stale_source_guards() {
    let f = MediaHome::new("generated-pair-guards");
    f.frame(
        20,
        true,
        ProviderImageGenerationStatusV1::Completed,
        None,
        Some(ProviderTextV1::inline("C:\\out.png")),
        6,
    );
    let asset = f.prepare(20);
    let proof = f.proof(20);
    let singleton = f
        .storage
        .publish_generated_media(proof.clone(), asset, "image/png", None, timestamp(7))
        .unwrap();
    assert!(matches!(
        execute(&f.store, singleton),
        CommandOutcome::NotCommitted { .. }
    ));
    let admission = f
        .state
        .assets()
        .generated_media_admission(&f.store, MediaHome::target(&proof))
        .unwrap()
        .unwrap();
    let mut mismatched = HomeCommand::new(f.store.home_revision().unwrap());
    mismatched
        .add(
            f.state
                .assets()
                .publish_generated_media(
                    &f.store,
                    f.state.assets().revision(&f.store).unwrap(),
                    &admission,
                )
                .unwrap(),
        )
        .unwrap();
    mismatched
        .add(
            f.storage
                .publish_generated_media(
                    proof.clone(),
                    asset,
                    "image/png",
                    Some([4, 4]),
                    timestamp(7),
                )
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        f.store.execute(mismatched),
        CommandOutcome::NotCommitted { .. }
    ));
    let stale = f
        .storage
        .publish_generated_media(proof, asset, "image/png", None, timestamp(8))
        .unwrap();
    f.frame(
        21,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        7,
    );
    let mut command = HomeCommand::new(f.store.home_revision().unwrap());
    command
        .add(
            f.state
                .assets()
                .publish_generated_media(
                    &f.store,
                    f.state.assets().revision(&f.store).unwrap(),
                    &admission,
                )
                .unwrap(),
        )
        .unwrap();
    command.add(stale).unwrap();
    assert!(matches!(
        f.store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        f.proof(20).resource().backing(),
        ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::PendingAsset)
    );
}

#[test]
fn awaiting_source_loss_resolves_same_resource_without_rewriting_provider_outcome() {
    let f = MediaHome::new("generated-awaiting-loss");
    f.frame(
        20,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        6,
    );
    let before = f.proof(20);
    let state = f
        .storage
        .turn_state(&f.store, f.turn, limit())
        .unwrap()
        .unwrap();
    let events = source_events(&f.store, &f.storage, f.turn);
    assert_committed(f.unavailable(before.clone(), Reason::LostSourceAuthority, 7));
    let after = f.proof(20);
    assert_eq!(after.resource_id(), before.resource_id());
    assert_eq!(after.source_fingerprint(), before.source_fingerprint());
    assert_eq!(after.item(), before.item());
    assert_eq!(source_events(&f.store, &f.storage, f.turn), events);
    assert_eq!(
        f.storage
            .turn_state(&f.store, f.turn, limit())
            .unwrap()
            .unwrap(),
        state
    );
    assert!(
        !f.storage
            .history_summary(&f.store, f.thread, limit())
            .unwrap()
            .unwrap()
            .complete()
    );
}
