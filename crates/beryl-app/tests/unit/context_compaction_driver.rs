use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
    CasTurnId, RuntimeId,
};
use syndic_storage::TurnEndStatus;

use super::*;
use crate::cas_projection::{
    LiveEventPoll, ProjectionServiceGeneration,
    connection::{
        CompactionTestTargetRegistration as TargetRegistration, EventRouter,
        LiveEventTargetHandoffError, LoadedThreadKey, ProvenTerminalOutcome,
        TargetHandoffRequirement, TargetTurnRegistration,
    },
    persistent_failure::MasterCommandGate,
};

struct DriverFixture {
    gate: MasterCommandGate,
    router: Arc<EventRouter>,
    registration: TargetRegistration,
    cas_thread: CasThreadId,
    cas_turn: CasTurnId,
    local: Arc<LocalCompaction>,
    driver: dispatch::CompactionDriverGuard,
    custody: Arc<CompactionCustodyPool>,
}

impl DriverFixture {
    fn new(seed: u8) -> Self {
        let gate = MasterCommandGate::new(ProjectionServiceGeneration::allocate().unwrap(), None);
        let runtime_id = RuntimeId::from_bytes([seed; 16]);
        let process_generation = CasProcessGeneration::new(u64::from(seed)).unwrap();
        let router = Arc::new(
            EventRouter::new_with_scheduler(
                runtime_id,
                process_generation,
                1,
                AcceptedInputSchedulerSignal::new(),
                gate.authorizer(),
                None,
            )
            .unwrap(),
        );
        let command = gate.authorizer().authorize().unwrap();
        let operation_id = CompactionOperationId::new(
            SyndicThreadId::from_bytes([seed; 16]),
            CompactionOperationNonce::from_bytes([seed.wrapping_add(1); 16]),
        );
        let cas_thread = CasThreadId::new(format!("compaction-driver-{seed}")).unwrap();
        let cas_turn = CasTurnId::new(format!("compaction-turn-{seed}")).unwrap();
        let registration = router
            .register(
                &command,
                LoadedThreadKey {
                    runtime_id,
                    process_generation,
                    cas_thread_id: cas_thread.clone(),
                },
                operation_id.thread_id(),
                CasLoadedSessionGeneration::new(
                    process_generation,
                    CasLoadedThreadGeneration::new(1).unwrap(),
                ),
                1,
                Duration::from_secs(1),
                TargetTurnRegistration::ContextCompaction(ContextCompactionTargetAuthority::new(
                    operation_id,
                    operation_id.provider_turn_id(),
                )),
            )
            .unwrap();
        let custody = CompactionCustodyPool::new(
            crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(),
        );
        let local = Arc::new(LocalCompaction::new(
            operation_id,
            CompactionAttemptNonce::from_bytes([seed.wrapping_add(2); 16]),
            CompactionOrigin::Manual,
            ResolvedContextCompactionTimeout::fixed(Duration::from_secs(1)),
            CompactionCommandCustody {
                command,
                preparation: custody.reserve().unwrap().prepare_command(),
            },
        ));
        let driver = dispatch::CompactionDriverGuard(Arc::clone(&local));
        router
            .authorize_context_compaction_command(&registration.proof())
            .unwrap();
        router
            .acquire_compaction_thread_status(&cas_thread)
            .unwrap()
            .finish()
            .unwrap();
        router
            .acquire_compaction_turn_started(&cas_thread, &cas_turn)
            .unwrap()
            .finish()
            .unwrap();
        Self {
            gate,
            router,
            registration,
            cas_thread,
            cas_turn,
            local,
            driver,
            custody,
        }
    }
}

#[test]
fn original_driver_permit_survives_local_completion_until_router_terminal_handoff() {
    let fixture = DriverFixture::new(211);
    let pressure: Vec<_> = (0..71)
        .map(|_| fixture.custody.reserve().unwrap())
        .collect();
    assert!(fixture.custody.reserve().is_none());
    let terminal = fixture
        .router
        .acquire_source_publication(&fixture.cas_thread, &fixture.cas_turn)
        .unwrap();
    fixture.local.mark_accepted();
    fixture.local.complete(ContextCompactionOutcome::Succeeded);
    assert!(fixture.local.is_finished());
    assert_eq!(fixture.custody.in_use(), 72);
    assert!(fixture.local.command_is_current());
    assert!(matches!(
        fixture.registration.poll_for_test(),
        LiveEventPoll::Quiet
    ));
    assert!(matches!(
        fixture.router.handoff_target(
            &fixture.registration,
            TargetHandoffRequirement::ProvenTerminal,
        ),
        Err(LiveEventTargetHandoffError::TargetNotTerminal)
    ));
    assert!(fixture.local.command_is_current());
    terminal
        .finish_terminal(ProvenTerminalOutcome::new(
            TurnEndStatus::complete(),
            SyndicTimestamp::from_unix_millis(1),
        ))
        .unwrap();
    assert!(matches!(
        fixture.registration.poll_for_test(),
        LiveEventPoll::ProvenTerminal(_)
    ));
    fixture
        .router
        .handoff_target(
            &fixture.registration,
            TargetHandoffRequirement::ProvenTerminal,
        )
        .unwrap();
    assert!(fixture.local.command_is_current());
    drop(fixture.driver);
    assert_eq!(fixture.custody.in_use(), 71);
    let replacement = fixture.custody.reserve().unwrap();
    drop(replacement);
    drop(pressure);
    assert_eq!(fixture.custody.in_use(), 0);
    assert!(!fixture.local.command_is_current());
    assert!(fixture.local.command.lock().unwrap().is_none());
    assert_eq!(fixture.local.wait(), ContextCompactionOutcome::Succeeded);
}

#[test]
fn original_driver_epoch_loss_while_router_terminal_is_pending_forbids_handoff() {
    let fixture = DriverFixture::new(212);
    let terminal = fixture
        .router
        .acquire_source_publication(&fixture.cas_thread, &fixture.cas_turn)
        .unwrap();
    fixture.local.complete(ContextCompactionOutcome::Succeeded);
    assert!(fixture.local.command_is_current());
    assert!(matches!(
        fixture.registration.poll_for_test(),
        LiveEventPoll::Quiet
    ));
    let _ = fixture.gate.close_for_shutdown();
    assert_eq!(fixture.custody.in_use(), 1);
    assert!(!fixture.local.command_is_current());
    assert!(matches!(
        fixture.router.handoff_target(
            &fixture.registration,
            TargetHandoffRequirement::ProvenTerminal,
        ),
        Err(LiveEventTargetHandoffError::ConnectionRetired)
    ));
    assert_eq!(fixture.local.wait(), ContextCompactionOutcome::Succeeded);
    drop(terminal);
    drop(fixture.driver);
    assert!(fixture.local.command.lock().unwrap().is_none());
}

#[test]
fn compaction_driver_unwind_releases_poisoned_custody_with_result_waiter_retained() {
    let fixture = DriverFixture::new(213);
    fixture.local.complete(ContextCompactionOutcome::Failed);
    let local = Arc::clone(&fixture.local);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _locked = local.command.lock().unwrap();
            panic!("poison command custody");
        }))
        .is_err()
    );
    assert_eq!(fixture.custody.in_use(), 1);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _driver = fixture.driver;
            panic!("unwind driver cleanup");
        }))
        .is_err()
    );
    assert_eq!(fixture.custody.in_use(), 0);
    assert_eq!(fixture.local.wait(), ContextCompactionOutcome::Failed);
    let replacement = fixture.custody.reserve().unwrap();
    assert_eq!(fixture.custody.in_use(), 1);
    drop(replacement);
}
