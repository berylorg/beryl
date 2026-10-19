use super::*;
use beryl_app::composer_host::{ComposerAcceptedInputHistoryOwner, ComposerHostRecallAdvance};
use gpui_text_input::{MutationKind, MutationPositions};
use std::sync::Arc;
#[path = "../syndic_composer_history/fresh_marker.rs"]
mod markers;
#[path = "../accepted_input_history/support.rs"]
mod recall_support;
use recall_support::{recall, submit};

#[test]
fn accepted_history_recalls_over_unsent_text_and_restores_the_original_directed_selection() {
    let (_home, store, state, storage, thread) = fixture_with_state("ordinary-recall", 11);
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let owner = Arc::new(ComposerAcceptedInputHistoryOwner::default());
    let (mut first, empty) = activated(storage.clone(), &store, thread, 12, 13);
    first.bind_accepted_input_history(owner.clone());
    commit_text(&mut first, &store, empty, 1, 0, 0, "accepted first", 14, 1);
    submit(&mut first, &store, &assets, &seals, 20);
    assert_eq!(owner.entry_count(store.home_id(), thread), 1);
    let source = owner.entry(store.home_id(), thread, 0).unwrap();
    let (mut host, empty) = activated(storage.clone(), &store, thread, 30, 31);
    let unsent = commit_text(&mut host, &store, empty, 1, 0, 0, "unsent second", 13, 1);
    let original_positions = MutationPositions::new(
        composer::position(3),
        composer::position(11),
        composer::position(3),
    );
    let ticket = host
        .begin_accepted_input_recall(
            &store,
            unsent,
            source.clone(),
            original_positions,
            operation_id(400),
            true,
        )
        .unwrap();
    let completion = recall(&mut host, &store, &assets, ticket);
    assert_eq!(
        composer::candidate_text(storage.clone(), &store, completion.binding()),
        b"accepted first"
    );
    assert_eq!(
        completion.positions(),
        MutationPositions::collapsed(composer::position(14))
    );
    assert!(completion.binding().range_history_frontier().undo_available);
    assert_eq!(
        host.submission_intent(),
        Some(syndic_storage::DraftSubmissionIntent::Ordinary)
    );
    assert!(host.is_dirty());
    let checkpoint = completion.checkpoint().unwrap().clone();
    let recalled = completion.binding();
    let edited = commit_text(&mut host, &store, recalled, 2, 14, 14, "!", 15, 1);
    let undone = composer::select_history(&mut host, &store, edited, 3, MutationKind::Undo);
    let ticket = host
        .begin_composer_checkpoint_restore(
            &store,
            undone,
            checkpoint,
            MutationPositions::collapsed(composer::position(14)),
            operation_id(401),
        )
        .unwrap();
    let restored = recall(&mut host, &store, &assets, ticket);
    assert_eq!(
        composer::candidate_text(storage.clone(), &store, restored.binding()),
        b"unsent second"
    );
    assert_eq!(restored.positions(), original_positions);
    assert_eq!(owner.entry(store.home_id(), thread, 0), Some(source));
    assert!(!restored.binding().range_history_frontier().redo_available);
    let undone =
        composer::select_history(&mut host, &store, restored.binding(), 4, MutationKind::Undo);
    assert_eq!(
        composer::candidate_text(storage, &store, undone),
        b"accepted first"
    );
}

#[test]
fn exact_acceptance_collapses_duplicates_and_evicts_entries_then_close_rejects_late_capture() {
    let (_home, store, state, storage, thread) = fixture_with_state("history-bounds", 71);
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let owner = Arc::new(ComposerAcceptedInputHistoryOwner::new(
        std::num::NonZeroUsize::new(2).unwrap(),
        std::num::NonZeroUsize::new(1).unwrap(),
    ));
    for (index, text) in ["same", "same", "next", "last"].iter().enumerate() {
        let seed = 80 + index as u8 * 10;
        let (mut host, empty) = activated(storage.clone(), &store, thread, seed, seed + 1);
        host.bind_accepted_input_history(owner.clone());
        commit_text(
            &mut host,
            &store,
            empty,
            1,
            0,
            0,
            text,
            text.len() as u64,
            1,
        );
        submit(&mut host, &store, &assets, &seals, seed + 2);
        assert_eq!(
            owner.entry_count(store.home_id(), thread),
            [1, 1, 2, 2][index]
        );
    }
    let (mut host, empty) = activated(storage.clone(), &store, thread, 130, 131);
    host.bind_accepted_input_history(owner.clone());
    commit_text(&mut host, &store, empty, 1, 0, 0, "late", 4, 1);
    owner.close();
    submit(&mut host, &store, &assets, &seals, 132);
    assert_eq!(owner.entry_count(store.home_id(), thread), 0);
    assert_eq!(owner.scope_count(), 0);
}

