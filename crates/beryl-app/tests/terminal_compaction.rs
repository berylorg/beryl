#![cfg(feature = "test-faults")]

#[allow(dead_code, unused_imports)]
#[path = "accepted_next_scheduler/support.rs"]
mod scheduler_support;
#[path = "support/compaction_server.rs"]
mod server;
#[path = "support/ordinary_compaction.rs"]
mod support;
#[path = "projection/syndic.rs"]
mod syndic;

use std::{thread, time::Duration};

use beryl_app::{
    LifecycleYieldOutcome,
    cas_projection::{
        ContextCompactionOutcome, ContextCompactionRequest, OrdinaryTurnExecutionOutcome,
        OrdinaryTurnExecutionRequest,
    },
};
use beryl_backend::TurnStartOptions;
use syndic_storage::{BindingState, CompactionAdmissionRead};

use server::{CompactionServer, SUBMITTED_TEXT, TIMEOUT};
use support::{execute, obtain};
use syndic::Fixture;

const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[test]
fn ordinary_terminal_compacts_current_prefix_without_rewriting_establishment_lineage() {
    let mut fixture = Fixture::new(208);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let selected = fixture.selected_path(fixture.thread);
    let server = CompactionServer::spawn(true);
    let (session, projection) = obtain(&fixture, &server);
    let lineage = projection.lineage_proof();
    let loaded_generation = projection.loaded_session_generation();
    let initial_revision = projection.binding_revision();
    assert_eq!(lineage.established_prefix().tail(), None);
    let request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT)
        .with_context_compaction_timeout(Duration::from_secs(1));
    thread::scope(|scope| {
        let worker = scope.spawn(|| execute(&fixture, projection, &request));
        server.wait_started();
        scheduler_support::wait_until("register lifecycle yield", || {
            fixture
                .store
                .record_lifecycle_yield_outcome(
                    fixture.thread,
                    submitted.turn,
                    LifecycleYieldOutcome::PhaseContinue,
                )
                .unwrap()
                .then_some(())
        });
        server.finish_turn();
        let result = worker.join().unwrap();
        assert!(
            matches!(
                result,
                Ok(OrdinaryTurnExecutionOutcome::LifecycleContinuationScheduled { .. })
            ),
            "expected exact lifecycle compaction: {result:?}"
        );
        server.wait_compaction();
        let home = fixture.home();
        let binding = fixture
            .storage
            .current_binding(&home, fixture.thread, scheduler_support::point_limit())
            .unwrap()
            .unwrap();
        let BindingState::Valid(usable) = binding.binding().state() else {
            panic!("compaction must preserve a valid ordinary binding")
        };
        assert_eq!(usable.lineage(), lineage);
        assert_eq!(usable.represented_prefix().tail(), Some(submitted.turn));
        assert_ne!(usable.represented_prefix(), lineage.established_prefix());
        assert_eq!(
            binding.binding().revision().get(),
            initial_revision.get() + 2
        );
        assert_eq!(fixture.selected_path(fixture.thread), selected);
        let CompactionAdmissionRead::Existing(operation) = fixture
            .storage
            .compaction_admission_read(&home, fixture.thread, scheduler_support::point_limit())
            .unwrap()
        else {
            panic!("one live compaction must own the current prefix")
        };
        assert_eq!(operation.target().loaded_generation(), loaded_generation);
        assert_eq!(
            operation.target().binding_revision(),
            binding.binding().revision()
        );
        drop(home);
        assert_eq!(
            fixture
                .store
                .compact_thread(ContextCompactionRequest::new(
                    fixture.thread,
                    Duration::from_secs(20)
                ))
                .unwrap(),
            ContextCompactionOutcome::StillRunning
        );
        let after = fixture
            .storage
            .current_binding(
                &fixture.home(),
                fixture.thread,
                scheduler_support::point_limit(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(after.binding(), binding.binding());
    });
    session.invalidate_connection();
    drop(session);
    server.join();
}
