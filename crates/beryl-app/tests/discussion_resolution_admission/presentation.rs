use super::*;
use beryl_app::discussion_presentation::*;
use beryl_state::{
    BranchHandoffJobRecord, HandoffFailureEvidence, HandoffFailureKind, HandoffJobTransition,
};

#[path = "presentation/custody.rs"]
mod custody;
#[path = "presentation/parent.rs"]
mod parent;

fn reader(fixture: &Fixture) -> DiscussionPresentationReader {
    fixture
        .service
        .test_configure_discussion_resolution(fixture.settlement.clone())
        .unwrap();
    fixture.service.discussion_presentation_reader()
}

fn job(fixture: &Fixture, id: JobId) -> BranchHandoffJobRecord {
    let command = fixture.service.live_home_command().unwrap();
    fixture
        .state
        .durable_jobs()
        .job(command.home(), id)
        .unwrap()
        .unwrap()
}

fn pause(fixture: &Fixture, id: JobId) -> BranchHandoffJobRecord {
    let command = fixture.service.live_home_command().unwrap();
    let before = fixture
        .state
        .durable_jobs()
        .job(command.home(), id)
        .unwrap()
        .unwrap();
    let transition = fixture
        .state
        .durable_jobs()
        .prepare_handoff_job_transition(
            command.home(),
            id,
            before.revision(),
            HandoffJobTransition::RetryableFailure(
                HandoffFailureEvidence::new(
                    HandoffFailureKind::RuntimeUnavailable,
                    Some("Runtime is unavailable"),
                )
                .unwrap(),
            ),
        )
        .unwrap();
    support::discussion_input::committed(command.home(), transition.contribution());
    drop(command);
    job(fixture, id)
}

#[test]
fn selected_open_admission_and_local_abandonment_share_one_publication_fence() {
    let fixture = Fixture::new();
    let reader = reader(&fixture);
    let ordinary = reader.observe(id(30)).unwrap();
    assert_eq!(ordinary.status(), None);
    let open = reader.observe(id(36)).unwrap();
    assert!(open.same_fence(&ordinary));
    assert!(
        open.home_observation()
            .same_interval(&ordinary.home_observation())
    );
    assert_eq!(open.status(), Some(DiscussionStatus::Open));
    assert_eq!(open.composer(), DiscussionComposerContribution::Writable);
    let command = fixture.service.live_home_command().unwrap();
    let revision = command.home().home_revision().unwrap();
    drop(command);
    let DiscussionResolutionAdmission::Prepared(prepared) = fixture
        .settlement
        .prepare_resolution_for_test(
            &fixture.context("prepared"),
            ResolutionIntentId::from_bytes([211; 16]),
            ResolutionText::new("Original payload").unwrap(),
            CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("original admission must prepare")
    };
    assert!(!open.current());
    assert!(
        open.try_elect(|| panic!("stale admission published"))
            .is_err()
    );
    let pending = reader.observe(id(36)).unwrap();
    assert!(!pending.same_fence(&open));
    assert_eq!(pending.status(), Some(DiscussionStatus::Open));
    assert_eq!(pending.composer(), DiscussionComposerContribution::Inert);
    assert!(pending.attempt().is_some());
    let command = fixture.service.live_home_command().unwrap();
    assert_eq!(command.home().home_revision().unwrap(), revision);
    drop(command);
    drop(prepared);
    assert!(!pending.current());
    let reopened = reader.observe(id(36)).unwrap();
    assert_eq!(reopened.status(), Some(DiscussionStatus::Open));
    assert_eq!(
        reopened.composer(),
        DiscussionComposerContribution::Writable
    );
    fixture.close();
    assert!(!reader.current());
    assert!(reader.observe(id(36)).is_err());
}