#[test]
fn cancellation_after_staging_preserves_the_root_history_and_positions() {
    let (_home, store, state, storage, thread) = fixture_with_state("recall-cancel", 151);
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let owner = Arc::new(ComposerAcceptedInputHistoryOwner::default());
    let (mut first, empty) = activated(storage.clone(), &store, thread, 152, 153);
    first.bind_accepted_input_history(owner.clone());
    commit_text(&mut first, &store, empty, 1, 0, 0, "accepted", 8, 1);
    submit(&mut first, &store, &assets, &seals, 160);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 170, 171);
    let original = commit_text(&mut host, &store, empty, 1, 0, 0, "keep", 4, 1);
    let positions = MutationPositions::collapsed(composer::position(2));
    let ticket = host
        .begin_accepted_input_recall(
            &store,
            original,
            owner.entry(store.home_id(), thread, 0).unwrap(),
            positions,
            operation_id(700),
            true,
        )
        .unwrap();
    assert!(matches!(
        host.advance_accepted_input_recall(&store, ticket, &assets, &CommandCancellation::new())
            .unwrap(),
        ComposerHostRecallAdvance::Progress
    ));
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    for _ in 0..64 {
        match host
            .advance_accepted_input_recall(&store, ticket, &assets, &cancellation)
            .unwrap()
        {
            ComposerHostRecallAdvance::Progress => {}
            ComposerHostRecallAdvance::Ready(completion) => {
                assert!(!completion.replaced());
                assert_eq!(completion.binding().root(), original.root());
                assert_eq!(completion.binding().history(), original.history());
                assert_eq!(completion.positions(), positions);
                host.acknowledge_accepted_input_recall(ticket, completion.binding())
                    .unwrap();
                return;
            }
            ComposerHostRecallAdvance::NotCommitted => {
                panic!("staged cancellation must update custody metadata")
            }
        }
    }
    panic!("cancellation did not settle")
}

#[test]
fn a_foreign_home_source_never_changes_the_destination_candidate() {
    let (_source_home, source_store, source_state, source_storage, thread) =
        fixture_with_state("recall-source-home", 191);
    let owner = Arc::new(ComposerAcceptedInputHistoryOwner::default());
    let assets = source_state.assets();
    let seals = service(&source_store, source_storage.clone(), assets.clone(), 1, 1);
    let (mut source, empty) = activated(source_storage, &source_store, thread, 192, 193);
    source.bind_accepted_input_history(owner.clone());
    commit_text(&mut source, &source_store, empty, 1, 0, 0, "foreign", 7, 1);
    submit(&mut source, &source_store, &assets, &seals, 200);
    let reference = owner.entry(source_store.home_id(), thread, 0).unwrap();
    let (_destination_home, store, _state, storage, destination_thread) =
        fixture_with_state("recall-destination-home", 191);
    assert_eq!(thread, destination_thread);
    let (mut destination, empty) = activated(storage, &store, thread, 212, 213);
    assert!(
        destination
            .begin_accepted_input_recall(
                &store,
                empty,
                reference,
                MutationPositions::collapsed(composer::position(0)),
                operation_id(900),
                true
            )
            .is_err()
    );
    assert_eq!(destination.binding(), Some(empty));
    assert!(!destination.accepted_input_recall_pending());
}

#[test]
fn large_accepted_text_is_recalled_through_bounded_steps_into_another_draft_namespace() {
    let (_home, store, state, storage, thread) = fixture_with_state("recall-large-text", 221);
    let owner = Arc::new(ComposerAcceptedInputHistoryOwner::default());
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut source, mut binding) = activated(storage.clone(), &store, thread, 222, 223);
    source.bind_accepted_input_history(owner.clone());
    let chunk = "bounded recall text ".repeat(200);
    let mut length = 0;
    for operation in 1..=32 {
        binding = commit_text(
            &mut source,
            &store,
            binding,
            operation,
            length,
            length,
            &chunk,
            length + chunk.len() as u64,
            1,
        );
        length += chunk.len() as u64;
    }
    submit(&mut source, &store, &assets, &seals, 230);
    let reference = owner.entry(store.home_id(), thread, 0).unwrap();
    assert_eq!(reference.content().summary().logical_utf8_bytes(), length);
    let (mut destination, empty) = activated(storage.clone(), &store, thread, 240, 241);
    let ticket = destination
        .begin_accepted_input_recall(
            &store,
            empty,
            reference,
            MutationPositions::collapsed(composer::position(0)),
            operation_id(1000),
            true,
        )
        .unwrap();
    let completion = recall(&mut destination, &store, &assets, ticket);
    assert_ne!(
        completion.binding().candidate().draft_id(),
        binding.candidate().draft_id()
    );
    assert_eq!(
        completion.binding().logical_extent().logical_utf8_bytes(),
        length
    );
    assert_eq!(
        completion.positions(),
        MutationPositions::collapsed(composer::position(length))
    );
    assert_eq!(owner.entry_count(store.home_id(), thread), 1);
}

#[test]
fn recalled_marker_retains_exact_identity_label_and_asset_across_another_acceptance() {
    recall_marker(false);
}

#[test]
fn recalled_marker_followed_by_text_places_the_caret_at_the_unambiguous_text_end() {
    recall_marker(true);
}

