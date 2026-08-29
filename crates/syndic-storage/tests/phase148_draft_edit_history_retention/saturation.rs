use super::{common::commit_edit, support::*};

#[test]
fn cumulative_keys_and_oldest_first_floor_repeat_at_exact_saturation() {
    let (_measure_home, measure_store, measure_storage, measure_thread) =
        fixture("retention-measure", 20, 65_536);
    let durable = current(&measure_storage, &measure_store, measure_thread);
    let initial = open_session(&measure_storage, &measure_store, &durable, 22, 23);
    let first = commit_edit(&measure_storage, &measure_store, &initial, 24, "a");
    let second = commit_edit(
        &measure_storage,
        &measure_store,
        first.adopted_session(),
        25,
        "variable",
    );
    let third = commit_edit(
        &measure_storage,
        &measure_store,
        second.adopted_session(),
        26,
        "charge",
    );
    let fourth = commit_edit(
        &measure_storage,
        &measure_store,
        third.adopted_session(),
        27,
        "boundary",
    );
    let first_components =
        draft_edit_history_stored_charge_components(first.adopted_history(), first.transition())
            .unwrap();
    let second_components =
        draft_edit_history_stored_charge_components(second.adopted_history(), second.transition())
            .unwrap();
    let first_charge = first_components[3] + first_components[5];
    let second_charge = second_components[3] + second_components[5];
    assert_ne!(first_charge, second_charge);
    assert_eq!(
        first.transition().key().cumulative_encoded_bytes(),
        first_charge
    );
    assert_eq!(
        second.transition().key().cumulative_encoded_bytes(),
        first_charge + second_charge
    );
    let required = |adoption: &syndic_storage::DraftPieceCommittedAdoptionV1| {
        let components = draft_edit_history_stored_charge_components(
            adoption.adopted_history(),
            adoption.transition(),
        )
        .unwrap();
        components[0] + components[2] + components[3] + components[5]
    };
    let exact_saturated_budget = [
        required(&first),
        required(&second),
        required(&third),
        required(&fourth),
    ]
    .into_iter()
    .max()
    .unwrap();

    let (home, store, storage, thread) =
        fixture("retention-saturation", 40, exact_saturated_budget);
    let durable = current(&storage, &store, thread);
    let mut head = open_session(&storage, &store, &durable, 42, 43);
    let mut retained: Vec<syndic_storage::DraftEditHistoryTransitionV1> = Vec::new();
    for (operation, text) in [(44, "a"), (45, "bb"), (46, "ccc"), (47, "dddd")] {
        let adoption = commit_edit(&storage, &store, &head, operation, text);
        let transition = adoption.transition().clone();
        assert_eq!(
            transition.key().cumulative_encoded_bytes(),
            transition.cumulative_encoded_bytes()
        );
        assert!(adoption.adopted_history().retained_encoded_bytes() <= exact_saturated_budget);
        if !retained.is_empty() {
            assert_eq!(
                adoption.adopted_history().oldest_eligible(),
                Some(transition.reference())
            );
            let components = draft_edit_history_stored_charge_components(
                adoption.adopted_history(),
                &transition,
            )
            .unwrap();
            assert_eq!(
                adoption.adopted_history().retained_encoded_bytes(),
                components[0] + components[2] + components[3] + components[5]
            );
        }
        for prior in &retained {
            assert!(draft_edit_history_transition_exists(
                &store,
                storage.clone(),
                prior.key()
            ));
            assert!(draft_edit_history_root_exists(
                &store,
                storage.clone(),
                prior.predecessor_root()
            ));
            assert!(draft_edit_history_root_exists(
                &store,
                storage.clone(),
                prior.successor_root()
            ));
        }
        head = adoption.adopted_session().clone();
        retained.push(transition);
    }

    drop(store);
    let mut reopened = open(&home);
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert!(matches!(
        reopened_storage
            .draft_editor_candidate_session(&reopened, head.draft_id(), head.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(value) if value == head
    ));
    let fresh = open_session(&reopened_storage, &reopened, &durable, 52, 53);
    assert_eq!(fresh.newest_root(), durable.draft().piece_root());
    assert!(!fresh.newest_history().availability().undo_available());
}

#[test]
fn variable_charge_cutoff_straddle_advances_to_the_next_transition_boundary() {
    let (_measure_home, measure_store, measure_storage, measure_thread) =
        fixture("retention-straddle-measure", 54, 65_536);
    let durable = current(&measure_storage, &measure_store, measure_thread);
    let session = open_session(&measure_storage, &measure_store, &durable, 55, 56);
    let first = commit_edit(&measure_storage, &measure_store, &session, 57, "a");
    let second = commit_edit(
        &measure_storage,
        &measure_store,
        first.adopted_session(),
        58,
        "variable",
    );
    let third = commit_edit(
        &measure_storage,
        &measure_store,
        second.adopted_session(),
        59,
        "charge",
    );
    let fourth = commit_edit(
        &measure_storage,
        &measure_store,
        third.adopted_session(),
        60,
        "boundary",
    );
    let charge = |adoption: &syndic_storage::DraftPieceCommittedAdoptionV1| {
        let components = draft_edit_history_stored_charge_components(
            adoption.adopted_history(),
            adoption.transition(),
        )
        .unwrap();
        components[3] + components[5]
    };
    let first_charge = charge(&first);
    let second_charge = charge(&second);
    let third_charge = charge(&third);
    let fourth_components =
        draft_edit_history_stored_charge_components(fourth.adopted_history(), fourth.transition())
            .unwrap();
    let fourth_charge = fourth_components[3] + fourth_components[5];
    let frontier_charge = fourth_components[0] + fourth_components[2];
    assert!(second_charge > 1);
    assert!(fourth_charge > first_charge);
    let straddling_budget = frontier_charge + second_charge + third_charge + fourth_charge - 1;

    let (_home, store, storage, thread) = fixture("retention-straddle", 61, straddling_budget);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 62, 63);
    let first = commit_edit(&storage, &store, &session, 64, "a");
    let second = commit_edit(&storage, &store, first.adopted_session(), 65, "variable");
    let third = commit_edit(&storage, &store, second.adopted_session(), 66, "charge");
    let fourth = commit_edit(&storage, &store, third.adopted_session(), 67, "boundary");

    assert_eq!(
        fourth.adopted_history().oldest_eligible(),
        Some(third.transition().reference())
    );
    assert_eq!(
        fourth.adopted_history().retained_encoded_bytes(),
        frontier_charge + charge(&third) + charge(&fourth)
    );
    assert!(fourth.adopted_history().retained_encoded_bytes() <= straddling_budget);
    assert!(draft_edit_history_transition_exists(
        &store,
        storage.clone(),
        first.transition().key()
    ));
    assert!(draft_edit_history_transition_exists(
        &store,
        storage.clone(),
        second.transition().key()
    ));
}

