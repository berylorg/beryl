use super::*;
use beryl_state::{HandoffFailureKind, ParentHandoffIdentity};

#[path = "parent_execution/recovery.rs"]
mod recovery;
#[path = "parent_execution/identity.rs"]
mod identity_tests;

pub(super) fn start() -> Fixture {
    let fixture = Fixture::new(false);
    parent_input::ready(&fixture);
    let prepared = fixture
        .service()
        .prepare_parent_input(
            fixture.job,
            parent_input::request(),
            CommandCancellation::new(),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Committed {
            result: DiscussionSettlementResult::StartingParent(_),
            later_failure: None,
            ..
        }
    ));
    fixture
}
fn identity(fixture: &Fixture) -> ParentHandoffIdentity {
    parent_input::identity(fixture)
}
fn observe(fixture: &Fixture) -> Option<PreparedDiscussionSettlement<'static>> {
    fixture
        .service()
        .prepare_parent_execution(
            fixture.job,
            support::timestamp(600),
            CommandCancellation::new(),
        )
        .unwrap()
}
pub(super) fn accepted(fixture: &Fixture) -> CasTurnSource {
    support::exact_cas::establish_turn(
        &fixture.store,
        fixture.syndic.clone(),
        id(30),
        identity(fixture).turn_id(),
        support::timestamp(501),
    )
}
pub(super) fn finish(fixture: &Fixture, source: &CasTurnSource, status: TurnEndStatus) {
    let turn = identity(fixture).turn_id();
    support::exact_cas::admit_event(
        &fixture.store,
        fixture.syndic.clone(),
        id(30),
        turn,
        source,
        SourceEventPayload::TurnActivated,
        support::timestamp(502),
    );
    support::exact_cas::correlate_user_item(
        &fixture.store,
        fixture.syndic.clone(),
        id(30),
        turn,
        parent_input::request().item_id,
        source,
        support::timestamp(503),
    );
    support::exact_cas::admit_event(
        &fixture.store,
        fixture.syndic.clone(),
        id(30),
        turn,
        source,
        SourceEventPayload::TurnEnded(status),
        support::timestamp(504),
    );
}
fn advance(fixture: &Fixture) {
    assert!(
        matches!(observe(fixture).unwrap().execute(), DiscussionSettlementOutcome::Committed { result: DiscussionSettlementResult::ParentActive(parent), later_failure: None, .. } if parent == identity(fixture))
    );
}
fn lifecycle(fixture: &Fixture) -> BranchHandoffJobLifecycle {
    fixture
        .state
        .durable_jobs()
        .job(&fixture.store, fixture.job)
        .unwrap()
        .unwrap()
        .lifecycle()
}

#[test]
fn exact_acceptance_and_terminal_results_publish_with_matching_child_disposition() {
    for (status, failure) in [
        (TurnEndStatus::complete(), None),
        (
            TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
            Some(HandoffFailureKind::ParentInterrupted),
        ),
        (
            TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
            Some(HandoffFailureKind::ParentTerminalFailure),
        ),
        (
            TurnEndStatus::incomplete(TurnIncompleteReason::StreamLost),
            Some(HandoffFailureKind::ParentIncomplete),
        ),
    ] {
        let fixture = start();
        assert!(observe(&fixture).is_none());
        let source = accepted(&fixture);
        finish(&fixture, &source, status);
        advance(&fixture);
        let job = fixture
            .state
            .durable_jobs()
            .job(&fixture.store, fixture.job)
            .unwrap()
            .unwrap();
        assert_eq!(
            job.state().parent_cas().unwrap().turn_id(),
            source.turn_id()
        );
        let prepared = observe(&fixture).unwrap();
        let audit = prepared.audit();
        assert_eq!(lifecycle(&fixture), BranchHandoffJobLifecycle::ParentActive);
        assert_eq!(
            fixture
                .syndic
                .discussion_handoff_gate(&fixture.store, id(36), limit())
                .unwrap(),
            Some(fixture.gate)
        );
        let result = match failure {
            None => DiscussionSettlementResult::ParentSucceeded(identity(&fixture)),
            Some(kind) => DiscussionSettlementResult::ParentFailed {
                parent: identity(&fixture),
                kind,
            },
        };
        assert!(
            matches!(prepared.execute(), DiscussionSettlementOutcome::Committed { result: actual, later_failure: None, .. } if actual == result)
        );
        assert_eq!(
            fixture.audit(&audit),
            DiscussionSettlementAuditOutcome::Settled(result)
        );
        assert_eq!(
            lifecycle(&fixture),
            if failure.is_none() {
                BranchHandoffJobLifecycle::Succeeded
            } else {
                BranchHandoffJobLifecycle::TerminalFailed
            }
        );
        assert_eq!(
            fixture
                .syndic
                .discussion_handoff_gate(&fixture.store, id(36), limit())
                .unwrap()
                .unwrap()
                .state(),
            DiscussionHandoffGateState::Open
        );
        assert_eq!(
            fixture
                .syndic
                .thread_attributes(&fixture.store, id(36), limit())
                .unwrap()
                .unwrap()
                .archive()
                == ThreadArchiveState::BranchDiscussionOpen,
            failure.is_some()
        );
        assert!(
            fixture
                .syndic
                .accepted_input(
                    &fixture.store,
                    identity(&fixture).accepted_input_id(),
                    limit()
                )
                .unwrap()
                .is_some()
        );
        fixture.store.close().unwrap();
    }
}

