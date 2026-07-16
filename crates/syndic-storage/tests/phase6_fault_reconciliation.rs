#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{
    CommandError, CursorReadLimits, HomeCommand, HomeHealthState, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{CasItemId, CasTurnId, SyndicItemId};
use syndic_storage::*;

use support::populated::{active_turn, cas_thread, cas_turn, populated_records};
use support::{TestHome, batch, commit, id, open, timestamp};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> Result<(), CommandError> {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).map(|_| ())
}

fn typed_error(error: &CommandError) -> &SyndicMutationError {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected Syndic validation rejection, got {error}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn exact_source() -> CasTurnSource {
    CasTurnSource::new(cas_thread(), cas_turn())
}

fn source_event(
    store: &HomeStore,
    storage: SyndicStorage,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) -> LiveSourceEvent {
    let turn = active_turn();
    let state = storage.turn_state(store, turn, limit()).unwrap().unwrap();
    let gate = storage.input_gate(store, id(40), limit()).unwrap().unwrap();
    LiveSourceEvent::new(
        id(40),
        turn,
        state.record().revision(),
        gate.record().revision(),
        SourceEventSequence::new(state.record().source_event_count() + 1).unwrap(),
        Some(exact_source()),
        payload,
        observed_at,
    )
    .unwrap()
}

fn start_item(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId, cas_item: &CasItemId) {
    let descriptor = assistant_descriptor(item, cas_item.clone());
    let event = source_event(
        store,
        storage,
        SourceEventPayload::ItemStarted {
            item: descriptor,
            assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
        },
        timestamp(9),
    );
    execute(
        store,
        storage.admit_live_source_event(storage.revision(store).unwrap(), event),
    )
    .unwrap();
}

fn assistant_descriptor(item: SyndicItemId, cas_item: CasItemId) -> SourceItemDescriptor {
    SourceItemDescriptor::new(
        item,
        cas_item,
        ProviderItemKind::AgentMessage,
        ProviderItemDisposition::CanonicalText,
    )
    .unwrap()
}

fn item_text(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId) -> String {
    let item = storage
        .canonical_item(store, item, limit())
        .unwrap()
        .unwrap();
    let mut after = None;
    let mut bytes = Vec::new();
    loop {
        let page = storage
            .content_chunks(
                store,
                item.record()
                    .payload()
                    .content()
                    .expect("assistant item has canonical content")
                    .id(),
                after,
                CursorReadLimits::new(8, 1_000_000).unwrap(),
            )
            .unwrap();
        for chunk in page.records() {
            bytes.extend_from_slice(chunk.bytes());
            after = Some(chunk.ordinal());
        }
        if !page.has_more() {
            break;
        }
    }
    String::from_utf8(bytes).unwrap()
}

#[test]
fn live_items_require_the_exact_active_cas_turn_and_item_identity() {
    let home = TestHome::new("phase6-external-identity");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    let item = SyndicItemId::from_bytes([70; 16]);
    let cas_item = CasItemId::new("phase6-exact-item").unwrap();
    let state = storage
        .turn_state(&store, active_turn(), limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, id(40), limit())
        .unwrap()
        .unwrap();
    let mismatched = LiveSourceEvent::new(
        id(40),
        active_turn(),
        state.record().revision(),
        gate.record().revision(),
        SourceEventSequence::new(6).unwrap(),
        Some(CasTurnSource::new(
            cas_thread(),
            CasTurnId::new("different-turn").unwrap(),
        )),
        SourceEventPayload::ItemStarted {
            item: assistant_descriptor(item, cas_item.clone()),
            assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
        },
        timestamp(9),
    )
    .unwrap();
    let error = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), mismatched),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceIdentityConflict
    ));

    start_item(&store, storage, item, &cas_item);
    let wrong_item = source_event(
        &store,
        storage,
        SourceEventPayload::ItemDelta {
            item_id: item,
            cas_item_id: CasItemId::new("different-item").unwrap(),
            expected_kind: ProviderItemKind::AgentMessage,
            text: SourceEventText::new("wrong").unwrap(),
        },
        timestamp(10),
    );
    let error = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), wrong_item),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceIdentityConflict
    ));

    let delta = source_event(
        &store,
        storage,
        SourceEventPayload::ItemDelta {
            item_id: item,
            cas_item_id: cas_item.clone(),
            expected_kind: ProviderItemKind::AgentMessage,
            text: SourceEventText::new("exact").unwrap(),
        },
        timestamp(10),
    );
    execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), delta),
    )
    .unwrap();
    let complete = source_event(
        &store,
        storage,
        SourceEventPayload::ItemCompleted {
            item: assistant_descriptor(item, cas_item.clone()),
            assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
        },
        timestamp(11),
    );
    execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), complete),
    )
    .unwrap();

    let record = storage
        .canonical_item(&store, item, limit())
        .unwrap()
        .unwrap();
    let source = record.record().cas_source().unwrap();
    assert_eq!(source.turn(), &exact_source());
    assert_eq!(source.item_id(), &cas_item);
    assert_eq!(record.record().source_event_count(), 3);
    assert_eq!(item_text(&store, storage, item), "exact");
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn delta_persistence_cuts_reconcile_to_wholly_old_or_wholly_new_history() {
    for (name, point, expected_events, expected_item_events, expected_text) in [
        (
            "phase6-delta-before-commit",
            FaultPoint::BeforeCommit,
            6,
            1,
            "",
        ),
        (
            "phase6-delta-after-commit-before-persist",
            FaultPoint::AfterCommitBeforePersist,
            7,
            2,
            "atomic delta",
        ),
        (
            "phase6-delta-after-persist",
            FaultPoint::AfterPersist,
            7,
            2,
            "atomic delta",
        ),
    ] {
        let home = TestHome::new(name);
        let faults = FaultController::new();
        let mut store = open_with_faults(home.path(), faults.clone());
        let storage = SyndicStorage::register(&mut store).unwrap();
        commit(&store, storage, batch(populated_records()));
        let item = SyndicItemId::from_bytes([71; 16]);
        let cas_item = CasItemId::new("phase6-fault-item").unwrap();
        start_item(&store, storage, item, &cas_item);
        let delta = source_event(
            &store,
            storage,
            SourceEventPayload::ItemDelta {
                item_id: item,
                cas_item_id: cas_item,
                expected_kind: ProviderItemKind::AgentMessage,
                text: SourceEventText::new("atomic delta").unwrap(),
            },
            timestamp(10),
        );
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.admit_live_source_event(storage.revision(&store).unwrap(), delta.clone()))
            .unwrap();

        faults.fail_next(point);
        assert!(store.execute(command).is_err());
        assert_eq!(store.health().state(), HomeHealthState::Verifying);
        store.verify_health().unwrap();
        assert_eq!(
            storage
                .turn_state(&store, active_turn(), limit())
                .unwrap()
                .unwrap()
                .record()
                .source_event_count(),
            expected_events
        );
        assert_eq!(
            storage
                .canonical_item(&store, item, limit())
                .unwrap()
                .unwrap()
                .record()
                .source_event_count(),
            expected_item_events
        );
        assert_eq!(item_text(&store, storage, item), expected_text);
        store.validate_registered_domains().unwrap();
        store.close().unwrap();

        let mut reopened = open(home.path());
        let storage = SyndicStorage::register(&mut reopened).unwrap();
        reopened.validate_registered_domains().unwrap();
        assert_eq!(item_text(&reopened, storage, item), expected_text);
        let retry = execute(
            &reopened,
            storage.admit_live_source_event(storage.revision(&reopened).unwrap(), delta),
        );
        if expected_events == 6 {
            retry.unwrap();
        } else {
            assert!(matches!(
                typed_error(&retry.unwrap_err()),
                SyndicMutationError::SourceEventAlreadyAdmitted
            ));
        }
        reopened.validate_registered_domains().unwrap();
        assert_eq!(item_text(&reopened, storage, item), "atomic delta");
        assert_eq!(
            storage
                .turn_state(&reopened, active_turn(), limit())
                .unwrap()
                .unwrap()
                .record()
                .source_event_count(),
            7
        );
        reopened.close().unwrap();
    }
}
