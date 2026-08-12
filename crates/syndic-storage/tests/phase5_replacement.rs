#![cfg(feature = "test-faults")]

mod support;

#[path = "phase5_replacement/fixtures.rs"]
mod fixtures;

#[path = "phase5_replacement/catalog_title.rs"]
mod catalog_title;

use beryl_home_store::{CommandError, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, SealedAssetReferenceSetProof,
    SyndicDraftMarkerId, SyndicItemId,
};
use syndic_storage::*;

use fixtures::target_asset_reference_set;
use support::exact_cas::{admit_event, correlate_user_item, establish_turn};
use support::populated::source_turn;
use support::{
    TestHome, converge_and_release_terminal_history, draft_id, id, open, read_composer_payload,
    stage_prepared_content, timestamp,
};

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean replacement fixture command, got {outcome:?}"),
    }
}

fn target_item() -> SyndicItemId {
    SyndicItemId::from_bytes([27; 16])
}

struct ReplacementSeed {
    target_turn: beryl_model::SyndicTurnId,
    target_item: SyndicItemId,
    selected_path: SelectedPathProof,
    transcript_entry: CurrentTranscriptEntryProof,
    current: SyndicCurrentDraft,
    gate: InputGateRecord,
    asset_reference_set: SealedAssetReferenceSetProof,
}

fn target_payload() -> ComposerPayload {
    let marker = target_marker();
    ComposerPayload::new(vec![
        ComposerAtom::text("original").unwrap(),
        ComposerAtom::image_marker(marker.marker_id(), marker.label()),
    ])
    .unwrap()
}

fn replacement_seed(store: &HomeStore, storage: SyndicStorage) -> ReplacementSeed {
    support::seed_populated(store, storage);
    converge_and_release_terminal_history(store, storage, id(30), source_turn());
    let payload = target_payload();
    let prepared = PreparedContent::composer(&payload).unwrap();
    let asset_reference_set = target_asset_reference_set(
        prepared.reference(beryl_model::ContentRevision::new(1).unwrap()),
    );
    stage_prepared_content(store, storage, &prepared);
    let current = storage
        .current_draft(store, id(30), point_limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &prepared, timestamp(5)).unwrap()
    else {
        panic!("replacement fixture draft must become nonempty");
    };
    execute(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    );
    let current = storage
        .current_draft(store, id(30), point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, id(30), point_limit())
        .unwrap()
        .unwrap();
    let target_turn = draft_id(31).submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(
            storage.revision(store).unwrap(),
            IdleSubmission::new(
                id(30),
                current.thread().revision(),
                current.draft().id(),
                current.draft().revision(),
                current.draft().content(),
                gate.revision(),
                draft_id(70),
                target_item(),
                Some(asset_reference_set),
                timestamp(6),
            ),
        ),
    );
    let source = establish_turn(store, storage, id(30), target_turn, timestamp(7));
    admit_event(
        store,
        storage,
        id(30),
        target_turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(7),
    );
    correlate_user_item(
        store,
        storage,
        id(30),
        target_turn,
        target_item(),
        &source,
        timestamp(8),
    );
    admit_event(
        store,
        storage,
        id(30),
        target_turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
        ),
        timestamp(9),
    );
    converge_and_release_terminal_history(store, storage, id(30), target_turn);
    let current = storage
        .current_draft(store, id(30), point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, id(30), point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, id(30), point_limit())
        .unwrap()
        .unwrap();
    let entries = storage
        .transcript_entries(
            store,
            id(30),
            head.generation(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.item_id() == target_item())
        .unwrap_or_else(|| panic!("replacement target transcript entry disappeared"));
    ReplacementSeed {
        target_turn,
        target_item: target_item(),
        selected_path: current.thread().selected_path(),
        transcript_entry: CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
        current,
        gate,
        asset_reference_set,
    }
}

fn target_marker() -> ComposerImageMarker {
    ComposerImageMarker::new(
        SyndicDraftMarkerId::from_bytes([25; 16]),
        ImageLabelOrdinal::FIRST,
    )
}

