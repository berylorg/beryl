#[path = "phase141_syndic_composer_host/support.rs"]
mod support;

use std::num::NonZeroU64;

use beryl_app::composer_host::{
    ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostError,
    ComposerHostInitialDemand, ComposerHostOpenDisposition, ComposerHostReadTarget,
    ComposerHostRequestId, ComposerHostRequestKey, ComposerHostRequestKind,
    ComposerHostRequestPurpose, ComposerHostResponseValue, ComposerHostRestorationSeed,
    SyndicComposerHost,
};
use beryl_home_store::CommandCancellation;
use syndic_storage::{
    DraftByThreadRecord, DraftEditorCandidateSessionIdV1, DraftPieceMalformedRangeRequestV1,
    DraftPieceMarkerAtV1, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerEdgeProofRequestV1, DraftPieceMarkerEdgeProofV1, DraftPieceMarkerScopeV1,
    DraftPieceOperationIdV1, DraftPieceRangeSourceErrorV1, DraftPieceTextDemandV1,
    SelectedPathProof, ThreadRecord,
};

use support::{current, fixture, point, populate};

#[cfg(feature = "test-faults")]
use support::{committed, execute, run_transaction, transaction, transaction_for_session};

#[cfg(feature = "test-faults")]
use syndic_storage::{
    DraftEditorCandidateSessionReadOutcomeV1, DraftPieceReplacementV1, DraftPieceV1,
};

#[cfg(feature = "test-faults")]
use syndic_storage::test_faults::{
    DraftPieceDescendantCorruption, DraftPieceDescendantTarget, DraftPieceImmutableDeletion,
    FixtureBatch, FixtureRecord, arm_draft_piece_candidate_read_fault,
    arm_draft_piece_current_read_fault, delete_draft_piece_immutable_record,
    inject_draft_piece_descendant_corruption,
};

