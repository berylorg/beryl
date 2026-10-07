use super::*;
use beryl_app::cas_projection::{
    RuntimeInterest, RuntimeInterestConfig, RuntimeInterestKind, RuntimeInterestStatus,
    RuntimeInterestTestHarness, RuntimeInterestTestProbe,
};
use beryl_backend::ManagedBackendLaunchSpec;
use beryl_model::{
    AdmittedHostPath, CasProcessGeneration, PathFlavor, RuntimeMode, RuntimeNativePath,
};
use std::{sync::Arc, time::Duration};

fn owner(f: &Fixture) -> RuntimeInterestTestHarness {
    RuntimeInterestTestHarness::with_enrollments(
        RuntimeInterestConfig::new(
            NonZeroUsize::new(1).unwrap(),
            NonZeroUsize::new(4).unwrap(),
            Duration::from_secs(5),
        )
        .unwrap(),
        f.operations.clone(),
    )
}

fn acquire(f: &Fixture, owner: &RuntimeInterestTestHarness) -> Arc<RuntimeInterest> {
    let binding = f
        .syndic
        .thread_execution(&f.home, id(30), limit())
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let native = |path| {
        RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
    };
    let spec = ManagedBackendLaunchSpec::new(
        binding.runtime_id(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\activity-test\codex.exe")
            .unwrap(),
        RuntimeMode::Host,
        beryl_model::RuntimeLaunchForm::CodexCli,
        native(r"C:\activity-test\codex.exe"),
        binding.root_path().clone(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\activity-test\tokens").unwrap(),
        native(r"C:\activity-test\tokens"),
    )
    .unwrap();
    let interest = owner
        .acquire(
            spec,
            binding,
            RuntimeInterestKind::RequiredWork,
            RuntimeInterestTestProbe::new(CasProcessGeneration::new(91).unwrap()),
        )
        .unwrap();
    assert!(matches!(
        interest.wait_for_change(RuntimeInterestStatus::Starting, Duration::from_secs(5)),
        RuntimeInterestStatus::Ready(_)
    ));
    Arc::new(interest)
}

fn token(interest: &RuntimeInterest, f: &Fixture) -> ActivityPeriodToken {
    match interest
        .with_activity_for_test(ActivityQuerySource::new(id(30), f.turn), |q| q)
        .unwrap()
    {
        ActivitySourceQualification::Current { token, .. } => token,
        _ => panic!("current runtime token"),
    }
}

#[test]
fn concurrent_first_enrollment_shares_one_period_and_other_threads_reuse_it() {
    let f = Fixture::new();
    let mut owner = owner(&f);
    let first = acquire(&f, &owner);
    let second = acquire(&f, &owner);
    let before = f.home.home_revision().unwrap();
    std::thread::scope(|scope| {
        for interest in [&first, &second] {
            let f = &f;
            scope.spawn(move || {
                interest
                    .enroll_activity_for_test(&f.home, &f.syndic, id(30), f.turn)
                    .unwrap()
            });
        }
    });
    assert_eq!(f.home.home_revision().unwrap().get(), before.get() + 1);
    let period = token(&first, &f).work_period();
    assert_eq!(token(&second, &f).work_period(), period);
    let binding = f
        .syndic
        .thread_execution(&f.home, id(30), limit())
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let mut create = beryl_home_store::HomeCommand::new(f.home.home_revision().unwrap());
    create
        .add(f.syndic.create_thread(
            f.syndic.revision(&f.home).unwrap(),
            CreateThread::ordinary(
                id(88),
                draft_id(89),
                binding,
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        f.home.execute(create),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let turn = support::exact_cas::submit_current_draft(
        &f.home,
        f.syndic.clone(),
        id(88),
        draft_id(90),
        SyndicItemId::from_bytes([91; 16]),
        "other thread",
        timestamp(103),
    );
    second
        .enroll_activity_for_test(&f.home, &f.syndic, id(88), turn)
        .unwrap();
    assert_eq!(
        f.syndic
            .activity_query_head(&f.home, id(88), limit())
            .unwrap()
            .unwrap()
            .work_period(),
        period
    );
    assert_eq!(
        f.syndic
            .activity_query_head(&f.home, id(30), limit())
            .unwrap()
            .unwrap()
            .work_period(),
        period
    );
    assert!(owner.shutdown());
    assert!(matches!(
        first.with_activity_for_test(ActivityQuerySource::new(id(30), f.turn), |q| q),
        Some(ActivitySourceQualification::Retired(_))
    ));
    assert!(
        first
            .enroll_activity_for_test(&f.home, &f.syndic, id(30), f.turn)
            .is_err()
    );
    f.home
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn undispatched_replacement_never_inherits_ended_attempt_period() {
    for uncertain in [false, true] {
        let f = Fixture::new();
        let mut old_owner = owner(&f);
        let old = acquire(&f, &old_owner);
        if uncertain {
            f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        }
        let result = old.enroll_activity_for_test(&f.home, &f.syndic, id(30), f.turn);
        assert_eq!(result.is_err(), uncertain);
        assert_eq!(f.operations.pending_count(), usize::from(uncertain));
        let previous = f
            .syndic
            .activity_query_head(&f.home, id(30), limit())
            .unwrap()
            .unwrap()
            .work_period();
        let pending = f
            .syndic
            .pending_dispatch_evidence(&f.home, id(30), limit())
            .unwrap()
            .unwrap();
        assert!(old_owner.shutdown());
        let mut replacement = owner(&f);
        let fresh = acquire(&f, &replacement);
        fresh
            .enroll_activity_for_test(&f.home, &f.syndic, id(30), f.turn)
            .unwrap();
        assert!(token(&fresh, &f).work_period() > previous);
        assert_eq!(f.operations.pending_count(), 0);
        let after = f
            .syndic
            .pending_dispatch_evidence(&f.home, id(30), limit())
            .unwrap()
            .unwrap();
        assert_eq!(after.turn_id(), pending.turn_id());
        assert_eq!(after.dispatch_provenance(), pending.dispatch_provenance());
        assert_eq!(after.input(), pending.input());
        assert!(!matches!(
            old.with_activity_for_test(ActivityQuerySource::new(id(30), f.turn), |q| q),
            Some(ActivitySourceQualification::Current { .. })
        ));
        assert!(replacement.shutdown());
        f.home
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
    }
}

#[test]
fn original_attempt_settles_uncertainty_before_publishing_its_token() {
    let f = Fixture::new();
    let mut owner = owner(&f);
    let interest = acquire(&f, &owner);
    f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(
        interest
            .enroll_activity_for_test(&f.home, &f.syndic, id(30), f.turn)
            .is_err()
    );
    assert!(
        interest
            .with_activity_for_test(ActivityQuerySource::new(id(30), f.turn), |q| q)
            .is_none()
    );
    let head = f
        .syndic
        .activity_query_head(&f.home, id(30), limit())
        .unwrap()
        .unwrap();
    interest
        .enroll_activity_for_test(&f.home, &f.syndic, id(30), f.turn)
        .unwrap();
    assert_eq!(token(&interest, &f).work_period(), head.work_period());
    assert_eq!(f.operations.pending_count(), 0);
    assert!(owner.shutdown());
}

#[test]
fn successive_turns_reuse_runtime_period_and_publish_under_its_lifetime_fence() {
    let f = Fixture::new();
    let mut owner = owner(&f);
    let interest = acquire(&f, &owner);
    interest
        .enroll_activity_for_test(&f.home, &f.syndic, id(30), f.turn)
        .unwrap();
    let period = token(&interest, &f).work_period();
    let state = f
        .syndic
        .turn_state(&f.home, f.turn, limit())
        .unwrap()
        .unwrap();
    let gate = f
        .syndic
        .input_gate(&f.home, id(30), limit())
        .unwrap()
        .unwrap();
    let event = LiveSourceEvent::new(
        id(30),
        f.turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(1).unwrap(),
        None,
        SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
            TurnIncompleteReason::AuthorityLost,
        )),
        timestamp(105),
    )
    .unwrap();
    let outcome = interest
        .with_activity_for_test(ActivityQuerySource::new(id(30), f.turn), |activity| {
            f.home
                .execute_current(f.syndic.current_admit_live_source_event(event, activity))
        })
        .unwrap();
    assert!(matches!(
        outcome,
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    support::converge_and_release_terminal_history(&f.home, f.syndic.clone(), id(30), f.turn);
    let next = support::exact_cas::submit_current_draft(
        &f.home,
        f.syndic.clone(),
        id(30),
        draft_id(224),
        SyndicItemId::from_bytes([225; 16]),
        "later",
        timestamp(110),
    );
    interest
        .enroll_activity_for_test(&f.home, &f.syndic, id(30), next)
        .unwrap();
    let head = f
        .syndic
        .activity_query_head(&f.home, id(30), limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.work_period(), period);
    assert_eq!(head.source_count(), 2);
    assert!(owner.shutdown());
    f.home
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn definitive_commit_keeps_its_witness_when_confirmation_races_an_unrelated_write() {
    let f = Fixture::new();
    let mut owner = owner(&f);
    let interest = acquire(&f, &owner);
    let execution = f
        .syndic
        .thread_execution(&f.home, id(30), limit())
        .unwrap()
        .unwrap();
    let committed = f.faults.block_next(FaultPoint::AfterCommitBeforePersist);
    std::thread::scope(|scope| {
        let task =
            scope.spawn(|| interest.enroll_activity_for_test(&f.home, &f.syndic, id(30), f.turn));
        assert!(committed.wait_until_reached(Duration::from_secs(10)));
        let confirmation = f.faults.block_next(FaultPoint::BeforeReadConfirmation);
        committed.release();
        assert!(confirmation.wait_until_reached(Duration::from_secs(10)));
        commit(
            &f.home,
            f.syndic.clone(),
            batch([FixtureRecord::ThreadExecution(execution)]),
        );
        confirmation.release();
        assert!(task.join().unwrap().is_err());
    });
    let head = f
        .syndic
        .activity_query_head(&f.home, id(30), limit())
        .unwrap()
        .unwrap();
    interest
        .enroll_activity_for_test(&f.home, &f.syndic, id(30), f.turn)
        .unwrap();
    assert_eq!(token(&interest, &f).work_period(), head.work_period());
    assert_eq!(f.operations.pending_count(), 0);
    assert!(owner.shutdown());
}
