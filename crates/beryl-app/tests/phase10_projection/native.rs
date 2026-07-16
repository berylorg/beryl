use super::*;

#[test]
fn native_resume_uses_the_exact_existing_cas_thread() {
    let mut fixture = Fixture::new(2);
    let first = fixture.submit_text("first");
    fixture.complete_with_assistant(first, "answer");
    fixture.submit_text("second pending");
    let source = fixture.native_source(fixture.thread);
    let source_id = source.binding().cas_thread_id().clone();
    let server = FakeAppServer::spawn(vec![ProjectionStep::Resume {
        source: source_id.as_str().to_string(),
    }]);
    let mut session = server.admit(execution_binding().runtime_id(), process(2));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();

    let projection = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request(&fixture, fixture.thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(projection.cas_thread_id(), &source_id);
    assert!(matches!(
        projection.lineage_proof(),
        CasLineageProof::Native {
            mechanism: NativeCasLineage::Resume,
            ..
        }
    ));
    server.join();
}

#[test]
fn target_execution_mismatch_retires_only_target_source_before_recovery() {
    let mut fixture = Fixture::new(24);
    let first = fixture.submit_text("history user");
    fixture.complete_with_assistant(first, "history assistant");
    fixture.submit_text("pending on another root identity");
    let source = fixture.native_source(fixture.thread);
    let source_id = source.binding().cas_thread_id().clone();
    let execution = alternate_root_binding();
    let request = CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution.clone(),
        ThreadStartOptions::persistent(),
        Some(1_000_000),
        SyndicTimestamp::from_unix_millis(10_000),
        TIMEOUT,
    );
    let server = FakeAppServer::spawn(vec![ProjectionStep::Recover {
        target: "phase10-execution-mismatch-recovery",
        injection: InjectionReply::Success,
    }]);
    let mut session = server.admit(execution.runtime_id(), process(24));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();

    let projection = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();

    assert_eq!(projection.execution_binding(), &execution);
    assert_eq!(
        projection.cas_thread_id().as_str(),
        "phase10-execution-mismatch-recovery"
    );
    assert_ne!(projection.cas_thread_id(), &source_id);
    assert!(matches!(
        projection.lineage_proof(),
        CasLineageProof::RecoveredInjection(_)
    ));
    let retired = fixture
        .storage
        .cas_thread_owner(&fixture.store, source_id, point_limit())
        .unwrap()
        .unwrap();
    assert!(retired.record().retired_binding_revision().is_some());
    server.join();
}