#[test]
fn activation_and_bounded_range_custody_are_exact() {
    let (_home, store, storage, thread) = fixture("range-custody", 1);
    let (left, right) = populate(storage, &store, thread, 10);
    let first_marker_demand = marker_demand(None, 1, 65_536);
    let mut host = SyndicComposerHost::new(storage);
    let activation = host
        .activate(
            &store,
            activation(
                thread,
                10,
                20,
                vec![
                    ComposerHostInitialDemand::Text {
                        request_id: request_id(1),
                        purpose: ComposerHostRequestPurpose::Viewport,
                        demand: DraftPieceTextDemandV1::Forward(0),
                        max_bytes: 65_536,
                    },
                    ComposerHostInitialDemand::Markers {
                        request_id: request_id(2),
                        purpose: ComposerHostRequestPurpose::Geometry,
                        demand: first_marker_demand.clone(),
                    },
                ],
            ),
            &CommandCancellation::new(),
        )
        .unwrap();
    let ComposerHostActivationOutcome::Activated {
        disposition: ComposerHostOpenDisposition::ExactReplay,
        binding,
    } = activation
    else {
        panic!("fresh session did not activate");
    };
    let root = binding.root();
    assert_eq!(binding.logical_extent(), root.summary().logical_extent());
    assert_eq!(host.initial_responses().len(), 2);
    assert!(matches!(
        host.initial_responses()[0].value(),
        ComposerHostResponseValue::CandidateText(value)
            if value.value().bytes() == "α\nβ\n".as_bytes()
    ));
    assert!(matches!(
        host.initial_responses()[1].value(),
        ComposerHostResponseValue::CandidateMarkers(value)
            if value.value().markers() == [DraftPieceMarkerAtV1::new(3, left)]
                && !value.value().requested_side_complete()
    ));

    let backward = run(
        &mut host,
        &store,
        binding,
        3,
        ComposerHostRequestKind::Text {
            target: ComposerHostReadTarget::Candidate,
            demand: DraftPieceTextDemandV1::Backward(6),
            max_bytes: 65_536,
        },
    )
    .unwrap();
    assert!(matches!(
        backward,
        ComposerHostResponseValue::CandidateText(value)
            if value.value().bytes() == "α\nβ\n".as_bytes()
    ));
    let historical = run(
        &mut host,
        &store,
        binding,
        4,
        ComposerHostRequestKind::Text {
            target: ComposerHostReadTarget::Historical(root),
            demand: DraftPieceTextDemandV1::Forward(0),
            max_bytes: 65_536,
        },
    )
    .unwrap();
    assert!(matches!(
        historical,
        ComposerHostResponseValue::HistoricalText(value)
            if value.bytes() == "α\nβ\n".as_bytes()
    ));
    let current_value = run(
        &mut host,
        &store,
        binding,
        5,
        ComposerHostRequestKind::Text {
            target: ComposerHostReadTarget::Current(thread),
            demand: DraftPieceTextDemandV1::Forward(0),
            max_bytes: 4,
        },
    )
    .unwrap();
    assert!(matches!(
        current_value,
        ComposerHostResponseValue::CurrentText(Some(value)) if value.value().bytes().is_empty()
    ));
    assert!(matches!(
        run(
            &mut host,
            &store,
            binding,
            6,
            ComposerHostRequestKind::Markers {
                target: ComposerHostReadTarget::Current(thread),
                demand: DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::ExactAnchor(0),
                    DraftPieceMarkerDirectionV1::Forward,
                    None,
                    1,
                    65_536,
                ),
            },
        ),
        Ok(ComposerHostResponseValue::CurrentMarkers(Some(value))) if value.value().markers().is_empty()
    ));
    assert!(matches!(
        run(
            &mut host,
            &store,
            binding,
            7,
            ComposerHostRequestKind::MarkerProof {
                target: ComposerHostReadTarget::Current(thread),
                request: DraftPieceMarkerEdgeProofRequestV1::Absence { anchor: 0 },
                retained_byte_ceiling: 9,
            },
        ),
        Ok(ComposerHostResponseValue::CurrentMarkerProof(Some(value)))
            if matches!(value.value(), Some(DraftPieceMarkerEdgeProofV1::Absence { .. }))
    ));
    assert!(matches!(
        run(
            &mut host,
            &store,
            binding,
            8,
            ComposerHostRequestKind::MarkerProof {
                target: ComposerHostReadTarget::Historical(root),
                request: DraftPieceMarkerEdgeProofRequestV1::Last {
                    marker: DraftPieceMarkerAtV1::new(3, right),
                },
                retained_byte_ceiling: 41,
            },
        ),
        Ok(ComposerHostResponseValue::HistoricalMarkerProof(Some(
            DraftPieceMarkerEdgeProofV1::Last { .. }
        )))
    ));
    assert!(matches!(
        run(
            &mut host,
            &store,
            binding,
            9,
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Forward(1),
                max_bytes: 4,
            },
        ),
        Err(ComposerHostError::Range(
            DraftPieceRangeSourceErrorV1::Malformed(
                DraftPieceMalformedRangeRequestV1::Utf8Boundary
            )
        ))
    ));

    let revision = storage.revision(&store).unwrap();
    for (ordinal, request, one_under, exact) in [
        (
            10,
            DraftPieceMarkerEdgeProofRequestV1::Absence { anchor: 0 },
            8,
            9,
        ),
        (
            12,
            DraftPieceMarkerEdgeProofRequestV1::First {
                marker: DraftPieceMarkerAtV1::new(3, left),
            },
            40,
            41,
        ),
        (
            14,
            DraftPieceMarkerEdgeProofRequestV1::Last {
                marker: DraftPieceMarkerAtV1::new(3, right),
            },
            40,
            41,
        ),
        (
            16,
            DraftPieceMarkerEdgeProofRequestV1::Adjacent {
                left: DraftPieceMarkerAtV1::new(3, left),
                right: DraftPieceMarkerAtV1::new(3, right),
            },
            80,
            81,
        ),
    ] {
        assert!(matches!(
            run(
                &mut host,
                &store,
                binding,
                ordinal,
                ComposerHostRequestKind::MarkerProof {
                    target: ComposerHostReadTarget::Candidate,
                    request,
                    retained_byte_ceiling: one_under,
                },
            ),
            Err(ComposerHostError::Range(
                DraftPieceRangeSourceErrorV1::Limit
            ))
        ));
        assert!(matches!(
            run(
                &mut host,
                &store,
                binding,
                ordinal + 1,
                ComposerHostRequestKind::MarkerProof {
                    target: ComposerHostReadTarget::Candidate,
                    request,
                    retained_byte_ceiling: exact,
                },
            ),
            Ok(ComposerHostResponseValue::CandidateMarkerProof(value))
                if matches!(value.value(), Some(
                    DraftPieceMarkerEdgeProofV1::Absence { .. }
                    | DraftPieceMarkerEdgeProofV1::First { .. }
                    | DraftPieceMarkerEdgeProofV1::Last { .. }
                    | DraftPieceMarkerEdgeProofV1::Adjacent { .. }
                ))
        ));
    }
    assert_eq!(storage.revision(&store).unwrap(), revision);

    let first_page = match run(
        &mut host,
        &store,
        binding,
        18,
        ComposerHostRequestKind::Markers {
            target: ComposerHostReadTarget::Candidate,
            demand: first_marker_demand,
        },
    )
    .unwrap()
    {
        ComposerHostResponseValue::CandidateMarkers(value) => value,
        _ => panic!("wrong marker response"),
    };
    let retained_bytes = first_page.value().retained_bytes();
    assert!(matches!(
        run(
            &mut host,
            &store,
            binding,
            19,
            ComposerHostRequestKind::Markers {
                target: ComposerHostReadTarget::Candidate,
                demand: marker_demand(None, 1, retained_bytes - 1),
            },
        ),
        Err(ComposerHostError::Range(
            DraftPieceRangeSourceErrorV1::Limit
        ))
    ));
    assert!(matches!(
        run(
            &mut host,
            &store,
            binding,
            20,
            ComposerHostRequestKind::Markers {
                target: ComposerHostReadTarget::Candidate,
                demand: marker_demand(None, 1, retained_bytes),
            },
        ),
        Ok(ComposerHostResponseValue::CandidateMarkers(value))
            if value.value().retained_bytes() == retained_bytes
    ));
    let continuation = first_page.value().continuation().unwrap();
    let second_page = run(
        &mut host,
        &store,
        binding,
        21,
        ComposerHostRequestKind::Markers {
            target: ComposerHostReadTarget::Candidate,
            demand: marker_demand(Some(continuation), 1, 65_536),
        },
    )
    .unwrap();
    assert!(matches!(
        second_page,
        ComposerHostResponseValue::CandidateMarkers(value)
            if value.value().markers() == [DraftPieceMarkerAtV1::new(3, right)]
    ));
    let backward_markers = run(
        &mut host,
        &store,
        binding,
        22,
        ComposerHostRequestKind::Markers {
            target: ComposerHostReadTarget::Historical(root),
            demand: DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(3),
                DraftPieceMarkerDirectionV1::Backward,
                None,
                2,
                65_536,
            ),
        },
    )
    .unwrap();
    assert!(matches!(
        backward_markers,
        ComposerHostResponseValue::HistoricalMarkers(value) if value.markers().len() == 2
    ));

    let seed = ComposerHostRestorationSeed::new(
        root,
        binding.logical_extent(),
        point(0),
        point(0),
        point(0),
        Some([9; 32]),
    );
    let current_root = current(storage, &store, thread).draft().piece_root();
    let current_seed = ComposerHostRestorationSeed::new(
        current_root,
        current_root.summary().logical_extent(),
        point(0),
        point(0),
        point(0),
        Some([9; 32]),
    );
    assert!(matches!(
        run(
            &mut host,
            &store,
            binding,
            23,
            ComposerHostRequestKind::Restoration {
                target: ComposerHostReadTarget::Candidate,
                seed: seed.clone(),
            },
        ),
        Ok(ComposerHostResponseValue::Restoration(_))
    ));
    assert!(matches!(
        run(
            &mut host,
            &store,
            binding,
            24,
            ComposerHostRequestKind::Restoration {
                target: ComposerHostReadTarget::Historical(root),
                seed: seed.clone(),
            },
        ),
        Ok(ComposerHostResponseValue::Restoration(_))
    ));
    assert!(matches!(
        run(
            &mut host,
            &store,
            binding,
            25,
            ComposerHostRequestKind::Restoration {
                target: ComposerHostReadTarget::Current(thread),
                seed: current_seed,
            },
        ),
        Ok(ComposerHostResponseValue::Restoration(_))
    ));

    for request in 26..=57 {
        let value = run(
            &mut host,
            &store,
            binding,
            request,
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Validate(0),
                max_bytes: 4,
            },
        )
        .unwrap();
        assert!(matches!(value, ComposerHostResponseValue::CandidateText(_)));
        assert_eq!(host.pending_request_count(), 0);
    }

    let pending = host
        .begin_request(
            key(binding, 58),
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 4,
            },
        )
        .unwrap();
    let execution = host.execute_pending(&store, pending);
    assert!(host.release().unwrap());
    assert!(matches!(
        host.complete_request(execution),
        Err(ComposerHostError::RequestNotPending)
    ));
    assert_eq!(host.binding(), None);
    assert_eq!(host.pending_request_count(), 0);
    assert!(host.initial_responses().is_empty());
}

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