#[test]
fn cumulative_seek_spans_a_nonempty_session_fork_and_reopens() {
    let (_measure_home, measure_store, measure_storage, measure_thread) =
        fixture("fork-saturation-measure", 200, 65_536);
    let durable = current(&measure_storage, &measure_store, measure_thread);
    let session = open_session(&measure_storage, &measure_store, &durable, 201, 202);
    let first = commit_edit(&measure_storage, &measure_store, &session, 203, "a");
    let second = commit_edit(
        &measure_storage,
        &measure_store,
        first.adopted_session(),
        204,
        "variable",
    );
    let third = commit_edit(
        &measure_storage,
        &measure_store,
        second.adopted_session(),
        205,
        "charge",
    );
    let fourth = commit_edit(
        &measure_storage,
        &measure_store,
        third.adopted_session(),
        206,
        "boundary",
    );
    let components = |adoption: &syndic_storage::DraftPieceCommittedAdoptionV1| {
        draft_edit_history_stored_charge_components(
            adoption.adopted_history(),
            adoption.transition(),
        )
        .unwrap()
    };
    let second_components = components(&second);
    let third_components = components(&third);
    let fourth_components = components(&fourth);
    let charge = |value: [u64; 6]| value[3] + value[5];
    let budget = fourth_components[0]
        + fourth_components[2]
        + charge(second_components)
        + charge(third_components)
        + charge(fourth_components)
        - 1;

    let (home, store, storage, thread) = fixture("fork-saturation", 210, budget);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 211, 212);
    let first = commit_edit(&storage, &store, &session, 213, "a");
    let second = commit_edit(&storage, &store, first.adopted_session(), 214, "variable");
    committed(execute(
        &store,
        publish_draft_edit_history_pair(
            &store,
            storage.clone(),
            durable.draft().clone(),
            second.adopted_root().reference(),
            second.adopted_history().reference(),
        ),
    ));
    let published = current(&storage, &store, thread);
    let forked = open_session(&storage, &store, &published, 215, 216);
    let third = commit_edit(&storage, &store, &forked, 217, "charge");
    let fourth = commit_edit(&storage, &store, third.adopted_session(), 218, "boundary");
    assert_eq!(
        fourth.adopted_history().oldest_eligible(),
        Some(third.transition().reference())
    );
    assert_ne!(
        second.transition().key().session_id(),
        third.transition().key().session_id()
    );
    assert!(draft_edit_history_transition_exists(
        &store,
        storage.clone(),
        second.transition().key()
    ));

    drop(store);
    let mut reopened = open(&home);
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert!(matches!(
        reopened_storage
            .draft_editor_candidate_session(
                &reopened,
                fourth.adopted_session().draft_id(),
                fourth.adopted_session().session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(value)
            if value == *fourth.adopted_session()
    ));
}
