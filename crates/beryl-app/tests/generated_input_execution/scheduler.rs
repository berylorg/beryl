use super::*;
use beryl_app::discussion_settlement::*;
use beryl_home_store::CommandCancellation;
use beryl_state::BranchHandoffJobLifecycle;
use std::{
    num::NonZeroUsize,
    time::{Duration, Instant},
};

struct Tools {
    lifecycle: Noop,
    branch: Noop,
}
impl OrdinaryDynamicToolAuthority for Tools {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        OrdinaryDynamicToolHandlers::new(&mut self.lifecycle, &mut self.branch)
    }
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + server::TIMEOUT;
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "generated scheduler condition timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn process_scheduler_resumes_on_slot_release_but_requires_explicit_handoff_retry() {
    let mut configured = None;
    let mut sessions = None;
    let fixture =
        syndic::Fixture::new_with_execution_authority(132, |home, state, syndic, process| {
            let handoff = DiscussionSettlementService::new(
                DiscussionSettlementOperations::new(process.clone(), NonZeroUsize::new(1).unwrap()),
                home.service_reference(),
                state.clone(),
                syndic.clone(),
            );
            configured = Some(handoff.clone());
            let (provider, registry) = ProcessScheduledExecutionProvider::new();
            sessions = Some(registry);
            Box::new(provider.with_discussion_settlement(handoff))
        });
    let sessions = sessions.unwrap();
    let (handoff, input, receipt) =
        generated_handoff::seed_with_service(&fixture, "Exact scheduled resolution", configured);
    let thread = input.thread_id();
    let job_id = beryl_model::JobId::from_bytes(*input.id().as_bytes());
    let limit = SyndicPointReadLimit::new(400_000).unwrap();
    let execution = fixture
        .storage
        .thread_execution(&fixture.home(), thread, limit)
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let turn = fixture
        .storage
        .turn(&fixture.home(), receipt.parent_turn_id, limit)
        .unwrap()
        .unwrap();
    let parent = fixture
        .storage
        .turn(&fixture.home(), turn.parent().turn().unwrap(), limit)
        .unwrap()
        .unwrap();
    let selected = fixture.selected_path(thread);
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.id()),
        selected.thread_revision(),
        parent.chain_digest(),
    );
    let current = fixture
        .storage
        .current_binding(&fixture.home(), thread, limit)
        .unwrap()
        .unwrap();
    support::discussion_input::committed(
        &fixture.home(),
        fixture.storage.publish_valid_binding(
            fixture.storage.revision(&fixture.home()).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                execution.clone(),
                CasThreadId::new("generated-scheduled").unwrap(),
                represented,
                CasNativeTurnCount::new(parent.depth().get()),
                beryl_app::conversation_tools::ConversationToolRegistry::canonical().profile(),
                CasLineageProof::native(NativeCasLineage::Resume, represented).unwrap(),
            ),
        ),
    );
    let reservation = handoff
        .reserve_parent_dispatch(job_id, thread, receipt.parent_turn_id)
        .unwrap();
    let server = server::NormalTerminalServer::spawn_generated_rejection_then_terminal(
        "generated-scheduled",
        "Discussion resolution:\n\nExact scheduled resolution",
        execution.root_path().as_str(),
    );
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), server::AUTHORIZATION);
    let session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution.runtime_id(),
            CasProcessGeneration::new(59_902).unwrap(),
            Path::new(execution.root_path().as_str()),
            server::TIMEOUT,
        )
        .unwrap();
    sessions
        .register(
            thread,
            execution,
            session,
            ScheduledOrdinaryRequestPolicy::new(
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                server::TIMEOUT,
                OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), server::TIMEOUT),
            ),
            fixture.state.assets(),
            Box::new(Tools {
                lifecycle: Noop,
                branch: Noop,
            }),
        )
        .unwrap();
    fixture.store.notify_scheduled_ordinary_execution_ready();
    server.wait_for_projection();
    wait_until(|| {
        fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_active()
            == 0
    });
    assert_eq!(server.generated_turn_starts(), 0);
    drop(reservation);
    wait_until(|| {
        fixture
            .state
            .durable_jobs()
            .job(&fixture.home(), job_id)
            .unwrap()
            .unwrap()
            .lifecycle()
            == BranchHandoffJobLifecycle::RetryableFailed
    });
    let before = fixture
        .store
        .accepted_input_scheduler_diagnostics()
        .recovered_pending_pass_count();
    fixture.store.notify_scheduled_ordinary_execution_ready();
    wait_until(|| {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        diagnostics.recovered_pending_pass_count() > before && diagnostics.workers_active() == 0
    });
    assert_eq!(server.generated_turn_starts(), 1);
    let quiet = fixture
        .store
        .accepted_input_scheduler_diagnostics()
        .recovered_pending_pass_count();
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .recovered_pending_pass_count()
            <= quiet + 2,
        "paused handoff must not generate a session-return wake loop"
    );
    let job = fixture
        .state
        .durable_jobs()
        .job(&fixture.home(), job_id)
        .unwrap()
        .unwrap();
    assert!(matches!(
        handoff
            .prepare_retry(job_id, job.revision(), CommandCancellation::new())
            .unwrap()
            .execute(),
        DiscussionSettlementOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    wait_until(|| {
        fixture
            .storage
            .turn_state(&fixture.home(), receipt.parent_turn_id, limit)
            .unwrap()
            .unwrap()
            .lifecycle()
            == TurnLifecycle::Complete
    });
    assert_eq!(server.generated_turn_starts(), 2);
    wait_until(|| {
        fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_active()
            == 0
    });
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    server.join();
    drop(directory);
}
