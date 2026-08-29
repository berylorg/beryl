#![allow(dead_code)]

use beryl_home_store::{CurrentDomainCommand, HomeStore};
use beryl_model::{
    AcceptedInputRevision, DraftRevision, InputGateRevision, SyndicAcceptedInputId, SyndicDraftId,
    SyndicThreadId, ThreadRevision,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use crate::accepted_fixtures::{delivering_input, seed_mixed_abandonment};
use crate::support::{
    TestHome, batch, commit, id, open,
    populated::{
        active_snapshot, active_turn, cas_thread, cas_turn, next_input, seed_populated,
        steering_input,
    },
};

pub fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub fn seeded(name: &str, records: Vec<FixtureRecord>) -> (TestHome, HomeStore, SyndicStorage) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage.clone(), batch(records));
    (home, store, storage)
}

pub fn seeded_populated(name: &str) -> (TestHome, HomeStore, SyndicStorage) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage.clone());
    (home, store, storage)
}

pub fn seeded_mixed(name: &str) -> (TestHome, HomeStore, SyndicStorage) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_mixed_abandonment(&store, storage.clone());
    (home, store, storage)
}

pub fn seeded_large_ready(name: &str, last_ordinal: u64) -> (TestHome, HomeStore, SyndicStorage) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_large_ready_generation(&store, &storage, last_ordinal);
    (home, store, storage)
}

pub fn seed_operation(store: &HomeStore, storage: &SyndicStorage, operation: AcceptedOperation) {
    match operation {
        AcceptedOperation::Begin => seed_populated(store, storage.clone()),
        AcceptedOperation::Retry | AcceptedOperation::Complete | AcceptedOperation::Reject => {
            seed_mixed_abandonment(store, storage.clone());
        }
    }
}

pub fn seeded_operation(
    name: &str,
    operation: AcceptedOperation,
) -> (TestHome, HomeStore, SyndicStorage) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_operation(&store, &storage, operation);
    (home, store, storage)
}

pub fn route_entry(
    store: &HomeStore,
    storage: &SyndicStorage,
    input: SyndicAcceptedInputId,
) -> (InputGateRecord, AcceptedRouteEntry) {
    let gate = storage.input_gate(store, id(40), limit()).unwrap().unwrap();
    let route = gate
        .selected_route()
        .expect("active fixture selects a route");
    let page = storage
        .accepted_route_page(store, id(40), route.generation(), route.revision(), None)
        .unwrap();
    let entry = page
        .records()
        .iter()
        .find(|entry| entry.input().id() == input)
        .unwrap()
        .clone();
    (gate, entry)
}

#[derive(Clone, Copy, Debug)]
pub enum AcceptedOperation {
    Begin,
    Retry,
    Complete,
    Reject,
}

