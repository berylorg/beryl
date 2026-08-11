use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

use beryl_home_store::{CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    BindingRevision, CasThreadId, CasTurnId, InputGateRevision, SyndicDraftId,
    SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    ContentAppend, ContentBuild, CreateThread, PreparedContent, SourceEventPayload,
    StopAdmissionIneligibility, StopAdmissionRead, StopCause, StopOperationTarget,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnStateRevision,
};

use super::*;
use crate::{
    LifecycleYieldOutcome,
    cas_projection::{
        PendingTurnActivation,
        connection::{TargetTurnRegistration, registry::LoadedThreadKey},
    },
};

#[allow(dead_code)]
mod exact_cas {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../syndic-storage/tests/support/exact_cas.rs"
    ));
}

static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

fn timestamp(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(home: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    match home.execute(command) {
        CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("stop fixture command committed with later failure: {outcome:?}"),
        CommandOutcome::NotCommitted { evidence } => panic!("stop fixture command was not committed: {evidence:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("stop fixture command was indeterminate: {outcome:?}"),
    }
}

fn stage_prepared_content(home: &HomeStore, storage: SyndicStorage, content: &PreparedContent) {
    execute(
        home,
        storage.begin_content(
            storage.revision(home).unwrap(),
            ContentBuild::from_prepared(content),
        ),
    );
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        let next = append.next_manifest().clone();
        execute(
            home,
            storage.append_content(storage.revision(home).unwrap(), append),
        );
        manifest = next;
    }
}

struct StopFixture {
    _directory: tempfile::TempDir,
    home: Arc<HomeStore>,
    storage: SyndicStorage,
    coordinator: Arc<StopCoordinator>,
    command_gate: crate::cas_projection::persistent_failure::MasterCommandGate,
    router: Arc<EventRouter>,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    target: StopOperationTarget,
    proof: StopTargetProof,
}

impl StopFixture {
    fn new(seed: u8) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut home = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut home).unwrap();
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        execute(
            &home,
            storage.create_thread(
                storage.revision(&home).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    exact_cas::execution_binding(),
                    timestamp(1),
                ),
            ),
        );
        let turn = exact_cas::submit_current_draft(
            &home,
            storage,
            thread,
            SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]),
            SyndicItemId::from_bytes([seed.wrapping_add(3); 16]),
            "coordinator stop target",
            timestamp(2),
        );
        let source = exact_cas::establish_turn(&home, storage, thread, turn, timestamp(3));
        exact_cas::admit_event(
            &home,
            storage,
            thread,
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(4),
        );
        let target = match storage
            .stop_admission_read(&home, thread, point_limit())
            .unwrap()
        {
            StopAdmissionRead::Admissible(candidate) => candidate.target().clone(),
            other => panic!("active fixture must admit a stop, observed {other:?}"),
        };
        let home_id = home.home_id();
        let home_generation = home.health().generation().unwrap();
        let home = Arc::new(home);
        let command_gate = crate::cas_projection::persistent_failure::MasterCommandGate::new(
            crate::cas_projection::persistent_failure::ProjectionServiceGeneration::allocate()
                .unwrap(),
            None,
        );
        let coordinator = Arc::new(StopCoordinator::new(
            &home,
            home_id,
            home_generation,
            storage,
            command_gate.authorizer(),
        ));
        let router = Arc::new(
            EventRouter::new_with_scheduler(
                target.runtime_id(),
                target.loaded_generation().process(),
                NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed),
                crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(
                ),
                command_gate.authorizer(),
                None,
            )
            .unwrap(),
        );
        let router_command = router.authorize_command_for_test().unwrap();
        router
            .register(
                &router_command,
                LoadedThreadKey {
                    runtime_id: target.runtime_id(),
                    process_generation: target.loaded_generation().process(),
                    cas_thread_id: target.cas_thread_id().clone(),
                },
                thread,
                target.loaded_generation(),
                home_generation.get(),
                Duration::from_secs(1),
                TargetTurnRegistration::Pending(PendingTurnActivation::new(
                    thread,
                    turn,
                    BindingRevision::new(1).unwrap(),
                    InputGateRevision::new(1).unwrap(),
                    TurnStateRevision::FIRST,
                    SyndicExecutionSnapshotId::from_bytes([seed.wrapping_add(4); 16]),
                    timestamp(4),
                )),
            )
            .unwrap();
        drop(router_command);
        router.activate_stop_target_for_test(target.cas_thread_id(), target.cas_turn_id().clone());
        let proof = router
            .stop_target(thread, target.cas_thread_id(), target.cas_turn_id())
            .unwrap();
        Self {
            _directory: directory,
            home,
            storage,
            coordinator,
            command_gate,
            router,
            thread,
            turn,
            target,
            proof,
        }
    }

    fn wrong_storage_target_proof(&self, seed: u8) -> StopTargetProof {
        let cas_thread = CasThreadId::new(format!("wrong-stop-thread-{seed}")).unwrap();
        let cas_turn = CasTurnId::new(format!("wrong-stop-turn-{seed}")).unwrap();
        let router_command = self.router.authorize_command_for_test().unwrap();
        self.router
            .register(
                &router_command,
                LoadedThreadKey {
                    runtime_id: self.target.runtime_id(),
                    process_generation: self.target.loaded_generation().process(),
                    cas_thread_id: cas_thread.clone(),
                },
                self.thread,
                self.target.loaded_generation(),
                self.home.health().generation().unwrap().get(),
                Duration::from_secs(1),
                TargetTurnRegistration::Pending(PendingTurnActivation::new(
                    self.thread,
                    self.turn,
                    BindingRevision::new(1).unwrap(),
                    InputGateRevision::new(1).unwrap(),
                    TurnStateRevision::FIRST,
                    SyndicExecutionSnapshotId::from_bytes([seed.wrapping_add(1); 16]),
                    timestamp(5),
                )),
            )
            .unwrap();
        drop(router_command);
        self.router
            .activate_stop_target_for_test(&cas_thread, cas_turn.clone());
        self.router
            .stop_target(self.thread, &cas_thread, &cas_turn)
            .unwrap()
    }

    fn live_stop(&self) -> syndic_storage::SyndicLiveStopOperation {
        match self
            .storage
            .stop_admission_read(&self.home, self.thread, point_limit())
            .unwrap()
        {
            StopAdmissionRead::Stopping(live) => *live,
            other => panic!("fixture must retain a live stop, observed {other:?}"),
        }
    }
}

fn failure_identity(
    fixture: &StopFixture,
) -> crate::cas_projection::persistent_failure::PersistentFailureCutIdentity {
    crate::cas_projection::persistent_failure::PersistentFailureCutIdentity::new(
        fixture.home.home_id(),
        fixture.home.health().generation().unwrap(),
        fixture.command_gate.service_generation(),
        crate::cas_projection::persistent_failure::PersistentFailureGeneration::FIRST,
    )
}
