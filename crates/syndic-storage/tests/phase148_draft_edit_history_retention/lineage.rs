use super::{common::commit_edit, support::*};

#[test]
fn sibling_at_the_threshold_cannot_redirect_selected_lineage_floor() {
    let (_measure_home, measure_store, measure_storage, measure_thread) =
        fixture("sibling-lineage-measure", 91, 65_536);
    let durable = current(measure_storage, &measure_store, measure_thread);
    let session = open_session(measure_storage, &measure_store, &durable, 92, 93);
    let first = commit_edit(measure_storage, &measure_store, &session, 94, "a");
    let second = commit_edit(
        measure_storage,
        &measure_store,
        first.adopted_session(),
        95,
        "variable",
    );
    let third = commit_edit(
        measure_storage,
        &measure_store,
        second.adopted_session(),
        96,
        "charge",
    );
    let fourth = commit_edit(
        measure_storage,
        &measure_store,
        third.adopted_session(),
        97,
        "boundary",
    );
    let components = |adoption: &syndic_storage::DraftPieceCommittedAdoptionV1| {
        draft_edit_history_stored_charge_components(
            adoption.adopted_history(),
            adoption.transition(),
        )
        .unwrap()
    };
    let third_components = components(&third);
    let fourth_components = components(&fourth);
    let charge = |value: [u64; 6]| value[3] + value[5];
    let budget = fourth_components[0]
        + fourth_components[2]
        + charge(third_components)
        + charge(fourth_components);

    let (home, store, storage, thread) = fixture("sibling-lineage", 101, budget);
    let durable = current(storage, &store, thread);
    let base = open_session(storage, &store, &durable, 102, 103);
    let first = commit_edit(storage, &store, &base, 104, "a");
    let second = commit_edit(storage, &store, first.adopted_session(), 105, "variable");
    committed(execute(
        &store,
        publish_draft_edit_history_pair(
            &store,
            storage,
            durable.draft().clone(),
            second.adopted_root().reference(),
            second.adopted_history().reference(),
        ),
    ));
    let published = current(storage, &store, thread);
    let sibling_session = open_session(storage, &store, &published, 106, 107);
    let selected_session = open_session(storage, &store, &published, 108, 109);
    let sibling = commit_edit(storage, &store, &sibling_session, 110, "charge");
    let selected = commit_edit(storage, &store, &selected_session, 111, "charge");
    assert_eq!(
        sibling.transition().cumulative_encoded_bytes(),
        selected.transition().cumulative_encoded_bytes()
    );
    assert_ne!(
        sibling.transition().key().session_id(),
        selected.transition().key().session_id()
    );
    let successor = commit_edit(storage, &store, selected.adopted_session(), 112, "boundary");
    assert_eq!(
        successor.adopted_history().oldest_eligible(),
        Some(selected.transition().reference())
    );
    assert_ne!(
        successor.adopted_history().oldest_eligible(),
        Some(sibling.transition().reference())
    );
    assert!(draft_edit_history_transition_exists(
        &store,
        storage,
        sibling.transition().key()
    ));

    drop(store);
    let mut reopened = open(&home);
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert!(matches!(
        reopened_storage
            .draft_editor_candidate_session(
                &reopened,
                successor.adopted_session().draft_id(),
                successor.adopted_session().session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(value)
            if value == *successor.adopted_session()
    ));
}
#[test]
fn append_constructs_canonical_depth_and_binary_ancestor_witness() {
    let (_home, store, storage, thread) = fixture("witness-construction", 113, 65_536);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 114, 115);
    let first = commit_edit(storage, &store, &session, 116, "a");
    let second = commit_edit(storage, &store, first.adopted_session(), 117, "bb");
    let third = commit_edit(storage, &store, second.adopted_session(), 118, "ccc");
    let fourth = commit_edit(storage, &store, third.adopted_session(), 119, "dddd");

    assert_eq!(first.transition().journal_depth(), 1);
    assert_eq!(first.transition().ancestor_witness().bitmap(), 0);
    assert_eq!(second.transition().journal_depth(), 2);
    assert_eq!(
        second.transition().ancestor_witness().ancestor(0),
        Some(first.transition().reference())
    );
    assert_eq!(third.transition().journal_depth(), 3);
    assert_eq!(
        third.transition().ancestor_witness().ancestor(1),
        Some(first.transition().reference())
    );
    assert_eq!(fourth.transition().journal_depth(), 4);
    assert_eq!(
        fourth.transition().ancestor_witness().ancestor(0),
        Some(third.transition().reference())
    );
    assert_eq!(
        fourth.transition().ancestor_witness().ancestor(1),
        Some(second.transition().reference())
    );
    assert_eq!(fourth.transition().ancestor_witness().ancestor(2), None);
}
