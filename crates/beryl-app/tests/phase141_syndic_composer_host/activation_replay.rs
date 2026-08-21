use super::*;

#[test]
fn activation_replay_collision_and_failure_preserve_the_prior_host() {
    let (_home, store, storage, thread) = fixture("atomic-activation", 60);
    populate(storage, &store, thread, 61);
    let mut host = SyndicComposerHost::new(storage);
    let initial = activation(thread, 70, 71, Vec::new());
    let first = host
        .activate(&store, initial.clone(), &CommandCancellation::new())
        .unwrap();
    let ComposerHostActivationOutcome::Activated {
        disposition: ComposerHostOpenDisposition::Opened,
        binding: first_binding,
    } = first
    else {
        panic!("fresh session did not open");
    };

    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        host.activate(&store, activation(thread, 72, 73, Vec::new()), &cancelled,),
        Ok(ComposerHostActivationOutcome::Cancelled)
    ));
    assert_eq!(host.binding(), Some(first_binding));

    let too_many = (1..=17)
        .map(|id| ComposerHostInitialDemand::Text {
            request_id: request_id(id),
            purpose: ComposerHostRequestPurpose::Viewport,
            demand: DraftPieceTextDemandV1::Validate(0),
            max_bytes: 4,
        })
        .collect();
    assert!(matches!(
        host.activate(
            &store,
            activation(thread, 72, 74, too_many),
            &CommandCancellation::new(),
        ),
        Err(ComposerHostError::TooManyInitialDemands)
    ));
    assert_eq!(host.binding(), Some(first_binding));

    let failing = activation(
        thread,
        74,
        75,
        vec![ComposerHostInitialDemand::Text {
            request_id: request_id(1),
            purpose: ComposerHostRequestPurpose::Viewport,
            demand: DraftPieceTextDemandV1::Forward(0),
            max_bytes: 3,
        }],
    );
    assert!(matches!(
        host.activate(&store, failing, &CommandCancellation::new()),
        Err(ComposerHostError::Range(
            DraftPieceRangeSourceErrorV1::Malformed(DraftPieceMalformedRangeRequestV1::Limit)
        ))
    ));
    assert_eq!(host.binding(), Some(first_binding));

    assert!(matches!(
        host.activate(
            &store,
            activation(thread, 70, 76, Vec::new()),
            &CommandCancellation::new(),
        ),
        Ok(ComposerHostActivationOutcome::OccupiedIdentityCollision(_))
    ));
    assert_eq!(host.binding(), Some(first_binding));

    let late_pending = host
        .begin_request(
            key(first_binding, 1),
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 4,
            },
        )
        .unwrap();
    let late_execution = host.execute_pending(&store, late_pending);
    let cancelled_pending = host
        .begin_request(
            key(first_binding, 2),
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 4,
            },
        )
        .unwrap();
    assert!(host.cancel_request(cancelled_pending.key()));
    let cancelled_execution = host.execute_pending(&store, cancelled_pending);
    assert!(matches!(
        host.complete_request(cancelled_execution),
        Err(ComposerHostError::RequestNotPending)
    ));

    let replay = host
        .activate(&store, initial, &CommandCancellation::new())
        .unwrap();
    let ComposerHostActivationOutcome::Activated {
        disposition: ComposerHostOpenDisposition::ExactReplay,
        binding: replay_binding,
    } = replay
    else {
        panic!("identical active session did not replay");
    };
    assert!(replay_binding.host_generation().get() > first_binding.host_generation().get());
    assert_eq!(replay_binding.root(), first_binding.root());
    assert!(matches!(
        host.complete_request(late_execution),
        Err(ComposerHostError::RequestNotPending)
    ));
    assert!(matches!(
        host.begin_request(
            key(first_binding, 3),
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 4,
            },
        ),
        Err(ComposerHostError::OldBinding)
    ));
    assert!(matches!(
        host.begin_request(
            key(replay_binding, 1),
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 4,
            },
        ),
        Ok(_)
    ));
    assert!(matches!(
        host.begin_request(
            key(replay_binding, 1),
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 4,
            },
        ),
        Err(ComposerHostError::StaleRequestIdentity)
    ));
    for id in 2..=64 {
        host.begin_request(
            key(replay_binding, id),
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Validate(0),
                max_bytes: 4,
            },
        )
        .unwrap();
    }
    assert_eq!(host.pending_request_count(), 64);
    assert!(matches!(
        host.begin_request(
            key(replay_binding, 65),
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Validate(0),
                max_bytes: 4,
            },
        ),
        Err(ComposerHostError::PendingRequestLimit)
    ));
    assert_eq!(host.pending_request_count(), 64);
    assert!(host.release().unwrap());
    assert_eq!(host.pending_request_count(), 0);
}
