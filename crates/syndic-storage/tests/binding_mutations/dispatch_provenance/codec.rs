use super::*;

#[test]
fn old_turn_state_record_is_rejected_without_inferred_provenance() {
    let home = TestHome::new("turn-state-legacy-version");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (_, _, turn, _) = same_home_pending_path(&store, &storage, 170);
    let prior = state(&store, &storage, turn);
    assert_eq!(
        prior.dispatch_provenance(),
        TurnDispatchProvenance::Unattempted
    );
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    test_faults::inject_turn_state_without_dispatch_provenance(&store, storage.clone(), &prior)
        .unwrap();
    assert!(storage.turn_state(&store, turn, point_limit()).is_err());
    assert!(
        store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
    drop(store);
}

#[test]
fn dispatch_codec_has_closed_tags_and_exact_nonzero_anchors() {
    let anchor = TurnDispatchAnchor::new(
        SyndicExecutionSnapshotId::from_bytes([190; 16]),
        BindingRevision::new(7).unwrap(),
    );
    for (tag, provenance) in [
        (0, TurnDispatchProvenance::Unattempted),
        (1, TurnDispatchProvenance::Activated(anchor)),
        (2, TurnDispatchProvenance::Cancelled(anchor)),
        (3, TurnDispatchProvenance::ProviderOperation),
    ] {
        let state = TurnStateRecord::new(
            SyndicTurnId::from_bytes([170; 16]),
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            0,
            None,
            timestamp(1),
            provenance,
        )
        .unwrap();
        let encoded = test_faults::turn_state_codec_bytes(&state);
        assert_eq!(
            test_faults::decode_turn_state_for_test(&encoded),
            Some(state)
        );
        let anchor_bytes = if tag == 1 || tag == 2 { 24 } else { 0 };
        let tag_offset = encoded.len() - anchor_bytes - 1;
        assert_eq!(encoded[tag_offset], tag);
        let mut bad_tag = encoded.clone();
        bad_tag[tag_offset] = 4;
        assert!(test_faults::decode_turn_state_for_test(&bad_tag).is_none());
        for end in tag_offset..encoded.len() {
            assert!(test_faults::decode_turn_state_for_test(&encoded[..end]).is_none());
        }
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(test_faults::decode_turn_state_for_test(&trailing).is_none());
        if anchor_bytes != 0 {
            let mut zero_revision = encoded.clone();
            let length = zero_revision.len();
            zero_revision[length - 8..].fill(0);
            assert!(test_faults::decode_turn_state_for_test(&zero_revision).is_none());
        }
    }
}
