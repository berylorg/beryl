use super::*;

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
            &store,
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
            &store,
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
