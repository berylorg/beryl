use super::*;

#[test]
fn activation_and_bounded_range_custody_are_exact() {
    let (_home, store, storage, thread) = fixture("range-custody", 1);
    let (left, right) = populate(storage.clone(), &store, thread, 10);
    let first_marker_demand = marker_demand(None, 1, 65_536);
    let mut host = SyndicComposerHost::new(storage.clone());
    let activation = host
        .test_activate(
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

    (|| {
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
    })();

    (|| {
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
    })();

    (|| {
        let seed = ComposerHostRestorationSeed::new(
            root,
            binding.history(),
            binding.logical_extent(),
            point(0),
            point(0),
            point(0),
        );
        let current = current(storage, &store, thread);
        let current_root = current.draft().piece_root();
        let current_seed = ComposerHostRestorationSeed::new(
            current_root,
            current.draft().history(),
            current_root.summary().logical_extent(),
            point(0),
            point(0),
            point(0),
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
        host.dispose_composer_service(&store).unwrap();
        assert!(matches!(
            host.complete_request(execution),
            Err(ComposerHostError::RequestNotPending)
        ));
        assert_eq!(host.binding(), None);
        assert_eq!(host.pending_request_count(), 0);
        assert!(host.initial_responses().is_empty());
    })();
}