#[test]
fn stale_acceptance_and_cancelled_terminal_preparations_do_not_advance_job() {
    let fixture = start();
    let source = accepted(&fixture);
    let stale = observe(&fixture).unwrap();
    finish(&fixture, &source, TurnEndStatus::complete());
    assert!(matches!(
        stale.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        lifecycle(&fixture),
        BranchHandoffJobLifecycle::StartingParent
    );
    advance(&fixture);
    let cancellation = CommandCancellation::new();
    let prepared = fixture
        .service()
        .prepare_parent_execution(fixture.job, support::timestamp(600), cancellation.clone())
        .unwrap()
        .unwrap();
    cancellation.cancel();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    assert_eq!(lifecycle(&fixture), BranchHandoffJobLifecycle::ParentActive);
    support::converge_and_release_terminal_history(
        &fixture.store,
        fixture.syndic.clone(),
        id(30),
        identity(&fixture).turn_id(),
    );
    let next = support::exact_cas::submit_current_draft(
        &fixture.store,
        fixture.syndic.clone(),
        id(30),
        beryl_model::SyndicDraftId::from_bytes([242; 16]),
        beryl_model::SyndicItemId::from_bytes([243; 16]),
        "new parent task",
        support::timestamp(550),
    );
    assert!(matches!(
        observe(&fixture).unwrap().execute(),
        DiscussionSettlementOutcome::Committed {
            result: DiscussionSettlementResult::ParentSucceeded(_),
            ..
        }
    ));
    assert_eq!(
        fixture
            .syndic
            .thread(&fixture.store, id(30), limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(next)
    );
    fixture.store.close().unwrap();
}

#[test]
fn unknown_dispatch_waits_then_session_loss_fails_without_cas_identity() {
    let fixture = start();
    let turn = identity(&fixture).turn_id();
    support::exact_cas::activate_turn(
        &fixture.store,
        fixture.syndic.clone(),
        id(30),
        turn,
        support::timestamp(501),
    );
    assert!(observe(&fixture).is_none());
    let page = fixture
        .syndic
        .delivery_recovery_startup_page(
            &fixture.store,
            None,
            beryl_home_store::CursorReadLimits::new(
                DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS,
                DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
            )
            .unwrap(),
        )
        .unwrap();
    let source = page
        .records()
        .iter()
        .find(|source| source.thread_id() == id(30))
        .unwrap();
    let DeliveryRecoveryCase::Active(active) = fixture
        .syndic
        .classify_delivery_recovery(&fixture.store, source, limit())
        .unwrap()
    else {
        panic!("active dispatch")
    };
    support::discussion_input::committed(
        &fixture.store,
        fixture.syndic.abandon_active_binding(
            fixture.syndic.revision(&fixture.store).unwrap(),
            active
                .generic_abandonment(" authority lost", support::timestamp(510))
                .unwrap(),
        ),
    );
    let state = fixture
        .syndic
        .turn_state(&fixture.store, turn, limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .syndic
        .input_gate(&fixture.store, id(30), limit())
        .unwrap()
        .unwrap();
    let event = LiveSourceEvent::new(
        id(30),
        turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        None,
        SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
            TurnIncompleteReason::AuthorityLost,
        )),
        support::timestamp(510),
    )
    .unwrap();
    support::discussion_input::committed(
        &fixture.store,
        fixture
            .syndic
            .admit_live_source_event(fixture.syndic.revision(&fixture.store).unwrap(), event),
    );
    assert!(matches!(
        observe(&fixture).unwrap().execute(),
        DiscussionSettlementOutcome::Committed {
            result: DiscussionSettlementResult::ParentFailed {
                kind: HandoffFailureKind::UnrecoverablePostAppend,
                ..
            },
            later_failure: None,
            ..
        }
    ));
    let job = fixture
        .state
        .durable_jobs()
        .job(&fixture.store, fixture.job)
        .unwrap()
        .unwrap();
    assert_eq!(job.state().parent_cas(), None);
    assert_eq!(
        fixture
            .syndic
            .thread_attributes(&fixture.store, id(36), limit())
            .unwrap()
            .unwrap()
            .archive(),
        ThreadArchiveState::BranchDiscussionOpen
    );
    fixture.store.close().unwrap();
}
