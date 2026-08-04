use std::{sync::Arc, time::Duration};

use beryl_app::{
    LifecycleYieldOutcome,
    cas_projection::{ContextCompactionLifecycleTestHarness, ProjectionConnectionService},
};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasTurnId,
    SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    ClaimCompactionDispatch, CompactionAdmissionRead, CompactionAttemptNonce,
    CompactionMarkerLifecycle, CompactionOperationId, CompactionOperationNonce,
    CompactionOperationRecord, CompactionProviderEvent, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp,
};

pub struct LifecycleFixture {
    directory: tempfile::TempDir,
    pub storage: SyndicStorage,
    pub service: Arc<ProjectionConnectionService>,
    pub harness: ContextCompactionLifecycleTestHarness,
    pub thread_id: SyndicThreadId,
    pub yielding_turn_id: SyndicTurnId,
    pub operation_id: CompactionOperationId,
}

impl LifecycleFixture {
    pub fn new(seed: u8, operation_byte: u8) -> Self {
        Self::new_with_accepted_next(seed, operation_byte, false)
    }

    pub fn with_accepted_next(seed: u8, operation_byte: u8) -> Self {
        Self::new_with_accepted_next(seed, operation_byte, true)
    }

    fn new_with_accepted_next(seed: u8, operation_byte: u8, accepted_next: bool) -> Self {
        let mut source = crate::syndic::Fixture::new(seed);
        let submitted = source.submit_text("phase72 completed yielding turn");
        source.complete_with_assistant(submitted, "phase72 completed answer");
        let thread_id = source.thread;
        let yielding_turn_id = submitted.turn;
        assert!(
            source
                .store
                .record_lifecycle_yield_outcome(
                    thread_id,
                    yielding_turn_id,
                    LifecycleYieldOutcome::PhaseContinue,
                )
                .unwrap()
        );

        let candidate = match source
            .storage
            .compaction_admission_read(&source.store, thread_id, point_limit())
            .unwrap()
        {
            CompactionAdmissionRead::Admissible(candidate) => candidate,
            other => panic!("fixture compaction was not admissible: {other:?}"),
        };
        let attempt = CompactionAttemptNonce::from_bytes([operation_byte.wrapping_add(1); 16]);
        let admission = candidate.admission(
            CompactionOperationNonce::from_bytes([operation_byte; 16]),
            attempt,
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(u64::from(operation_byte) + 1).unwrap(),
                CasLoadedThreadGeneration::new(u64::from(operation_byte) + 2).unwrap(),
            ),
            SyndicTimestamp::from_unix_millis(72_000),
        );
        let operation_id = admission.operation_id();
        source
            .store
            .execute_current(source.storage.current_admit_compaction_operation(admission))
            .unwrap();
        let admitted = operation(&source.store, source.storage, operation_id);
        source
            .store
            .execute_current(source.storage.current_claim_compaction_dispatch(
                ClaimCompactionDispatch::new(operation_id, admitted.revision(), attempt),
            ))
            .unwrap();
        let harness = source
            .store
            .context_compaction_lifecycle_test_harness()
            .unwrap();
        harness
            .mount_lifecycle_operation(
                operation_id,
                attempt,
                yielding_turn_id,
                Duration::from_secs(30),
            )
            .unwrap();
        if accepted_next {
            let _ = source.accept_text("phase72 accepted while compacting");
        }
        let storage = source.storage;
        let (directory, service) = source.into_service();
        Self {
            directory,
            storage,
            service: Arc::new(service),
            harness,
            thread_id,
            yielding_turn_id,
            operation_id,
        }
    }

    pub fn publish_success_prefix(&self) {
        let events = [
            CompactionProviderEvent::ThreadStatus(syndic_storage::CompactionThreadStatus::Active),
            CompactionProviderEvent::TurnStarted(CasTurnId::new("phase72-compaction").unwrap()),
            CompactionProviderEvent::Marker {
                item_id: SyndicItemId::from_bytes([244; 16]),
                lifecycle: CompactionMarkerLifecycle::Started,
            },
            CompactionProviderEvent::Marker {
                item_id: SyndicItemId::from_bytes([244; 16]),
                lifecycle: CompactionMarkerLifecycle::Completed,
            },
            CompactionProviderEvent::ThreadStatus(syndic_storage::CompactionThreadStatus::Idle),
        ];
        for (index, event) in events.into_iter().enumerate() {
            self.harness
                .publish_provider_event(
                    self.operation_id,
                    event,
                    SyndicTimestamp::from_unix_millis(72_010 + index as u64),
                )
                .unwrap();
        }
    }

    pub fn publish_success_terminal(&self) {
        self.harness
            .publish_provider_event(
                self.operation_id,
                CompactionProviderEvent::Terminal(syndic_storage::TurnEndStatus::complete()),
                SyndicTimestamp::from_unix_millis(72_020),
            )
            .unwrap();
    }

    pub fn operation(&self) -> CompactionOperationRecord {
        operation(&self.service, self.storage, self.operation_id)
    }

    pub fn committed_tail(&self) -> Option<SyndicTurnId> {
        self.storage
            .thread(&self.service, self.thread_id, point_limit())
            .unwrap()
            .unwrap()
            .committed_tail()
    }

    pub fn input_gate(&self) -> syndic_storage::InputGateRecord {
        self.storage
            .input_gate(&self.service, self.thread_id, point_limit())
            .unwrap()
            .unwrap()
    }

    pub fn close(self) {
        let Self {
            directory,
            storage: _,
            service,
            harness: _,
            thread_id: _,
            yielding_turn_id: _,
            operation_id: _,
        } = self;
        Arc::try_unwrap(service).ok().unwrap().close().unwrap();
        drop(directory);
    }
}

pub fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn operation(
    home: &ProjectionConnectionService,
    storage: SyndicStorage,
    operation_id: CompactionOperationId,
) -> CompactionOperationRecord {
    storage
        .compaction_operation(home, operation_id, point_limit())
        .unwrap()
        .unwrap()
}