#[test]
fn stale_same_id_completion_and_cancellation_are_inert_across_rebind_and_release() {
    let (_home, store, storage, thread) = fixture("same-id-aba", 75);
    populate(storage, &store, thread, 76);
    let mut host = SyndicComposerHost::new(storage);
    let original_activation = activation(thread, 77, 78, Vec::new());
    let ComposerHostActivationOutcome::Activated {
        binding: original_binding,
        ..
    } = host
        .activate(
            &store,
            original_activation.clone(),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("fixture activation failed");
    };
    let old_key = key(original_binding, 1);
    let old_pending = host.begin_request(old_key, text_request()).unwrap();
    let old_execution = host.execute_pending(&store, old_pending);

    let ComposerHostActivationOutcome::Activated {
        disposition: ComposerHostOpenDisposition::ExactReplay,
        binding: rebound_binding,
    } = host
        .activate(&store, original_activation, &CommandCancellation::new())
        .unwrap()
    else {
        panic!("fixture session did not rebind by exact replay");
    };
    let rebound_key = key(rebound_binding, 1);
    let rebound_pending = host.begin_request(rebound_key, text_request()).unwrap();
    assert!(matches!(
        host.complete_request(old_execution),
        Err(ComposerHostError::RequestMismatch)
    ));
    assert_eq!(host.pending_request_count(), 1);
    assert!(!host.cancel_request(old_key));
    assert_eq!(host.pending_request_count(), 1);
    let rebound_execution = host.execute_pending(&store, rebound_pending);
    assert!(matches!(
        host.complete_request(rebound_execution),
        Ok(response) if response.key() == rebound_key
    ));

    let release_old_key = key(rebound_binding, 2);
    let release_old_pending = host.begin_request(release_old_key, text_request()).unwrap();
    let release_old_execution = host.execute_pending(&store, release_old_pending);
    assert!(host.release().unwrap());
    let ComposerHostActivationOutcome::Activated {
        binding: released_binding,
        ..
    } = host
        .activate(
            &store,
            activation(thread, 79, 80, Vec::new()),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("fixture did not reactivate after release");
    };
    let released_key = key(released_binding, 2);
    let released_pending = host.begin_request(released_key, text_request()).unwrap();
    assert!(matches!(
        host.complete_request(release_old_execution),
        Err(ComposerHostError::RequestMismatch)
    ));
    assert_eq!(host.pending_request_count(), 1);
    assert!(!host.cancel_request(release_old_key));
    assert_eq!(host.pending_request_count(), 1);
    let released_execution = host.execute_pending(&store, released_pending);
    assert!(matches!(
        host.complete_request(released_execution),
        Ok(response) if response.key() == released_key
    ));
    assert_eq!(host.pending_request_count(), 0);
}