#[test]
fn unclassified_resume_exhaustion_preserves_binding_and_explicit_retry_reuses_it() {
    let mut fixture = Fixture::new(20);
    let first = fixture.submit_text("first");
    fixture.complete_with_assistant(first, "answer");
    fixture.submit_text("second pending");
    let source = fixture.native_source(fixture.thread);
    let source_id = source.binding().cas_thread_id().clone();
    let source_revision = source.binding_revision();
    let binding_before = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let rejected = || ProjectionStep::RejectResume {
        source: source_id.as_str().to_string(),
    };
    let server = FakeAppServer::spawn(vec![
        rejected(),
        rejected(),
        rejected(),
        ProjectionStep::Resume {
            source: source_id.as_str().to_string(),
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(20));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection_request = request(&fixture, fixture.thread);

    let error = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &projection_request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap_err();
    let decision = error
        .into_native_lineage_recovery_decision()
        .expect("retry exhaustion must return an exact recovery decision");
    assert_eq!(decision.operation(), NativeLineageOperation::Resume);
    assert_eq!(decision.failed_attempts(), 3);
    assert_eq!(decision.target_thread_id(), fixture.thread);
    assert_eq!(decision.source_thread_id(), fixture.thread);
    assert_eq!(decision.source_binding_revision(), source_revision);

    let preserved = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(preserved, binding_before);

    let projection = coordinator
        .retry_native_lineage(
            &fixture.store,
            fixture.storage,
            &mut session,
            decision,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(projection.cas_thread_id(), &source_id);
    assert!(matches!(
        projection.lineage_proof(),
        CasLineageProof::Native {
            mechanism: NativeCasLineage::Resume,
            ..
        }
    ));
    server.join();
}

#[test]
fn recovery_decision_rejects_a_later_binding_revision_without_backend_work() {
    let mut fixture = Fixture::new(23);
    let first = fixture.submit_text("first");
    fixture.complete_with_assistant(first, "answer");
    fixture.submit_text("second pending");
    let source = fixture.native_source(fixture.thread);
    let source_id = source.binding().cas_thread_id().clone();
    let rejected = || ProjectionStep::RejectResume {
        source: source_id.as_str().to_string(),
    };
    let server = FakeAppServer::spawn(vec![rejected(), rejected(), rejected()]);
    let mut session = server.admit(execution_binding().runtime_id(), process(23));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection_request = request(&fixture, fixture.thread);
    let decision = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &projection_request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap_err()
        .into_native_lineage_recovery_decision()
        .unwrap();

    fixture.retire_current_binding(fixture.thread);
    let error = coordinator
        .retry_native_lineage(
            &fixture.store,
            fixture.storage,
            &mut session,
            decision,
            &ProjectionCancellationToken::new(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ProjectionExecutionError::NativeLineageRecoveryDecisionStale { thread_id }
            if thread_id == fixture.thread
    ));
    server.join();
}

#[test]
fn explicit_resume_recovery_retires_target_binding_and_injects_once() {
    let mut fixture = Fixture::new(21);
    let first = fixture.submit_text("history user");
    fixture.complete_with_assistant(first, "history assistant");
    fixture.submit_text("pending user");
    let source = fixture.native_source(fixture.thread);
    let source_id = source.binding().cas_thread_id().clone();
    let target_revision = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
        .binding()
        .revision();
    let rejected = || ProjectionStep::RejectResume {
        source: source_id.as_str().to_string(),
    };
    let server = FakeAppServer::spawn(vec![
        rejected(),
        rejected(),
        rejected(),
        ProjectionStep::Recover {
            target: "phase10-explicit-resume-recovery",
            injection: InjectionReply::Success,
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(21));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection_request = request(&fixture, fixture.thread);
    let decision = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &projection_request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap_err()
        .into_native_lineage_recovery_decision()
        .unwrap();
    coordinator
        .validate_native_lineage_recovery(
            &fixture.store,
            fixture.storage,
            &session,
            &decision,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();

    let projection = coordinator
        .recover_native_lineage_from_syndic(
            &fixture.store,
            fixture.storage,
            &mut session,
            decision,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(
        projection.cas_thread_id().as_str(),
        "phase10-explicit-resume-recovery"
    );
    assert_ne!(projection.cas_thread_id(), &source_id);
    let CasLineageProof::RecoveredInjection(proof) = projection.lineage_proof() else {
        panic!("explicit recovery must retain recovered-injection provenance")
    };
    assert!(proof.completed_at() > SyndicTimestamp::from_unix_millis(10_000));
    assert_eq!(
        projection.binding_revision(),
        target_revision
            .checked_next()
            .unwrap()
            .checked_next()
            .unwrap()
    );
    server.join();
}

#[test]
fn divergent_nonempty_branch_uses_inclusive_fork_and_a_distinct_target() {
    let mut fixture = Fixture::new(3);
    let first = fixture.submit_text("shared first");
    fixture.complete_with_assistant(first, "shared answer");
    let child = fixture.create_child_pending("child pending");
    let source_advance = fixture.submit_text("source advances");
    fixture.complete_with_assistant(source_advance, "source answer");
    let current = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(source) = current.binding().state() else {
        panic!("advanced source must remain valid")
    };
    let source_id = source.cas_thread_id().clone();
    let server = FakeAppServer::spawn(vec![ProjectionStep::Fork {
        source: source_id.as_str().to_string(),
        through_turn: Some(format!("phase10-turn-{}", first.turn)),
        target: "phase10-fork-target",
    }]);
    let mut session = server.admit(execution_binding().runtime_id(), process(3));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();

    let projection = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request(&fixture, child),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(projection.cas_thread_id().as_str(), "phase10-fork-target");
    assert_ne!(projection.cas_thread_id(), &source_id);
    assert!(matches!(
        projection.lineage_proof(),
        CasLineageProof::Native {
            mechanism: NativeCasLineage::Fork,
            ..
        }
    ));
    server.join();
}

#[test]
fn explicit_child_recovery_preserves_the_parent_fork_source() {
    let mut fixture = Fixture::new(22);
    let first = fixture.submit_text("history user");
    fixture.complete_with_assistant(first, "history assistant");
    let child = fixture.create_child_pending("child pending");
    let parent_advance = fixture.submit_text("parent advances");
    fixture.complete_with_assistant(parent_advance, "parent answer");
    let parent_before = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let parent_revision = parent_before.binding().revision();
    let BindingState::Valid(parent_source) = parent_before.binding().state() else {
        panic!("parent source must be valid")
    };
    let parent_cas_thread = parent_source.cas_thread_id().clone();
    let rejected = || ProjectionStep::RejectFork {
        source: parent_cas_thread.as_str().to_string(),
        through_turn: Some(format!("phase10-turn-{}", first.turn)),
    };
    let server = FakeAppServer::spawn(vec![
        rejected(),
        rejected(),
        rejected(),
        ProjectionStep::Recover {
            target: "phase10-explicit-child-recovery",
            injection: InjectionReply::Success,
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(22));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection_request = request(&fixture, child);
    let decision = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &projection_request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap_err()
        .into_native_lineage_recovery_decision()
        .unwrap();
    assert_eq!(decision.operation(), NativeLineageOperation::Fork);
    assert_eq!(decision.target_thread_id(), child);
    assert_eq!(decision.source_thread_id(), fixture.thread);

    let child_projection = coordinator
        .recover_native_lineage_from_syndic(
            &fixture.store,
            fixture.storage,
            &mut session,
            decision,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(
        child_projection.cas_thread_id().as_str(),
        "phase10-explicit-child-recovery"
    );
    assert!(matches!(
        child_projection.lineage_proof(),
        CasLineageProof::RecoveredInjection(_)
    ));

    let parent_after = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(parent_after.binding().revision(), parent_revision);
    let BindingState::Valid(parent_after) = parent_after.binding().state() else {
        panic!("child recovery must not stale its parent source")
    };
    assert_eq!(parent_after.cas_thread_id(), &parent_cas_thread);
    server.join();
}
