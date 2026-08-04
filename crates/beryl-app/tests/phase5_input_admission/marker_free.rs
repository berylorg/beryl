use super::*;

#[test]
fn idle_and_accepted_admission_validate_exact_owner_absence() {
    let fixture = Fixture::new(40);
    fixture.publish_marker_free(2);
    let current = fixture
        .syndic
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let user_item = SyndicItemId::from_bytes([42; 16]);
    let next_draft = SyndicDraftId::from_bytes([43; 16]);
    let submission = IdleSubmission::new(
        fixture.thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        InputGateRevision::new(1).unwrap(),
        next_draft,
        user_item,
        None,
        time(3),
    );
    let command = idle_submission_command(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        submission,
    )
    .unwrap();
    fixture.store.execute(command).unwrap();
    assert!(
        fixture
            .state
            .assets()
            .owner_head(&fixture.store, AssetOwner::SubmittedTurnItem(user_item))
            .unwrap()
            .is_none()
    );

    fixture.publish_text("accepted marker free", 4);
    let current = fixture
        .syndic
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let accepted = AcceptedInputAdmission::new(
        fixture.thread,
        current.thread().revision(),
        next_draft,
        current.draft().revision(),
        current.draft().content(),
        InputGateRevision::new(2).unwrap(),
        SyndicDraftId::from_bytes([44; 16]),
        None,
        time(5),
    );
    let input = accepted.accepted_input_id();
    let prepared = prepare_accepted_input_admission(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        accepted,
    )
    .unwrap();
    fixture
        .store
        .execute_accepted_input_admission(prepared)
        .unwrap();
    assert!(
        fixture
            .state
            .assets()
            .owner_head(&fixture.store, AssetOwner::AcceptedInput(input))
            .unwrap()
            .is_none()
    );
}

#[test]
fn admission_rejects_either_stray_owner_head_atomically() {
    for (seed, stray_destination) in [(50, false), (60, true)] {
        let mut fixture = Fixture::new(seed);
        let marker = SyndicDraftMarkerId::from_bytes([seed.wrapping_add(2); 16]);
        fixture.publish_marker(marker, 2);
        let draft = fixture.draft;
        let (_asset, proof) = admit_asset(&mut fixture, marker, b"stray", draft, seed);
        fixture.publish_marker_free(3);
        let item = SyndicItemId::from_bytes([seed.wrapping_add(3); 16]);
        if stray_destination {
            let assets = fixture.state.assets();
            let source = assets
                .owner_head(&fixture.store, AssetOwner::CurrentDraft(draft))
                .unwrap()
                .unwrap()
                .expectation();
            execute_one(
                &fixture.store,
                assets.update_owner_heads(
                    assets.revision(&fixture.store).unwrap(),
                    UpdateAssetOwnerHeads::new(
                        vec![
                            AssetOwnerHeadUpdate::replace(
                                AssetOwner::CurrentDraft(draft),
                                Some(source),
                                None,
                            ),
                            AssetOwnerHeadUpdate::replace(
                                AssetOwner::SubmittedTurnItem(item),
                                None,
                                Some(proof),
                            ),
                        ]
                        .into_boxed_slice(),
                    )
                    .unwrap(),
                ),
            );
        }
        let current = fixture
            .syndic
            .current_draft(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        let submission = IdleSubmission::new(
            fixture.thread,
            current.thread().revision(),
            draft,
            current.draft().revision(),
            current.draft().content(),
            InputGateRevision::new(1).unwrap(),
            SyndicDraftId::from_bytes([seed.wrapping_add(4); 16]),
            item,
            None,
            time(4),
        );
        let command = idle_submission_command(
            &fixture.store,
            fixture.syndic,
            fixture.state.assets(),
            submission,
        )
        .unwrap();
        assert!(fixture.store.execute(command).is_err());
        assert!(
            fixture
                .syndic
                .turn(&fixture.store, draft.submitted_turn_id(), point_limit())
                .unwrap()
                .is_none()
        );
    }
}