#[cfg(feature = "test-faults")]
#[test]
fn disposed_absent_corrupt_and_selector_drift_are_typed_and_atomic() {
    let (_home, store, storage, thread) = fixture("typed-failures", 80);
    populate(storage, &store, thread, 81);
    let mut host = SyndicComposerHost::new(storage);
    let request = activation(thread, 82, 83, Vec::new());
    let ComposerHostActivationOutcome::Activated { binding, .. } = host
        .activate(&store, request.clone(), &CommandCancellation::new())
        .unwrap()
    else {
        panic!("fixture activation failed");
    };
    committed(execute(
        &store,
        storage.test_dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        ),
    ));
    assert!(matches!(
        host.activate(&store, request, &CommandCancellation::new()),
        Ok(ComposerHostActivationOutcome::StaleDisposed(_))
    ));
    assert_eq!(host.binding(), Some(binding));

    let (_home, store, storage, thread) = fixture("missing-root", 84);
    populate(storage, &store, thread, 85);
    let mut host = SyndicComposerHost::new(storage);
    let ComposerHostActivationOutcome::Activated { binding, .. } = host
        .activate(
            &store,
            activation(thread, 85, 95, Vec::new()),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("fixture activation failed");
    };
    committed(execute(
        &store,
        delete_draft_piece_immutable_record(
            &store,
            storage,
            binding.root(),
            DraftPieceImmutableDeletion::Root,
        ),
    ));
    assert!(matches!(
        run(
            &mut host,
            &store,
            binding,
            1,
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Historical(binding.root()),
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 65_536,
            },
        ),
        Err(ComposerHostError::Range(
            DraftPieceRangeSourceErrorV1::Absent
        ))
    ));

    for (case, corruption) in [
        DraftPieceDescendantCorruption::NewlineAggregate,
        DraftPieceDescendantCorruption::LogicalLineAggregate,
    ]
    .into_iter()
    .enumerate()
    {
        let (_home, store, storage, thread) = fixture("corrupt-summary", 88 + case as u8);
        let before = current(storage, &store, thread);
        let pieces = (0..130)
            .map(|_| DraftPieceV1::Text("x\n".to_owned()))
            .collect();
        let build = transaction(
            storage,
            &store,
            &before,
            91 + case as u8,
            93 + case as u8,
            vec![DraftPieceReplacementV1::new(point(0), point(0), pieces)],
            point(260),
        );
        run_transaction(storage, &store, &build, 2);
        let mut host = SyndicComposerHost::new(storage);
        let ComposerHostActivationOutcome::Activated { binding, .. } = host
            .activate(
                &store,
                activation(thread, 91 + case as u8, 93 + case as u8, Vec::new()),
                &CommandCancellation::new(),
            )
            .unwrap()
        else {
            panic!("fixture activation failed");
        };
        committed(execute(
            &store,
            inject_draft_piece_descendant_corruption(
                &store,
                storage,
                binding.root(),
                DraftPieceDescendantTarget::Sequence,
                corruption,
            ),
        ));
        assert!(matches!(
            run(
                &mut host,
                &store,
                binding,
                1,
                ComposerHostRequestKind::Text {
                    target: ComposerHostReadTarget::Candidate,
                    demand: DraftPieceTextDemandV1::Forward(0),
                    max_bytes: 65_536,
                },
            ),
            Err(ComposerHostError::Range(
                DraftPieceRangeSourceErrorV1::Invariant
            ))
        ));
    }

    let (_home, store, storage, thread) = fixture("selector-conflict", 101);
    let before = current(storage, &store, thread);
    let next_thread_revision = before.thread().revision().checked_next().unwrap();
    let advanced_thread = ThreadRecord::new(
        before.thread().id(),
        SelectedPathProof::new(
            before.thread().committed_tail(),
            next_thread_revision,
            before.thread().selected_path_digest(),
        ),
        before.thread().current_draft_id(),
        before.thread().lineage(),
        before.thread().image_label_frontiers(),
        before.thread().context_owner_id(),
    );
    let advanced_index = DraftByThreadRecord::new(
        before.thread().id(),
        before.draft().id(),
        before.draft().revision(),
        next_thread_revision,
    );
    let mut host = SyndicComposerHost::new(storage);
    host.test_arm_activation_after_selector_fault(move |store, storage| {
        let mut batch = FixtureBatch::new();
        batch.put(FixtureRecord::Thread(advanced_thread)).unwrap();
        batch
            .put(FixtureRecord::DraftByThread(advanced_index))
            .unwrap();
        committed(execute(
            store,
            storage.fixture_contribution(storage.revision(store).unwrap(), batch),
        ));
    });
    assert!(matches!(
        host.activate(
            &store,
            activation(thread, 104, 105, Vec::new()),
            &CommandCancellation::new(),
        ),
        Ok(ComposerHostActivationOutcome::SelectorConflict(_))
    ));
    assert_eq!(host.binding(), None);
}

