use super::*;

fn image_fixture(name: &str) -> Fixture {
    let id = SyndicDraftMarkerId::from_bytes([243; 16]);
    let label = ImageLabelOrdinal::new(13).unwrap();
    Fixture::new(
        name,
        ComposerPayload::new(vec![ComposerAtom::image_marker(id, label)]).unwrap(),
        vec![(id, label)],
        65_536,
    )
}

#[test]
fn final_asset_participant_refuses_historical_and_current_draft_head_disagreement() {
    for historical_changed in [true, false] {
        let f = image_fixture("final-asset-head-disagreement");
        let p = f.prepare();
        begin(&f, &p);
        advance_all(&f, &p);
        let prepared = f
            .storage
            .prepare_replacement_draft_start(&f.store, &p, SyndicTimestamp::from_unix_millis(3))
            .unwrap();
        let before = f.head();
        let draft = ordinary::current(&f.storage, &f.store, f.thread);
        let old = f
            .state
            .assets()
            .owner_head(&f.store, AssetOwner::SubmittedTurnItem(f.item))
            .unwrap()
            .unwrap();
        let update = if historical_changed {
            AssetOwnerHeadUpdate::replace(old.owner(), Some(old.expectation()), None)
        } else {
            AssetOwnerHeadUpdate::replace(
                AssetOwner::CurrentDraft(before.draft_id()),
                None,
                Some(old.set()),
            )
        };
        committed(execute(
            &f.store,
            f.state.assets().update_owner_heads(
                f.state.assets().revision(&f.store).unwrap(),
                UpdateAssetOwnerHeads::new(Box::from([update])).unwrap(),
            ),
        ));
        assert!(matches!(
            f.state.assets().historical_input_owner_selection(
                &f.store,
                prepared.basis().selector().draft_id(),
                f.item,
                f.proof
            ),
            Err(beryl_state::AssetHistoricalInputError::Rejected)
        ));
        assert_eq!(f.head(), before);
        assert_eq!(ordinary::current(&f.storage, &f.store, f.thread), draft);
    }
}

#[test]
fn original_whole_command_mixed_asset_heads_close_as_collision_without_reexecution() {
    for historical_changed in [true, false] {
        let f = image_fixture("original-mixed-head-collision");
        let p = f.prepare();
        begin(&f, &p);
        advance_all(&f, &p);
        let prepared = f
            .storage
            .prepare_replacement_draft_start(&f.store, &p, SyndicTimestamp::from_unix_millis(3))
            .unwrap();
        f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let outcome = f.store.execute(start_command(&f, &prepared));
        assert!(
            matches!(outcome, CommandOutcome::Indeterminate { .. }),
            "collision test needs genuine original custody: {outcome:?}"
        );
        if f.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
            f.faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
        }
        assert!(
            f.storage
                .reconcile_replacement_draft_start(&f.store, &prepared, outcome)
                .is_err()
        );
        assert!(!f.store.pending_reconciliations().is_empty());
        let f = f.recover();
        let owner = if historical_changed {
            AssetOwner::SubmittedTurnItem(f.item)
        } else {
            AssetOwner::CurrentDraft(f.session.draft_id())
        };
        let old = f
            .state
            .assets()
            .owner_head(&f.store, owner)
            .unwrap()
            .unwrap();
        committed(execute(
            &f.store,
            f.state.assets().update_owner_heads(
                f.state.assets().revision(&f.store).unwrap(),
                UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                    owner,
                    Some(old.expectation()),
                    None,
                )]))
                .unwrap(),
            ),
        ));
        let before = (f.head(), f.store.home_revision().unwrap());
        assert!(matches!(
            f.storage
                .resume_replacement_draft_start(&f.store, &prepared),
            Err(DraftReplacementErrorV1::Conflict)
        ));
        assert!(matches!(
            f.storage
                .inspect_replacement_draft_start(&f.store, &prepared),
            Err(DraftReplacementErrorV1::Conflict)
        ));
        assert!(matches!(
            f.storage
                .resume_replacement_draft_start(&f.store, &prepared),
            Err(DraftReplacementErrorV1::Conflict)
        ));
        assert_eq!((f.head(), f.store.home_revision().unwrap()), before);
        assert_eq!(
            f.store.health().state(),
            beryl_home_store::HomeHealthState::Healthy
        );
    }
}