impl AcceptedOperation {
    pub const ALL: [Self; 4] = [Self::Begin, Self::Retry, Self::Complete, Self::Reject];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Begin => "begin",
            Self::Retry => "retry",
            Self::Complete => "complete",
            Self::Reject => "rejection",
        }
    }

    pub fn input(self) -> SyndicAcceptedInputId {
        match self {
            Self::Begin => steering_input(),
            Self::Retry | Self::Complete | Self::Reject => delivering_input(),
        }
    }

    pub fn thread(self) -> SyndicThreadId {
        id(40)
    }

    pub fn expected_route(self) -> AcceptedRouteHeadProof {
        let revision = match self {
            Self::Begin => AcceptedRouteRevision::new(2).unwrap(),
            Self::Retry | Self::Complete | Self::Reject => AcceptedRouteRevision::new(3).unwrap(),
        };
        AcceptedRouteHeadProof::new(AcceptedRouteGeneration::FIRST, revision)
    }

    pub fn expected_gate_revision(self) -> InputGateRevision {
        InputGateRevision::new(match self {
            Self::Begin => 4,
            Self::Retry | Self::Complete | Self::Reject => 6,
        })
        .unwrap()
    }

    pub fn expected_input_revision(self) -> AcceptedInputRevision {
        AcceptedInputRevision::new(match self {
            Self::Begin => 1,
            Self::Retry | Self::Complete | Self::Reject => 2,
        })
        .unwrap()
    }

    pub fn target(self) -> SteeringTargetProof {
        SteeringTargetProof::new(
            PendingSteeringTargetProof::new(
                beryl_model::BindingRevision::new(3).unwrap(),
                active_snapshot(),
                active_turn(),
                cas_thread(),
            ),
            cas_turn(),
        )
    }

    pub fn current_command(self, storage: &SyndicStorage) -> CurrentDomainCommand {
        match self {
            Self::Begin => storage.current_begin_accepted_input_delivery(self.begin_request()),
            Self::Retry => storage.current_retry_accepted_input_delivery(self.retry_request()),
            Self::Complete => {
                storage.current_complete_accepted_input_delivery(self.complete_request())
            }
            Self::Reject => storage.current_record_steering_rejection(self.rejection_request()),
        }
    }

    pub fn status(
        self,
        store: &HomeStore,
        storage: &SyndicStorage,
    ) -> AcceptedInputDeliveryTransitionStatus {
        match self {
            Self::Begin => {
                storage.begin_accepted_input_delivery_status(store, &self.begin_request(), limit())
            }
            Self::Retry => {
                storage.retry_accepted_input_delivery_status(store, &self.retry_request(), limit())
            }
            Self::Complete => storage.complete_accepted_input_delivery_status(
                store,
                &self.complete_request(),
                limit(),
            ),
            Self::Reject => {
                storage.steering_rejection_status(store, &self.rejection_request(), limit())
            }
        }
        .unwrap()
    }

    pub fn expected_leaf(
        self,
    ) -> (
        AcceptedRouteLeafState,
        AcceptedInputLifecycle,
        AcceptedRouteEffectiveState,
    ) {
        match self {
            Self::Begin => (
                AcceptedRouteLeafState::Routed,
                AcceptedInputLifecycle::Delivering,
                AcceptedRouteEffectiveState::Delivering,
            ),
            Self::Retry => (
                AcceptedRouteLeafState::Routed,
                AcceptedInputLifecycle::Retryable,
                AcceptedRouteEffectiveState::Ready,
            ),
            Self::Complete => (
                AcceptedRouteLeafState::Routed,
                AcceptedInputLifecycle::Delivered,
                AcceptedRouteEffectiveState::Delivered,
            ),
            Self::Reject => (
                AcceptedRouteLeafState::NextTurn(NextTurnReason::SteeringRejected),
                AcceptedInputLifecycle::Retryable,
                AcceptedRouteEffectiveState::NextTurn(NextTurnReason::SteeringRejected),
            ),
        }
    }

    pub const fn expected_transition_kind(self) -> AcceptedRouteLeafTransitionKind {
        match self {
            Self::Begin => AcceptedRouteLeafTransitionKind::Begin,
            Self::Retry => AcceptedRouteLeafTransitionKind::Retry,
            Self::Complete => AcceptedRouteLeafTransitionKind::Complete,
            Self::Reject => AcceptedRouteLeafTransitionKind::SteeringRejected,
        }
    }

    fn begin_request(self) -> BeginAcceptedInputDelivery {
        BeginAcceptedInputDelivery::new(
            self.thread(),
            self.input(),
            self.expected_input_revision(),
            self.target(),
        )
    }

    fn retry_request(self) -> RetryAcceptedInputDelivery {
        RetryAcceptedInputDelivery::new(
            self.thread(),
            self.input(),
            self.expected_input_revision(),
            self.target(),
        )
    }

    fn complete_request(self) -> CompleteAcceptedInputDelivery {
        CompleteAcceptedInputDelivery::new(
            self.thread(),
            self.input(),
            self.expected_input_revision(),
            self.target(),
        )
    }

    fn rejection_request(self) -> SteeringRejection {
        SteeringRejection::new(
            self.thread(),
            self.input(),
            self.expected_input_revision(),
            self.target(),
        )
    }
}