#[test]
fn exact_retry_stays_visible_while_reserved_and_resumes_the_original_job_once() {
    let fixture = Fixture::new();
    let reader = reader(&fixture);
    let admitted = admitted(
        fixture
            .admit("resolution", 210, "Keep original resolution")
            .unwrap(),
    );
    assert_eq!(
        reader.observe(id(36)).unwrap().status(),
        Some(DiscussionStatus::ResolutionPending)
    );
    let paused = pause(&fixture, admitted);
    let failed = reader.observe(id(36)).unwrap();
    assert_eq!(failed.status(), Some(DiscussionStatus::HandoffFailed));
    assert_eq!(failed.composer(), DiscussionComposerContribution::Inert);
    assert!(failed.detail().is_some());
    let descriptor = failed.retry().unwrap().clone();
    let prepared = reader
        .prepare_retry(&descriptor, CommandCancellation::new())
        .unwrap();
    let pending = reader.observe(id(36)).unwrap();
    assert!(pending.retry_pending());
    assert_eq!(pending.retry().unwrap().job_id(), admitted);
    assert!(
        reader
            .prepare_retry(&descriptor, CommandCancellation::new())
            .is_err()
    );
    drop(prepared);
    let current = reader.observe(id(36)).unwrap();
    assert!(!current.retry_pending());
    let command = fixture.service.live_home_command().unwrap();
    let before_syndic = fixture.syndic.revision(command.home()).unwrap();
    drop(command);
    let result = reader
        .prepare_retry(current.retry().unwrap(), CommandCancellation::new())
        .unwrap()
        .execute();
    assert_eq!(result.observation(), DiscussionRetryObservation::Committed);
    assert!(!result.has_original_failure());
    drop(result);
    assert!(fixture.operations.retained_audit(admitted).is_none());
    let resumed = job(&fixture, admitted);
    assert_eq!(resumed.job_id(), paused.job_id());
    assert_eq!(resumed.intent_id(), paused.intent_id());
    assert_eq!(resumed.request(), paused.request());
    assert_eq!(resumed.resolution(), paused.resolution());
    assert_eq!(resumed.attempt_ordinal(), paused.attempt_ordinal());
    assert_eq!(resumed.revision().get(), paused.revision().get() + 1);
    let command = fixture.service.live_home_command().unwrap();
    assert_eq!(
        fixture.syndic.revision(command.home()).unwrap(),
        before_syndic
    );
    drop(command);
    assert!(
        reader
            .prepare_retry(&descriptor, CommandCancellation::new())
            .is_err()
    );
    assert_eq!(
        reader.observe(id(36)).unwrap().status(),
        Some(DiscussionStatus::ResolutionPending)
    );
    fixture.close();
}

#[test]
fn an_original_admission_collision_is_unavailable_before_any_state_job_or_latest_pointer() {
    let fixture = Fixture::new();
    let reader = reader(&fixture);
    let cancellation = CommandCancellation::new();
    let DiscussionResolutionAdmission::Prepared(prepared) = fixture
        .settlement
        .prepare_resolution_for_test(
            &fixture.context("collision"),
            ResolutionIntentId::from_bytes([211; 16]),
            ResolutionText::new("Original never committed").unwrap(),
            cancellation.clone(),
        )
        .unwrap()
    else {
        panic!("original admission must prepare")
    };
    let audit = prepared.audit();
    cancellation.cancel();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    let command = fixture.service.live_home_command().unwrap();
    let other = fixture
        .syndic
        .prepare_discussion_handoff(
            command.home(),
            DiscussionHandoffMutation::Admit(fixture.source.clone()),
        )
        .unwrap();
    let pending_gate = other.intent().new_gate();
    support::discussion_input::committed(command.home(), other.contribution());
    assert!(
        fixture
            .state
            .durable_jobs()
            .latest_attempt(command.home(), id(36))
            .unwrap()
            .is_none()
    );
    let before = command.home().home_revision().unwrap();
    assert_eq!(
        audit
            .reconcile(command.home(), &fixture.syndic, &fixture.state)
            .unwrap(),
        DiscussionSettlementAuditOutcome::Collision
    );
    assert_eq!(command.home().home_revision().unwrap(), before);
    let release = fixture
        .syndic
        .prepare_discussion_handoff(
            command.home(),
            DiscussionHandoffMutation::Release {
                expected: pending_gate,
            },
        )
        .unwrap();
    support::discussion_input::committed(command.home(), release.contribution());
    assert_eq!(
        fixture
            .syndic
            .discussion_handoff_gate(
                command.home(),
                id(36),
                SyndicPointReadLimit::new(256).unwrap()
            )
            .unwrap()
            .unwrap()
            .state(),
        DiscussionHandoffGateState::Open
    );
    drop(command);
    let unavailable = reader.observe(id(36)).unwrap();
    assert_eq!(unavailable.status(), Some(DiscussionStatus::Unavailable));
    assert_eq!(
        unavailable.composer(),
        DiscussionComposerContribution::Inert
    );
    assert!(unavailable.detail().is_some());
    assert!(unavailable.retry().is_none());
    assert!(unavailable.attempt().is_some());
    assert!(unavailable.current());
    drop(audit);
    assert_eq!(
        reader.observe(id(36)).unwrap().status(),
        Some(DiscussionStatus::Unavailable)
    );
    assert!(matches!(
        fixture.admit("different-after-collision", 214, "Cannot replace original"),
        Err(DiscussionSettlementError::OriginalAttemptUnavailable)
    ));
    fixture.close();
}

