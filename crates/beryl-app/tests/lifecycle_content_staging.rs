#![cfg(feature = "test-faults")]

#[path = "lifecycle_content_staging/failures.rs"]
mod failures;
#[path = "lifecycle_content_staging/support.rs"]
mod support;
#[path = "projection/syndic.rs"]
mod syndic;

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

use beryl_home_store::CommandOutcome;
use support::{LifecycleFixture, point_limit};
use syndic_storage::{
    CompactionOperationState, CompactionSettlement, ContentLifecycle, InputGateState,
    prepare_lifecycle_continuation_content, test_faults::lifecycle_content_canonical_records,
};

#[test]
fn app_publishes_sealed_fixed_content_once_and_reuses_without_revision_advance() {
    let source = syndic::Fixture::new(201);
    let expected = prepare_lifecycle_continuation_content().unwrap();
    let before = source.home().home_revision().unwrap();
    let first = source
        .store
        .stage_context_compaction_continuation_for_test()
        .unwrap();
    assert_eq!(
        source.home().home_revision().unwrap(),
        before.checked_next().unwrap()
    );
    assert_eq!(first.id(), expected.id());
    assert_eq!(first.summary(), expected.summary());
    assert_eq!(first.encoding(), expected.encoding());
    let home_revision = source.home().home_revision().unwrap();
    let domain_revision = source.storage.revision(&source.home()).unwrap();
    let records = lifecycle_content_canonical_records(&source.home(), &source.storage);
    assert_eq!(records.len(), 5);
    for _ in 0..3 {
        assert_eq!(
            source
                .store
                .stage_context_compaction_continuation_for_test()
                .unwrap(),
            first
        );
        assert_eq!(source.home().home_revision().unwrap(), home_revision);
        assert_eq!(
            source.storage.revision(&source.home()).unwrap(),
            domain_revision
        );
        assert_eq!(
            lifecycle_content_canonical_records(&source.home(), &source.storage),
            records
        );
        assert!(source.home().pending_reconciliations().is_empty());
    }
    let manifest = source
        .storage
        .content_manifest(&source.home(), expected.id(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(manifest.lifecycle(), ContentLifecycle::Sealed);
    assert_eq!(manifest.owner(), None);
    assert_eq!(manifest.sealed_reference(), Some(first));
    let (directory, service) = source.into_service();
    assert!(matches!(
        service.close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    drop(directory);
}

#[test]
fn successful_compaction_admits_one_fixed_continuation_from_fresh_or_reused_content() {
    for reused in [false, true] {
        let fixture = LifecycleFixture::new(202 + u8::from(reused), 210 + u8::from(reused));
        if reused {
            fixture
                .service
                .stage_context_compaction_continuation_for_test()
                .unwrap();
        }
        fixture.publish_success_prefix();
        fixture.publish_success_terminal();
        let operation = fixture.operation();
        let CompactionOperationState::Consumed(witness) = operation.state() else {
            panic!("successful compaction remains unconsumed: {operation:?}");
        };
        let CompactionSettlement::LifecycleContinuation {
            turn_id,
            content_id,
            ..
        } = witness.settlement()
        else {
            panic!("continuation was not admitted: {witness:?}");
        };
        assert_eq!(
            *content_id,
            prepare_lifecycle_continuation_content().unwrap().id()
        );
        assert_eq!(
            fixture.input_gate().state(),
            &InputGateState::PendingTurn(*turn_id)
        );
        assert_eq!(fixture.input_gate().live_count(), 0);
        let diagnostics = fixture.service.context_compaction_diagnostics();
        assert_eq!(diagnostics.retained_operations(), 0);
        assert_eq!(diagnostics.lifecycle_continuation_failures(), 0);
        assert_eq!(
            fixture
                .service
                .take_terminal_lifecycle_yield_outcome(fixture.thread_id, fixture.yielding_turn_id)
                .unwrap(),
            None
        );
        let revision = fixture
            .service
            .live_home_command()
            .unwrap()
            .home()
            .home_revision()
            .unwrap();
        fixture
            .service
            .stage_context_compaction_continuation_for_test()
            .unwrap();
        assert_eq!(
            fixture
                .service
                .live_home_command()
                .unwrap()
                .home()
                .home_revision()
                .unwrap(),
            revision
        );
        assert_eq!(fixture.operation(), operation);
        fixture.close();
    }
}

#[test]
fn accepted_user_work_wins_after_atomic_content_publication() {
    let fixture = LifecycleFixture::with_accepted_next(204, 212);
    let gate_before = fixture.input_gate();
    let tail_before = fixture.committed_tail();
    let wakes_before = fixture
        .service
        .accepted_input_scheduler_diagnostics()
        .wake_count();
    fixture.publish_success_prefix();
    fixture.publish_success_terminal();
    let operation = fixture.operation();
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("compaction remains unconsumed: {operation:?}");
    };
    assert_eq!(
        witness.settlement(),
        &CompactionSettlement::LifecycleUserWorkWon
    );
    let after = fixture.input_gate();
    assert_eq!(after.state(), &InputGateState::Idle);
    assert_eq!(after.live_count(), 1);
    assert_eq!(
        after.accepted_high_water(),
        gate_before.accepted_high_water()
    );
    assert_eq!(
        after.live_logical_utf8_bytes(),
        gate_before.live_logical_utf8_bytes()
    );
    assert_eq!(fixture.committed_tail(), tail_before);
    assert!(
        fixture
            .service
            .accepted_input_scheduler_diagnostics()
            .wake_count()
            > wakes_before
    );
    assert_eq!(
        fixture
            .service
            .context_compaction_diagnostics()
            .lifecycle_continuation_failures(),
        0
    );
    fixture.close();
}

fn assert_committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "expected clean commit: {outcome:?}"
    );
}