pub fn assert_operation_committed(
    store: &HomeStore,
    storage: &SyndicStorage,
    operation: AcceptedOperation,
) {
    let (gate, entry) = route_entry(store, storage, operation.input());
    let (state, lifecycle, effective) = operation.expected_leaf();
    assert_eq!(
        entry.leaf().revision(),
        operation.expected_input_revision().checked_next().unwrap()
    );
    assert_eq!(entry.leaf().state(), state);
    assert_eq!(entry.leaf().lifecycle(), lifecycle);
    assert_eq!(entry.effective_state(), effective);
    assert_eq!(
        gate.revision(),
        operation.expected_gate_revision().checked_next().unwrap()
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

pub fn seed_large_ready_generation(store: &HomeStore, storage: &SyndicStorage, last_ordinal: u64) {
    assert!(last_ordinal > 256);
    seed_populated(store, storage.clone());
    let limit = limit();
    let thread = id(40);
    let generation = AcceptedRouteGeneration::FIRST;
    let thread_record = storage.thread(store, thread, limit).unwrap().unwrap();
    let draft = storage
        .current_draft(store, thread, limit)
        .unwrap()
        .unwrap();
    let summary = storage
        .history_summary(store, thread, limit)
        .unwrap()
        .unwrap();
    let next = storage
        .accepted_input(store, next_input(), limit)
        .unwrap()
        .unwrap();
    let gate = storage.input_gate(store, thread, limit).unwrap().unwrap();
    let route = syndic_storage::test_faults::accepted_route_generation(
        store,
        storage.clone(),
        thread,
        generation,
    )
    .unwrap();
    let final_thread_revision = ThreadRevision::new(last_ordinal + 1).unwrap();
    let final_gate_revision = InputGateRevision::new(last_ordinal + 1).unwrap();
    let empty_content = next.content();
    let mut records = vec![
        FixtureRecord::Thread(ThreadRecord::new(
            thread,
            SelectedPathProof::new(
                thread_record.committed_tail(),
                final_thread_revision,
                thread_record.selected_path_digest(),
            ),
            thread_record.current_draft_id(),
            thread_record.lineage(),
            thread_record.context_owner_id(),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            draft.draft().id(),
            draft.draft().revision(),
            final_thread_revision,
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            summary.revision().checked_next().unwrap(),
            final_thread_revision,
            summary.committed_tail(),
            summary.selected_path_digest(),
            summary.complete(),
            summary.last_activity_at(),
        )),
        FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                next.id(),
                thread,
                next.ordinal(),
                AcceptedInputAdmissionProof::new(
                    next.admission().expected_thread_revision(),
                    next.admission().source_draft_id(),
                    next.admission().expected_draft_revision(),
                    next.admission().expected_gate_revision(),
                    SyndicDraftId::from_bytes(*accepted_id(3).as_bytes()),
                )
                .unwrap(),
                generation,
                next.content(),
                next.asset_reference_set(),
                next.admitted_at(),
            )
            .unwrap(),
        ),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                final_gate_revision,
                gate.state().clone(),
                last_ordinal,
                Some(generation),
                gate.selected_route(),
                last_ordinal - 1,
                1,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
                thread,
                generation,
                route.revision(),
                route.target().clone(),
                Some(AcceptedInputOrdinal::FIRST),
                Some(AcceptedInputOrdinal::new(last_ordinal).unwrap()),
                last_ordinal,
                last_ordinal - 1,
                0,
                1,
                0,
                0,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedReadySource(AcceptedReadySourceRecord::new(
            thread,
            final_gate_revision,
            generation,
            route.revision(),
            AcceptedInputOrdinal::FIRST,
            AcceptedInputOrdinal::new(last_ordinal).unwrap(),
        )),
        FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
            thread,
            generation,
            route.revision(),
            AcceptedInputOrdinal::FIRST,
            AcceptedInputOrdinal::new(last_ordinal).unwrap(),
        )),
    ];

    for value in 3..=last_ordinal {
        let ordinal = AcceptedInputOrdinal::new(value).unwrap();
        let input_id = accepted_id(value);
        records.extend([
            FixtureRecord::AcceptedInput(
                AcceptedInputRecord::new(
                    input_id,
                    thread,
                    ordinal,
                    AcceptedInputAdmissionProof::new(
                        ThreadRevision::new(value).unwrap(),
                        SyndicDraftId::from_bytes(*input_id.as_bytes()),
                        DraftRevision::new(1).unwrap(),
                        InputGateRevision::new(value).unwrap(),
                        if value == last_ordinal {
                            crate::support::draft_id(41)
                        } else {
                            SyndicDraftId::from_bytes(*accepted_id(value + 1).as_bytes())
                        },
                    )
                    .unwrap(),
                    generation,
                    empty_content,
                    None,
                    crate::support::timestamp(8),
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
                thread, ordinal, input_id, generation,
            )),
            FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
                input_id,
                thread,
                generation,
                ordinal,
                AcceptedInputRevision::new(1).unwrap(),
                AcceptedRouteLeafState::Routed,
                AcceptedInputLifecycle::Admitted,
            )),
        ]);
    }
    let additions = records.split_off(8);
    for chunk in additions.chunks(96) {
        commit(store, storage.clone(), batch(chunk.iter().cloned()));
    }
    commit(store, storage.clone(), batch(records));
}

fn accepted_id(ordinal: u64) -> SyndicAcceptedInputId {
    let mut bytes = [0x53; 16];
    bytes[..8].copy_from_slice(&ordinal.to_be_bytes());
    SyndicAcceptedInputId::from_bytes(bytes)
}
