use std::{sync::Arc, time::Duration};

use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasThreadId, CasTurnId, SyndicThreadId,
};
use syndic_storage::{
    CompactionOperationId, CompactionOperationNonce, SyndicTimestamp, TurnEndStatus,
};

use super::{live_command, router};
use crate::cas_projection::{
    connection::{
        registry::LoadedThreadKey,
        router::{
            LiveEventPoll, ProvenTerminalOutcome, TargetLossAcquisition, TargetTurnRegistration,
        },
    },
    context_compaction::ContextCompactionTargetAuthority,
};

fn register_compaction(
    router: &Arc<crate::cas_projection::connection::router::EventRouter>,
    suffix: &str,
    owner_byte: u8,
) -> (super::TargetRegistration, CasThreadId, CasTurnId) {
    let owner = SyndicThreadId::from_bytes([owner_byte; 16]);
    let operation = CompactionOperationId::new(
        owner,
        CompactionOperationNonce::from_bytes([owner_byte.wrapping_add(1); 16]),
    );
    let provider_turn = operation.provider_turn_id();
    let cas_thread = CasThreadId::new(format!("compaction-{suffix}")).unwrap();
    let command = live_command(router);
    let registration = router
        .register(
            &command,
            LoadedThreadKey {
                runtime_id: router.runtime_id,
                process_generation: router.process_generation,
                cas_thread_id: cas_thread.clone(),
            },
            owner,
            CasLoadedSessionGeneration::new(
                router.process_generation,
                CasLoadedThreadGeneration::new(u64::from(owner_byte)).unwrap(),
            ),
            1,
            Duration::from_secs(1),
            TargetTurnRegistration::ContextCompaction(ContextCompactionTargetAuthority::new(
                operation,
                provider_turn,
            )),
        )
        .unwrap();
    (
        registration,
        cas_thread,
        CasTurnId::new(format!("turn-{suffix}")).unwrap(),
    )
}

#[test]
fn context_compaction_router_terminal_publication_wins_loss_after_durable_control() {
    let router = router(71);
    let (registration, cas_thread, cas_turn) = register_compaction(&router, "terminal", 71);
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
    let terminal = router
        .acquire_source_publication(&cas_thread, &cas_turn)
        .unwrap();
    let loss_router = Arc::clone(&router);
    let proof = registration.proof();
    let loss = std::thread::spawn(move || {
        let command = super::live_command(&loss_router);
        loss_router.acquire_target_loss(command, &proof)
    });

    terminal
        .finish_terminal(ProvenTerminalOutcome::new(
            TurnEndStatus::complete(),
            SyndicTimestamp::from_unix_millis(71),
        ))
        .unwrap();

    assert!(matches!(
        loss.join().unwrap().unwrap(),
        TargetLossAcquisition::ProvenTerminal(outcome)
            if outcome.status() == TurnEndStatus::complete()
    ));
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::ProvenTerminal(_)
    ));
}

#[test]
fn context_compaction_loss_acquires_without_ordinary_pending_activation() {
    let router = router(72);
    let (registration, _, _) = register_compaction(&router, "loss", 72);

    assert!(matches!(
        router
            .acquire_target_loss(super::live_command(&router), &registration.proof())
            .unwrap(),
        TargetLossAcquisition::Authority(_)
    ));
}