fn start_edit(storage: SyndicStorage, store: &HomeStore, seed: &ReplacementSeed) {
    execute(
        store,
        storage.start_replacement_edit(
            storage.revision(store).unwrap(),
            StartReplacementEdit::new(
                id(30),
                seed.current.thread().revision(),
                seed.current.draft().id(),
                seed.current.draft().revision(),
                seed.gate.revision(),
                seed.target_turn,
                seed.target_item,
                seed.selected_path,
                seed.transcript_entry,
                Some(seed.asset_reference_set),
                timestamp(10),
            ),
        ),
    );
}

#[test]
fn replacement_start_and_cancel_preserve_payload_and_committed_binding() {
    let home = TestHome::new("phase5-replacement-cancel");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let seed = replacement_seed(&store, storage);

    let binding_before = storage
        .current_binding(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    start_edit(storage, &store, &seed);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    let editing = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        editing.draft().revision().get(),
        seed.current.draft().revision().get() + 1
    );
    let DraftSubmissionIntent::Replacement(intent) = editing.draft().submission_intent() else {
        panic!("replacement intent was not published");
    };
    assert_eq!(intent.target_turn_id(), seed.target_turn);
    assert_eq!(intent.selected_path(), seed.selected_path);
    assert_eq!(intent.transcript_entry(), seed.transcript_entry);
    assert_eq!(
        read_composer_payload(&store, storage, &editing).atoms()[0].text_value(),
        Some("original")
    );
    let binding_during = storage
        .current_binding(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(binding_during, binding_before);

    execute(
        &store,
        storage.cancel_replacement_edit(
            storage.revision(&store).unwrap(),
            CancelReplacementEdit::new(
                id(30),
                editing.thread().revision(),
                editing.draft().id(),
                editing.draft().revision(),
                seed.gate.revision(),
                timestamp(11),
            ),
        ),
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let cancelled = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        cancelled.draft().revision().get(),
        editing.draft().revision().get() + 1
    );
    assert_eq!(
        cancelled.draft().submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    assert_eq!(cancelled.draft().content(), editing.draft().content());

    store.close().unwrap();
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn accepted_replacement_creates_a_sibling_and_keeps_the_old_path_immutable() {
    let home = TestHome::new("phase5-replacement-submit");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let seed = replacement_seed(&store, storage);
    start_edit(storage, &store, &seed);

    let replacement_turn = seed.current.draft().id().submitted_turn_id();
    let next_draft = draft_id(71);
    let expected_content = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .content();
    execute(
        &store,
        storage.submit_idle_draft(
            storage.revision(&store).unwrap(),
            IdleSubmission::new(
                id(30),
                seed.current.thread().revision(),
                seed.current.draft().id(),
                seed.current.draft().revision().checked_next().unwrap(),
                expected_content,
                seed.gate.revision(),
                next_draft,
                SyndicItemId::from_bytes([72; 16]),
                Some(seed.asset_reference_set),
                timestamp(11),
            ),
        ),
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    let replacement = storage
        .turn(&store, replacement_turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        replacement.parent(),
        ConversationParent::Turn(source_turn())
    );
    let original = storage
        .turn(&store, seed.target_turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(original.parent(), ConversationParent::Turn(source_turn()));
    let current = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.thread().committed_tail(), Some(replacement_turn));
    assert_eq!(current.draft().id(), next_draft);
    assert_eq!(
        current.draft().submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    let gate = storage
        .input_gate(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::PendingTurn(replacement_turn));

    store.close().unwrap();
}

#[test]
fn stale_selected_path_rejects_replacement_edit_without_changing_the_draft() {
    let home = TestHome::new("phase5-replacement-stale-path");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let seed = replacement_seed(&store, storage);
    let stale = SelectedPathProof::new(
        seed.selected_path.tail(),
        seed.selected_path.thread_revision().checked_next().unwrap(),
        seed.selected_path.digest(),
    );
    let edit = StartReplacementEdit::new(
        id(30),
        seed.current.thread().revision(),
        seed.current.draft().id(),
        seed.current.draft().revision(),
        seed.gate.revision(),
        seed.target_turn,
        seed.target_item,
        stale,
        seed.transcript_entry,
        Some(seed.asset_reference_set),
        timestamp(10),
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.start_replacement_edit(storage.revision(&store).unwrap(), edit))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::NotCommitted {
            evidence: CommandError::ContributorValidation { .. }
        }
    ));

    let current = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().revision(), seed.current.draft().revision());
    assert_eq!(
        current.draft().submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    assert_eq!(
        read_composer_payload(&store, storage, &current),
        ComposerPayload::default()
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