#[cfg(feature = "test-faults")]
#[test]
fn current_marker_reads_stay_stable_while_exact_candidate_reads_reject_drift() {
    for proof in [false, true] {
        let (_home, store, storage, thread) =
            fixture("current-marker-drift", 110 + u8::from(proof));
        let mut host = SyndicComposerHost::new(storage);
        let ComposerHostActivationOutcome::Activated { binding, .. } = host
            .activate(
                &store,
                activation(
                    thread,
                    118 + u8::from(proof),
                    120 + u8::from(proof),
                    Vec::new(),
                ),
                &CommandCancellation::new(),
            )
            .unwrap()
        else {
            panic!("fixture activation failed");
        };
        let session = match storage
            .draft_editor_candidate_session(
                &store,
                binding.candidate().draft_id(),
                binding.candidate().session_id(),
            )
            .unwrap()
        {
            DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
            other => panic!("candidate session was not active: {other:?}"),
        };
        let candidate_settlement = transaction_for_session(
            storage,
            session,
            122 + u8::from(proof),
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("candidate".to_owned())],
            )],
            point(9),
        );
        arm_draft_piece_current_read_fault(move |store, storage| {
            run_transaction(storage, store, &candidate_settlement, 2);
        });
        let current_kind = if proof {
            ComposerHostRequestKind::MarkerProof {
                target: ComposerHostReadTarget::Current(thread),
                request: DraftPieceMarkerEdgeProofRequestV1::Absence { anchor: 0 },
                retained_byte_ceiling: 9,
            }
        } else {
            ComposerHostRequestKind::Markers {
                target: ComposerHostReadTarget::Current(thread),
                demand: DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::ExactAnchor(0),
                    DraftPieceMarkerDirectionV1::Forward,
                    None,
                    1,
                    65_536,
                ),
            }
        };
        let current_response = run(&mut host, &store, binding, 1, current_kind).unwrap();
        match current_response {
            ComposerHostResponseValue::CurrentMarkerProof(Some(value)) => assert_eq!(
                value.value(),
                &Some(DraftPieceMarkerEdgeProofV1::Absence { anchor: 0 })
            ),
            ComposerHostResponseValue::CurrentMarkers(Some(value)) => {
                assert!(value.value().markers().is_empty());
            }
            other => panic!("current marker read was not stable: {other:?}"),
        }
        assert!(matches!(
            run(
                &mut host,
                &store,
                binding,
                2,
                ComposerHostRequestKind::Text {
                    target: ComposerHostReadTarget::Historical(binding.root()),
                    demand: DraftPieceTextDemandV1::Forward(0),
                    max_bytes: 65_536,
                },
            ),
            Ok(ComposerHostResponseValue::HistoricalText(_))
        ));

        let (_home, store, storage, thread) =
            fixture("candidate-marker-drift", 124 + u8::from(proof));
        let (left, _) = populate(storage, &store, thread, 126 + u8::from(proof));
        let mut host = SyndicComposerHost::new(storage);
        let ComposerHostActivationOutcome::Activated { binding, .. } = host
            .activate(
                &store,
                activation(
                    thread,
                    126 + u8::from(proof),
                    136 + u8::from(proof),
                    Vec::new(),
                ),
                &CommandCancellation::new(),
            )
            .unwrap()
        else {
            panic!("fixture activation failed");
        };
        let session = match storage
            .draft_editor_candidate_session(
                &store,
                binding.candidate().draft_id(),
                binding.candidate().session_id(),
            )
            .unwrap()
        {
            DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
            other => panic!("candidate session was not active: {other:?}"),
        };
        let drift = transaction_for_session(
            storage,
            session,
            150 + u8::from(proof),
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("drift".to_owned())],
            )],
            point(5),
        );
        arm_draft_piece_candidate_read_fault(move |store, storage| {
            run_transaction(storage, store, &drift, 2);
        });
        let candidate_kind = if proof {
            ComposerHostRequestKind::MarkerProof {
                target: ComposerHostReadTarget::Candidate,
                request: DraftPieceMarkerEdgeProofRequestV1::First {
                    marker: DraftPieceMarkerAtV1::new(3, left),
                },
                retained_byte_ceiling: 41,
            }
        } else {
            ComposerHostRequestKind::Markers {
                target: ComposerHostReadTarget::Candidate,
                demand: marker_demand(None, 1, 65_536),
            }
        };
        assert!(matches!(
            run(&mut host, &store, binding, 1, candidate_kind),
            Err(ComposerHostError::Range(
                DraftPieceRangeSourceErrorV1::ConcurrentChange
            ))
        ));
    }
}

