#![cfg(feature = "test-faults")]

mod support;

use beryl_model::{SyndicDraftId, SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::*;

use support::exact_cas::{admit_event, correlate_user_item, establish_turn, submit_current_draft};
use support::populated::source_turn;
use support::semantic::exercise_seeded_populated_case;
use support::*;

fn replacement_mutation(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    target: SyndicTurnId,
    selected: SelectedPathProof,
    entry: CurrentTranscriptEntryProof,
) -> FixtureBatch {
    let current = storage
        .current_draft(store, thread, SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap();
    let summary = storage
        .history_summary(store, thread, SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap();
    let revision = current.draft().revision().checked_next().unwrap();
    batch([
        FixtureRecord::Draft(DraftRecord::new(
            current.draft().id(),
            current.thread().id(),
            revision,
            DraftSubmissionIntent::Replacement(ReplacementEditIntent::new(target, selected, entry)),
            current.draft().content(),
            current.draft().created_at(),
            summary.last_activity_at(),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            current.thread().id(),
            current.draft().id(),
            revision,
            current.thread().revision(),
        )),
    ])
}

fn seed_empty_replacement_thread(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
) {
    let mut command = beryl_home_store::HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ))
        .unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean empty replacement seed, got {outcome:?}"),
    }
}

fn seed_real_replacement_target(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
) -> (SyndicTurnId, SelectedPathProof, CurrentTranscriptEntryProof) {
    converge_and_release_terminal_history(store, storage, id(30), source_turn());
    let item = SyndicItemId::from_bytes([27; 16]);
    let turn = submit_current_draft(
        store,
        storage,
        id(30),
        draft_id(70),
        item,
        "replacement target",
        timestamp(10),
    );
    let source = establish_turn(store, storage, id(30), turn, timestamp(11));
    admit_event(
        store,
        storage,
        id(30),
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(11),
    );
    correlate_user_item(store, storage, id(30), turn, item, &source, timestamp(12));
    admit_event(
        store,
        storage,
        id(30),
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
        ),
        timestamp(13),
    );
    converge_and_release_terminal_history(store, storage, id(30), turn);
    let current = storage
        .current_draft(store, id(30), SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, id(30), SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap();
    let entries = storage
        .transcript_entries(
            store,
            id(30),
            head.generation(),
            None,
            beryl_home_store::CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.item_id() == item)
        .unwrap();
    (
        turn,
        current.thread().selected_path(),
        CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
    )
}

#[test]
fn replacement_intent_roundtrips_with_exact_selected_path_proof() {
    let home = TestHome::new("replacement-roundtrip");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    let (turn, selected, entry) = seed_real_replacement_target(&store, storage);
    let current = storage
        .current_draft(
            &store,
            id(30),
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap();
    let current_draft = current.draft().id();
    commit(
        &store,
        storage,
        replacement_mutation(&store, storage, id(30), turn, selected, entry),
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let draft = storage
        .draft(
            &store,
            current_draft,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let DraftSubmissionIntent::Replacement(intent) = draft.submission_intent() else {
        panic!("replacement intent did not roundtrip");
    };
    assert_eq!(intent.target_turn_id(), turn);
    assert_eq!(intent.selected_path(), selected);
    assert_eq!(intent.transcript_entry(), entry);
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn replacement_intent_rejects_stale_proof_and_wrong_entry_target() {
    exercise_seeded_populated_case(
        "replacement-selected-proof",
        "replacement edit selected-path proof disagrees with current thread",
        |store, storage| {
            let (turn, _selected, entry) = seed_real_replacement_target(store, storage);
            let current = storage
                .current_draft(store, id(30), SyndicPointReadLimit::new(1_000_000).unwrap())
                .unwrap()
                .unwrap();
            let selected = SelectedPathProof::new(
                None,
                current.thread().revision(),
                empty_selected_path_digest(),
            );
            replacement_mutation(store, storage, id(30), turn, selected, entry)
        },
    );
    exercise_seeded_populated_case(
        "replacement-off-path",
        "replacement edit transcript entry or user item disagrees",
        |store, storage| {
            let (turn, selected, entry) = seed_real_replacement_target(store, storage);
            replacement_mutation(
                store,
                storage,
                id(30),
                turn,
                selected,
                CurrentTranscriptEntryProof::new(entry.generation(), TranscriptPosition::FIRST),
            )
        },
    );
    exercise_seeded_populated_case(
        "replacement-empty-path",
        "replacement edit target requires a selected path",
        |store, storage| {
            let thread = id(80);
            let target = SyndicTurnId::from_bytes([82; 16]);
            seed_empty_replacement_thread(store, storage, thread, draft_id(81));
            let current = storage
                .current_draft(store, thread, SyndicPointReadLimit::new(1_000_000).unwrap())
                .unwrap()
                .unwrap();
            let mut corrupt = replacement_mutation(
                store,
                storage,
                thread,
                target,
                SelectedPathProof::new(
                    None,
                    current.thread().revision(),
                    empty_selected_path_digest(),
                ),
                CurrentTranscriptEntryProof::new(
                    TranscriptGeneration::FIRST,
                    TranscriptPosition::FIRST,
                ),
            );
            corrupt
                .put(FixtureRecord::Turn(TurnRecord::new(
                    target,
                    thread,
                    TurnKind::OrdinaryUser,
                    ConversationParent::Root,
                    None,
                    TurnDepth::FIRST,
                    root_turn_chain_digest(target),
                    timestamp(2),
                )))
                .unwrap();
            corrupt
                .put(FixtureRecord::TurnState(fixture_turn_state(
                    target,
                    TurnStateRevision::FIRST,
                    TurnLifecycle::Interrupted,
                    0,
                    0,
                    timestamp(2),
                )))
                .unwrap();
            corrupt
        },
    );
}

#[test]
fn replacement_intent_rejects_provider_operation_target() {
    let target = SyndicTurnId::from_bytes([52; 16]);
    let digest = root_turn_chain_digest(target);
    exercise_seeded_populated_case(
        "replacement-provider-operation",
        "replacement edit target is not an ordinary user turn",
        |store, storage| {
            let (_turn, selected, entry) = seed_real_replacement_target(store, storage);
            let mut corrupt = replacement_mutation(store, storage, id(30), target, selected, entry);
            corrupt
                .put(FixtureRecord::Turn(TurnRecord::new(
                    target,
                    id(30),
                    TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction),
                    ConversationParent::Root,
                    None,
                    TurnDepth::FIRST,
                    digest,
                    timestamp(14),
                )))
                .unwrap();
            corrupt
                .put(FixtureRecord::TurnState(fixture_turn_state(
                    target,
                    TurnStateRevision::FIRST,
                    TurnLifecycle::Interrupted,
                    0,
                    0,
                    timestamp(14),
                )))
                .unwrap();
            corrupt
        },
    );
}

#[test]
fn replacement_validation_uses_one_exact_current_entry_instead_of_ancestry_walk() {
    exercise_seeded_populated_case(
        "replacement-wrong-current-entry",
        "replacement edit transcript entry or user item disagrees",
        |store, storage| {
            let (turn, selected, entry) = seed_real_replacement_target(store, storage);
            replacement_mutation(
                store,
                storage,
                id(30),
                turn,
                selected,
                CurrentTranscriptEntryProof::new(entry.generation(), TranscriptPosition::FIRST),
            )
        },
    );
}
