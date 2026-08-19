use super::*;

#[test]
fn direct_rebind_drops_only_unadmitted_staging_and_accepts_new_work() {
    let (_home, store, storage, thread) = fixture("phase143-staged-rebind", 8);
    let (mut host, old) = activated(storage, &store, thread, 9, 10);
    host.begin_mutation(&store, text_request(old, 11, 0, 0, &["old"], 3))
        .unwrap();
    assert_eq!(
        host.mutation_status(),
        Some(ComposerHostMutationStatus::Staged)
    );

    let ComposerHostActivationOutcome::Activated {
        binding: rebound, ..
    } = host
        .activate(
            &store,
            activation(thread, 12, 13),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("direct rebind did not activate");
    };
    assert_ne!(rebound, old);
    assert_eq!(host.mutation_status(), None);
    assert!(matches!(
        host.execute_mutation(&store, &CommandCancellation::new()),
        Err(ComposerHostError::MutationNotPending)
    ));
    let committed = commit_text(&mut host, &store, rebound, 14, 0, 0, &["new"], 3);
    assert_eq!(candidate_text(storage, &store, committed), b"new");
}

#[test]
fn full_widget_identity_survives_clean_rebind_and_release_clears_it() {
    let (_home, store, storage, thread) = fixture("phase143-rebind-identity", 15);
    let (mut host, original) = activated(storage, &store, thread, 16, 17);
    let exact = text_request(original, 18, 0, 0, &["rejected"], 99);
    host.begin_mutation(&store, exact.clone()).unwrap();
    assert_eq!(
        host.execute_mutation(&store, &CommandCancellation::new())
            .unwrap(),
        ComposerHostMutationOutcome::Rejected
    );
    host.begin_mutation(&store, exact).unwrap();
    assert_eq!(
        host.mutation_status(),
        Some(ComposerHostMutationStatus::Staged)
    );

    let ComposerHostActivationOutcome::Activated {
        binding: rebound, ..
    } = host
        .activate(
            &store,
            activation(thread, 19, 20),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("clean rebind did not activate");
    };
    let rebound_request = text_request(rebound, 18, 0, 0, &["rejected"], 99);
    let collision = ComposerHostMutationRequest::new(
        rebound,
        MutationProposal::new(
            rebound_request.proposal().key(),
            MutationKind::Edit,
            rebound_request.proposal().replacement(),
            1,
        ),
        rebound_request.operation_id(),
        rebound_request.fragments().to_vec().into_boxed_slice(),
        Box::new([]),
    );
    assert!(matches!(
        host.begin_mutation(&store, collision),
        Err(ComposerHostError::MutationIdentityCollision)
    ));
    assert_eq!(host.binding(), Some(rebound));
    assert_eq!(host.mutation_status(), None);

    assert_eq!(host.release().unwrap(), true);
    let ComposerHostActivationOutcome::Activated {
        binding: released_rebind,
        ..
    } = host
        .activate(
            &store,
            activation(thread, 21, 22),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("post-release rebind did not activate");
    };
    host.begin_mutation(
        &store,
        text_request(released_rebind, 18, 0, 0, &["fresh"], 5),
    )
    .unwrap();
    assert_eq!(
        host.mutation_status(),
        Some(ComposerHostMutationStatus::Staged)
    );
    assert_eq!(host.release().unwrap(), true);
}

#[test]
fn stale_release_rebind_late_delivery_numeric_aba_and_old_reads_fail_closed() {
    let (_home, store, storage, thread) = fixture("phase143-stale", 61);
    let (mut host, old) = activated(storage, &store, thread, 62, 63);
    let pending = text_request(old, 64, 0, 0, &["late"], 4);
    host.begin_mutation(&store, pending.clone()).unwrap();
    assert!(host.release().unwrap());
    assert!(matches!(
        host.execute_mutation(&store, &CommandCancellation::new()),
        Err(ComposerHostError::MutationNotPending)
    ));

    let ComposerHostActivationOutcome::Activated {
        binding: rebound, ..
    } = host
        .activate(
            &store,
            activation(thread, 62, 63),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("rebind did not activate");
    };
    assert_ne!(rebound.host_generation(), old.host_generation());
    assert!(matches!(
        host.begin_mutation(&store, pending),
        Err(ComposerHostError::OldBinding)
    ));
    let committed = commit_text(&mut host, &store, rebound, 64, 0, 0, &["late"], 4);
    assert_eq!(candidate_text(storage, &store, committed), b"late");

    let read_key = ComposerHostRequestKey::new(
        committed,
        ComposerHostRequestId::new(NonZeroU64::MIN),
        ComposerHostRequestPurpose::Viewport,
    );
    let old_read = host
        .begin_request(
            read_key,
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 65_536,
            },
        )
        .unwrap();
    let successor = commit_text(&mut host, &store, committed, 65, 4, 4, &["!"], 5);
    assert_eq!(host.pending_request_count(), 0);
    assert!(matches!(
        host.complete_request(host.execute_pending(&store, old_read)),
        Err(ComposerHostError::OldBinding | ComposerHostError::RequestNotPending)
    ));
    assert_eq!(host.binding(), Some(successor));
}