fn recall_marker(trailing_text: bool) {
    let fixture = markers::fixture("recall-marker", 41);
    let owner = Arc::new(ComposerAcceptedInputHistoryOwner::default());
    let (mut source, empty) = markers::activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        42,
        43,
    );
    source.bind_accepted_input_history(owner.clone());
    let asset = markers::publish_image_asset(
        &fixture.store,
        fixture.assets.clone(),
        b"ordinary-recall-image",
    );
    let (marked, _, after) =
        markers::admit_fresh_marker(&fixture, &mut source, empty, 1, asset, true);
    let marked = if trailing_text {
        composer::commit_text_at_positions(
            &mut source,
            &fixture.store,
            marked,
            2,
            after,
            after,
            "trailing text",
            13,
            1,
        )
    } else {
        marked
    };
    let original = fixture
        .storage
        .draft_piece_marker_demand(
            &fixture.store,
            marked.root(),
            syndic_storage::DraftPieceMarkerDemandV1::new(
                syndic_storage::DraftPieceMarkerScopeV1::ExactAnchor(0),
                syndic_storage::DraftPieceMarkerDirectionV1::Forward,
                None,
                1,
                4096,
            ),
        )
        .unwrap();
    let original = original.markers()[0].marker().clone();
    let seals = service(
        &fixture.store,
        fixture.storage.clone(),
        fixture.assets.clone(),
        1,
        1,
    );
    submit(&mut source, &fixture.store, &fixture.assets, &seals, 50);
    let reference = owner
        .entry(fixture.store.home_id(), fixture.thread, 0)
        .unwrap();
    let (mut host, empty) = markers::activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        60,
        61,
    );
    host.bind_accepted_input_history(owner.clone());
    let ticket = host
        .begin_accepted_input_recall(
            &fixture.store,
            empty,
            reference.clone(),
            MutationPositions::collapsed(composer::position(0)),
            operation_id(2000),
            true,
        )
        .unwrap();
    let completion = recall(&mut host, &fixture.store, &fixture.assets, ticket);
    let page = fixture
        .storage
        .draft_piece_marker_demand(
            &fixture.store,
            completion.binding().root(),
            syndic_storage::DraftPieceMarkerDemandV1::new(
                syndic_storage::DraftPieceMarkerScopeV1::ExactAnchor(0),
                syndic_storage::DraftPieceMarkerDirectionV1::Forward,
                None,
                1,
                4096,
            ),
        )
        .unwrap();
    assert_eq!(page.markers()[0].marker().marker_id(), original.marker_id());
    assert_eq!(page.markers()[0].marker().label(), original.label());
    if trailing_text {
        assert_eq!(
            completion.positions(),
            MutationPositions::collapsed(composer::position(13))
        );
        assert_eq!(
            composer::candidate_text(
                fixture.storage.clone(),
                &fixture.store,
                completion.binding()
            ),
            b"trailing text"
        );
    } else {
        assert!(matches!(
            completion.positions().caret().gap,
            gpui_text_input::InlineObjectGap::After(_)
        ));
    }
    submit(&mut host, &fixture.store, &fixture.assets, &seals, 70);
    assert_eq!(
        owner.entry_count(fixture.store.home_id(), fixture.thread),
        1
    );
    let association = fixture
        .assets
        .marker_reference(
            &fixture.store,
            reference.asset_reference_set().unwrap(),
            original.marker_id(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(association.asset_id(), asset);
}

#[test]
fn healthy_capacity_refusal_terminates_staging_and_leaves_the_original_editor_writable() {
    use beryl_app::composer_host::ComposerHostRecallRefusal;
    let (_home, store, state, storage, thread) = fixture_with_state("recall-capacity-refusal", 31);
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let owner = Arc::new(ComposerAcceptedInputHistoryOwner::default());
    let (mut first, empty) = activated(storage.clone(), &store, thread, 32, 33);
    first.bind_accepted_input_history(owner.clone());
    commit_text(&mut first, &store, empty, 1, 0, 0, "accepted", 8, 1);
    submit(&mut first, &store, &assets, &seals, 40);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 50, 51);
    let original = commit_text(&mut host, &store, empty, 1, 0, 0, "keep", 4, 1);
    let positions = MutationPositions::collapsed(composer::position(2));
    let ticket = host
        .begin_accepted_input_recall(
            &store,
            original,
            owner.entry(store.home_id(), thread, 0).unwrap(),
            positions,
            operation_id(9000),
            true,
        )
        .unwrap();
    host.test_refuse_next_recall_preparation(ComposerHostRecallRefusal::CapacityUnavailable);
    let completion = recall(&mut host, &store, &assets, ticket);
    assert_eq!(
        completion.refusal(),
        Some(ComposerHostRecallRefusal::CapacityUnavailable)
    );
    assert!(!completion.replaced());
    assert_eq!(completion.binding().root(), original.root());
    assert_eq!(completion.binding().history(), original.history());
    assert_eq!(completion.positions(), positions);
    assert_eq!(host.settlement_custody_in_use(), 0);
    let edited = commit_text(
        &mut host,
        &store,
        completion.binding(),
        2,
        4,
        4,
        " writable",
        13,
        1,
    );
    assert_eq!(
        composer::candidate_text(storage, &store, edited),
        b"keep writable"
    );
}