#[test]
fn an_elected_selected_frame_orders_before_a_concurrent_local_admission_write() {
    let fixture = Fixture::new();
    let reader = reader(&fixture);
    let open = reader.observe(id(36)).unwrap();
    let (start, started) = std::sync::mpsc::sync_channel(0);
    let (entered, entering) = std::sync::mpsc::sync_channel(0);
    let (complete, completed) = std::sync::mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let fixture = &fixture;
        let worker = scope.spawn(move || {
            started.recv().unwrap();
            entered.send(()).unwrap();
            let prepared = fixture.settlement.prepare_resolution_for_test(
                &fixture.context("linearized"),
                ResolutionIntentId::from_bytes([212; 16]),
                ResolutionText::new("Ordered admission").unwrap(),
                CommandCancellation::new(),
            );
            complete.send(()).unwrap();
            prepared
        });
        open.try_elect_local(|| {
            start.send(()).unwrap();
            entering.recv().unwrap();
            completed
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
        })
        .unwrap();
        assert!(matches!(
            worker.join().unwrap(),
            Err(DiscussionSettlementError::ConcurrentChange)
        ));
        assert!(open.current());
        let prepared = fixture
            .settlement
            .prepare_resolution_for_test(
                &fixture.context("after-election"),
                ResolutionIntentId::from_bytes([212; 16]),
                ResolutionText::new("Ordered admission").unwrap(),
                CommandCancellation::new(),
            )
            .unwrap();
        assert!(!open.current());
        assert_eq!(
            reader.observe(id(36)).unwrap().composer(),
            DiscussionComposerContribution::Inert
        );
        drop(prepared);
    });
    fixture.close();
}

#[test]
fn admitted_preparation_closes_the_selected_gate_before_its_full_audit_exists() {
    let fixture = Fixture::new();
    let reader = reader(&fixture);
    let open = reader.observe(id(36)).unwrap();
    let context = fixture.context("preparing");
    let settlement = fixture.settlement.clone();
    let pause = fixture
        .faults
        .block_next(FaultPoint::BeforeReadConfirmation);
    let worker = std::thread::spawn(move || {
        settlement.prepare_resolution_for_test(
            &context,
            ResolutionIntentId::from_bytes([213; 16]),
            ResolutionText::new("Source preparation is pending").unwrap(),
            CommandCancellation::new(),
        )
    });
    let reached = pause.wait_until_reached(std::time::Duration::from_secs(5));
    if !reached {
        pause.release();
    }
    assert!(
        reached,
        "original source read did not reach its bounded pause"
    );
    let selected = reader.observe(id(36));
    pause.release();
    let prepared = worker.join().unwrap().unwrap();
    let selected = selected.unwrap();
    assert_eq!(selected.status(), Some(DiscussionStatus::Open));
    assert_eq!(selected.composer(), DiscussionComposerContribution::Inert);
    assert!(
        selected.attempt().is_none(),
        "pause must precede full audit registration"
    );
    assert!(!open.current());
    assert!(!selected.current());
    drop(prepared);
    assert_eq!(
        reader.observe(id(36)).unwrap().composer(),
        DiscussionComposerContribution::Writable
    );
    fixture.close();
}
