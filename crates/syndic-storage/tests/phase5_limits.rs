#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::{
    AcceptedInputRevision, DraftRevision, InputGateRevision, SyndicAcceptedInputId, SyndicDraftId,
    ThreadRevision,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use support::populated::populated_records;
use support::*;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute_one(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn accepted_id(ordinal: u64) -> SyndicAcceptedInputId {
    let mut bytes = [0xA5; 16];
    bytes[..8].copy_from_slice(&ordinal.to_be_bytes());
    SyndicAcceptedInputId::from_bytes(bytes)
}

fn live_capacity_records(live_count: u32) -> Vec<FixtureRecord> {
    let thread = id(40);
    let mut records = populated_records();
    let state = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::InputGate(gate) if gate.thread_id() == thread => {
                Some(gate.state().clone())
            }
            _ => None,
        })
        .unwrap();
    records.retain(
        |record| !matches!(record, FixtureRecord::InputGate(gate) if gate.thread_id() == thread),
    );
    records.push(FixtureRecord::InputGate(
        InputGateRecord::new(
            thread,
            InputGateRevision::new(3).unwrap(),
            state,
            u64::from(live_count),
            1,
            live_count - 1,
            0,
        )
        .unwrap(),
    ));
    let content = empty_composer_content();
    for value in 3..=u64::from(live_count) {
        let ordinal = AcceptedInputOrdinal::new(value).unwrap();
        let input_id = accepted_id(value);
        let revision = AcceptedInputRevision::new(1).unwrap();
        records.extend([
            FixtureRecord::AcceptedInput(AcceptedInputRecord::new(
                input_id,
                thread,
                revision,
                ordinal,
                InputGateRevision::new(3).unwrap(),
                AcceptedInputDisposition::NextTurn(NextTurnReason::WorkerCapacity),
                AcceptedInputLifecycle::Admitted,
                content,
                0,
                timestamp(8),
            )),
            FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
                thread, ordinal, input_id, revision,
            )),
            FixtureRecord::AcceptedNextTurn(AcceptedNextTurnIndexRecord::new(
                thread, ordinal, input_id, revision,
            )),
        ]);
    }
    records
}

#[test]
fn the_two_live_limits_accept_the_boundary_and_reject_the_next_unit() {
    let thread = id(1);
    let turn = draft_id(2).submitted_turn_id();
    assert!(
        InputGateRecord::new(
            thread,
            InputGateRevision::new(1).unwrap(),
            InputGateState::PendingTurn(turn),
            u64::from(MAX_LIVE_ACCEPTED_INPUTS),
            MAX_LIVE_ACCEPTED_INPUTS,
            0,
            MAX_LIVE_ACCEPTED_UTF8_BYTES,
        )
        .is_ok()
    );
    assert!(matches!(
        InputGateRecord::new(
            thread,
            InputGateRevision::new(1).unwrap(),
            InputGateState::PendingTurn(turn),
            u64::from(MAX_LIVE_ACCEPTED_INPUTS) + 1,
            MAX_LIVE_ACCEPTED_INPUTS,
            1,
            0,
        ),
        Err(SyndicRecordError::LiveAcceptedInputCountTooLarge { .. })
    ));
    assert!(matches!(
        InputGateRecord::new(
            thread,
            InputGateRevision::new(1).unwrap(),
            InputGateState::PendingTurn(turn),
            u64::from(MAX_LIVE_ACCEPTED_INPUTS),
            MAX_LIVE_ACCEPTED_INPUTS,
            0,
            MAX_LIVE_ACCEPTED_UTF8_BYTES + 1,
        ),
        Err(SyndicRecordError::LiveAcceptedInputBytesTooLarge { .. })
    ));
}

#[test]
fn a_two_hundred_fifty_sixth_live_fragment_is_admitted() {
    let home = TestHome::new("phase5-live-fragment-boundary");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(
        &store,
        storage,
        batch(live_capacity_records(MAX_LIVE_ACCEPTED_INPUTS - 1)),
    );
    let thread = id(40);
    let draft = draft_id(41);
    let payload = ComposerPayload::new(vec![ComposerAtom::text("last slot").unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(&store, storage, &content);
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(9)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute_one(
        &store,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    );
    let expected_content = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .content();
    let admission = AcceptedInputAdmission::new(
        thread,
        ThreadRevision::new(1).unwrap(),
        draft,
        DraftRevision::new(2).unwrap(),
        expected_content,
        InputGateRevision::new(3).unwrap(),
        SyndicDraftId::from_bytes([79; 16]),
        AdmissionMarkers::default(),
        timestamp(10),
    );
    execute_one(
        &store,
        storage.admit_accepted_input(storage.revision(&store).unwrap(), admission.clone()),
    );
    assert_eq!(
        storage
            .accepted_input_status(&store, &admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::ExactAccepted
    );
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().live_count(), MAX_LIVE_ACCEPTED_INPUTS);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn a_two_hundred_fifty_seventh_live_fragment_is_rejected_without_consuming_the_draft() {
    let home = TestHome::new("phase5-live-fragment-limit");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(
        &store,
        storage,
        batch(live_capacity_records(MAX_LIVE_ACCEPTED_INPUTS)),
    );
    let thread = id(40);
    let draft = draft_id(41);
    let payload = ComposerPayload::new(vec![ComposerAtom::text("overflow").unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(&store, storage, &content);
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(9)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute_one(
        &store,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    );
    let expected_content = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .content();
    let admission = AcceptedInputAdmission::new(
        thread,
        ThreadRevision::new(1).unwrap(),
        draft,
        DraftRevision::new(2).unwrap(),
        expected_content,
        InputGateRevision::new(3).unwrap(),
        SyndicDraftId::from_bytes([80; 16]),
        AdmissionMarkers::default(),
        timestamp(10),
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.admit_accepted_input(storage.revision(&store).unwrap(), admission.clone()))
        .unwrap();
    assert!(store.execute(command).is_err());
    assert_eq!(
        storage
            .accepted_input_status(&store, &admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::Absent
    );
    assert_eq!(
        storage
            .current_draft(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .draft()
            .id(),
        draft
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