fn activation(
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
    first_demands: Vec<ComposerHostInitialDemand>,
) -> ComposerHostActivationRequest {
    ComposerHostActivationRequest::new(
        thread,
        DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        NonZeroU64::MIN,
        None,
        first_demands.into_boxed_slice(),
    )
}

fn request_id(value: u64) -> ComposerHostRequestId {
    ComposerHostRequestId::new(NonZeroU64::new(value).unwrap())
}

fn key(binding: beryl_app::composer_host::ComposerHostBinding, id: u64) -> ComposerHostRequestKey {
    ComposerHostRequestKey::new(
        binding,
        request_id(id),
        ComposerHostRequestPurpose::Viewport,
    )
}

fn marker_demand(
    cursor: Option<syndic_storage::DraftCompositeSearchKeyV1>,
    count: usize,
    bytes: usize,
) -> DraftPieceMarkerDemandV1 {
    DraftPieceMarkerDemandV1::new(
        DraftPieceMarkerScopeV1::ExactAnchor(3),
        DraftPieceMarkerDirectionV1::Forward,
        cursor,
        count,
        bytes,
    )
}

fn text_request() -> ComposerHostRequestKind {
    ComposerHostRequestKind::Text {
        target: ComposerHostReadTarget::Candidate,
        demand: DraftPieceTextDemandV1::Forward(0),
        max_bytes: 4,
    }
}

fn run(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: beryl_app::composer_host::ComposerHostBinding,
    id: u64,
    kind: ComposerHostRequestKind,
) -> Result<ComposerHostResponseValue, ComposerHostError> {
    let pending = host.begin_request(key(binding, id), kind)?;
    let execution = host.execute_pending(store, pending);
    host.complete_request(execution)
        .map(|response| response.value().clone())
}
