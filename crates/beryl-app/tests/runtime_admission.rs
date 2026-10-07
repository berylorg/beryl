#![cfg(all(feature = "test-faults", target_os = "windows"))]

#[path = "runtime_admission/mod.rs"]
mod support;

use support::*;

#[test]
fn both_runtime_forms_qualify_and_register_without_rebinding_a_duplicate() {
    use beryl_model::RuntimeLaunchForm;

    for (selected_form, other_form) in [
        (
            RuntimeLaunchForm::StandaloneAppServer,
            RuntimeLaunchForm::CodexCli,
        ),
        (
            RuntimeLaunchForm::CodexCli,
            RuntimeLaunchForm::StandaloneAppServer,
        ),
    ] {
        let fixture = Fixture::new();
        let admission = committed(fixture.add_runtime_with_form(EXECUTABLE, selected_form));
        let runtime_id = admission.facts().runtime_id();
        fixture.assert_closure(&admission);
        drop(admission);
        let before = fixture.store.home_revision().unwrap();
        assert!(matches!(
            fixture.add_runtime_with_form(ALIAS, other_form),
            RuntimeAdmissionOutcome::Existing { runtime_id: found, root_id: None }
                if found == runtime_id
        ));
        let runtime = fixture
            .state
            .runtime_roots()
            .runtime(&fixture.store, runtime_id)
            .unwrap()
            .unwrap();
        assert_eq!(runtime.launch_form(), selected_form);
        assert_eq!(
            *fixture.backend.launch_forms.lock().unwrap(),
            vec![selected_form]
        );
        assert_eq!(fixture.store.home_revision().unwrap(), before);
        fixture.assert_selection_released();
    }
}

#[test]
fn first_runtime_publishes_complete_closure_in_one_revision_and_existing_window() {
    let fixture = Fixture::new();
    let before = fixture.store.home_revision().unwrap();
    let admission = committed(fixture.add_runtime(EXECUTABLE));
    assert_eq!(
        fixture.store.home_revision().unwrap().get(),
        before.get() + 1
    );
    fixture.assert_closure(&admission);
    fixture.assert_selection_held();
    assert!(admission.validate_publication().is_ok());
    assert!(
        fixture
            .process
            .test_admit_selection(&[WINDOW], WINDOW)
            .is_err()
    );
    assert!(
        fixture
            .process
            .reserve_main_window(beryl_model::WindowId::from_bytes([43; 16]))
            .is_err()
    );
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
    drop(admission);
    fixture.assert_selection_released();
}

#[test]
fn later_runtime_and_root_preserve_complete_session_and_selection() {
    let fixture = Fixture::new();
    let first = committed(fixture.add_runtime(EXECUTABLE));
    let runtime = first.facts().runtime_id();
    drop(first);
    let original = fixture.session();
    let later = committed(fixture.add_runtime(r"C:\other\codex.exe"));
    assert!(later.facts().onboarding().is_none());
    assert_eq!(fixture.session(), original);
    drop(later);
    let root = committed(fixture.add_root(runtime, r"C:\work"));
    assert!(root.facts().onboarding().is_none());
    assert_eq!(fixture.session(), original);
    drop(root);
    assert_eq!(fixture.backend.counts(), (2, 2, 2, 2, 2));
}

#[test]
fn canonical_duplicates_bypass_backend_and_home_command() {
    let fixture = Fixture::new();
    let first = committed(fixture.add_runtime(EXECUTABLE));
    let runtime = first.facts().runtime_id();
    let root = first.facts().root_id();
    drop(first);
    let before = fixture.store.home_revision().unwrap();
    assert!(
        matches!(fixture.add_runtime(ALIAS), RuntimeAdmissionOutcome::Existing {
        runtime_id, root_id: None,
    } if runtime_id == runtime)
    );
    assert!(
        matches!(fixture.add_root(runtime, HOME), RuntimeAdmissionOutcome::Existing {
        runtime_id, root_id: Some(root_id),
    } if runtime_id == runtime && root_id == root)
    );
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
    fixture.assert_selection_released();
}

#[test]
fn cancellation_foreign_and_stale_window_sources_publish_nothing() {
    let fixture = Fixture::new();
    let before = fixture.store.home_revision().unwrap();
    let cancel = CommandCancellation::new();
    cancel.cancel();
    assert_refused(fixture.service.add_runtime(
        fixture.source(),
        &[WINDOW],
        Path::new(EXECUTABLE),
        beryl_model::RuntimeLaunchForm::CodexCli,
        cancel,
    ));
    let foreign = Fixture::new();
    assert_refused(fixture.service.add_runtime(
        foreign.source(),
        &[WINDOW],
        Path::new(EXECUTABLE),
        beryl_model::RuntimeLaunchForm::CodexCli,
        CommandCancellation::new(),
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    let stale = fixture.source();
    fixture.change_placement();
    let after_change = fixture.store.home_revision().unwrap();
    assert_refused(fixture.service.add_runtime(
        stale,
        &[WINDOW],
        Path::new(EXECUTABLE),
        beryl_model::RuntimeLaunchForm::CodexCli,
        CommandCancellation::new(),
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), after_change);
    fixture.assert_empty();
    assert_eq!(fixture.backend.counts(), (0, 0, 0, 0, 0));
    fixture.assert_selection_released();
}

#[test]
fn nonfile_and_rejected_profile_dispose_without_partial_rows() {
    for failure in [
        ValidationIssue::ExecutableUnavailable,
        ValidationIssue::ReleaseRejected,
    ] {
        let fixture = Fixture::new();
        let before = fixture.store.home_revision().unwrap();
        if failure == ValidationIssue::ExecutableUnavailable {
            *fixture.filesystem.failure.lock().unwrap() = Some(failure);
        } else {
            *fixture.backend.failure.lock().unwrap() = Some(failure);
        }
        assert_refused(fixture.add_runtime(EXECUTABLE));
        assert_eq!(fixture.store.home_revision().unwrap(), before);
        fixture.assert_empty();
        fixture.assert_selection_released();
        let expected = if failure == ValidationIssue::ExecutableUnavailable {
            (0, 0, 0, 0, 0)
        } else {
            (1, 1, 1, 1, 1)
        };
        assert_eq!(fixture.backend.counts(), expected);
    }
}

#[test]
fn root_cross_environment_refusal_preserves_session_and_registry() {
    let fixture = Fixture::new();
    let first = committed(fixture.add_runtime(EXECUTABLE));
    let runtime = first.facts().runtime_id();
    drop(first);
    let before = fixture.store.home_revision().unwrap();
    let session = fixture.session();
    assert_refused(fixture.add_root(runtime, r"\\wsl.localhost\Ubuntu\work"));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    assert_eq!(fixture.session(), session);
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
}

#[test]
fn command_noncommit_releases_selection_without_partial_closure() {
    let fixture = Fixture::new();
    let before = fixture.store.home_revision().unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeCommit);
    assert_refused(fixture.add_runtime(EXECUTABLE));
    fixture.assert_selection_released();
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
    let fixture = fixture.recover_generation();
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    fixture.assert_empty();
}

#[test]
fn indeterminate_pending_retains_original_custody_until_exact_new() {
    let fixture = Fixture::new();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let reconciliation = indeterminate(fixture.add_runtime(EXECUTABLE));
    fixture.assert_selection_held();
    let original = fixture.store.pending_reconciliations();
    assert_eq!(original.len(), 1);
    fixture
        .faults
        .fail_next(FaultPoint::BeforeReconciliationSnapshot);
    let AdmissionReconciliationOutcome::Pending { reconciliation, .. } =
        reconciliation.reconcile(&fixture.store)
    else {
        panic!("injected reconciliation read must remain pending");
    };
    assert_eq!(
        fixture.store.pending_reconciliations().len(),
        original.len()
    );
    fixture.assert_selection_held();
    assert!(fixture.store.reconcile(&original[0]).is_err());
    let AdmissionReconciliationOutcome::Committed { admission, .. } =
        reconciliation.retry(&fixture.store)
    else {
        panic!("exact new closure must reconcile as committed");
    };
    fixture.assert_closure(&admission);
    fixture.assert_selection_held();
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
    drop(admission);
    fixture.assert_selection_released();
}

#[test]
fn failed_journal_admission_reconciles_exact_old_without_resubmission() {
    let fixture = Fixture::new();
    let before = fixture.store.home_revision().unwrap();
    let fault = beryl_home_store::test_faults::fail_next_journal_write();
    let reconciliation = indeterminate(fixture.add_runtime(EXECUTABLE));
    drop(fault);
    fixture.assert_selection_held();
    let fixture = fixture.recover_exact_old(reconciliation);
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    fixture.assert_empty();
    fixture.assert_selection_released();
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
}

#[test]
fn changed_committed_window_is_terminal_unavailable_and_keeps_original_evidence() {
    let fixture = Fixture::new();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let reconciliation = indeterminate(fixture.add_runtime(EXECUTABLE));
    let original = fixture.store.pending_reconciliations();
    fixture.change_placement();
    let AdmissionReconciliationOutcome::Unavailable { custody } =
        reconciliation.reconcile(&fixture.store)
    else {
        panic!("mixed or successor window evidence cannot publish exact admission");
    };
    assert_eq!(custody.facts().window_id(), WINDOW);
    assert_eq!(
        fixture.store.pending_reconciliations().len(),
        original.len()
    );
    assert!(matches!(
        fixture.store.reconcile(&original[0]).unwrap(),
        beryl_home_store::ReconciliationResolution::Collision
            | beryl_home_store::ReconciliationResolution::ExactSuccessor { .. }
    ));
    fixture.assert_selection_held();
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
    drop(custody);
    fixture.assert_selection_released();
    assert_eq!(
        fixture.store.pending_reconciliations().len(),
        original.len()
    );
}

#[test]
fn retirement_refuses_new_work_without_backend_launch() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let before = fixture.store.home_revision().unwrap();
    fixture.service.retire().unwrap();
    assert_refused(fixture.service.add_runtime(
        source,
        &[WINDOW],
        Path::new(EXECUTABLE),
        beryl_model::RuntimeLaunchForm::CodexCli,
        CommandCancellation::new(),
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    assert_eq!(fixture.backend.counts(), (0, 0, 0, 0, 0));
    fixture.assert_selection_released();
}

#[test]
fn retirement_refuses_committed_publication_and_keeps_durable_facts_and_lease_custody() {
    let fixture = Fixture::new();
    let admission = committed(fixture.add_runtime(EXECUTABLE));
    let committed_revision = fixture.store.home_revision().unwrap();
    assert!(admission.validate_publication().is_ok());
    fixture.service.retire().unwrap();
    assert!(admission.validate_publication().is_err());
    assert_eq!(fixture.store.home_revision().unwrap(), committed_revision);
    fixture.assert_closure(&admission);
    fixture.assert_selection_held();
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
    drop(admission);
    fixture.assert_selection_released();
    assert_eq!(fixture.store.home_revision().unwrap(), committed_revision);
}

#[test]
fn fresh_same_home_generation_rejects_old_source_before_validation() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let home = fixture.store.home_id();
    let fixture = fixture.recover_generation();
    assert_eq!(fixture.store.home_id(), home);
    let before = fixture.store.home_revision().unwrap();
    assert_refused(fixture.service.add_runtime(
        source,
        &[WINDOW],
        Path::new(EXECUTABLE),
        beryl_model::RuntimeLaunchForm::CodexCli,
        CommandCancellation::new(),
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    fixture.assert_empty();
    fixture.assert_selection_released();
    assert_eq!(fixture.backend.counts(), (0, 0, 0, 0, 0));
}

#[test]
fn failed_cleanup_retains_service_custody_and_selection_until_retirement_joins() {
    let fixture = Fixture::new();
    fixture
        .backend
        .cleanup_failures
        .store(1, std::sync::atomic::Ordering::SeqCst);
    let before = fixture.store.home_revision().unwrap();
    let RuntimeAdmissionOutcome::NotCommitted {
        error: beryl_app::runtime_admission::AdmissionError::Cleanup(cleanup),
    } = fixture.add_runtime(EXECUTABLE)
    else {
        panic!("failed cleanup must retain qualification resource custody");
    };
    fixture.assert_selection_held();
    assert_eq!(cleanup.issue(), ValidationIssue::Cleanup);
    drop(cleanup);
    fixture.assert_selection_held();
    assert_refused(fixture.add_runtime(EXECUTABLE));
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 0));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    fixture.assert_empty();
    fixture.service.retire().unwrap();
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
    fixture.assert_selection_released();
    assert_refused(fixture.add_runtime(EXECUTABLE));
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
}

#[test]
fn active_validation_excludes_duplicates_and_cancellation_creates_no_rows() {
    let fixture = Fixture::new();
    let before = fixture.store.home_revision().unwrap();
    let cancellation = CommandCancellation::new();
    let (entered, release) = fixture.filesystem.pause_executable();
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            fixture.service.add_runtime(
                fixture.source(),
                &[WINDOW],
                Path::new(EXECUTABLE),
                beryl_model::RuntimeLaunchForm::CodexCli,
                cancellation.clone(),
            )
        });
        entered
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        fixture.assert_selection_held();
        assert_refused(fixture.add_runtime(ALIAS));
        assert_eq!(fixture.backend.counts(), (0, 0, 0, 0, 0));
        cancellation.cancel();
        release.send(()).unwrap();
        assert_refused(worker.join().unwrap());
    });
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    fixture.assert_empty();
    fixture.assert_selection_released();
    assert_eq!(fixture.backend.counts(), (0, 0, 0, 0, 0));
}

#[test]
fn retirement_cancels_and_joins_original_active_validation_before_completing() {
    let fixture = Fixture::new();
    let before = fixture.store.home_revision().unwrap();
    let cancellation = CommandCancellation::new();
    let (entered, release) = fixture.filesystem.pause_executable();
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            fixture.service.add_runtime(
                fixture.source(),
                &[WINDOW],
                Path::new(EXECUTABLE),
                beryl_model::RuntimeLaunchForm::CodexCli,
                cancellation.clone(),
            )
        });
        entered
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let retirement = scope.spawn(|| fixture.service.retire());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !cancellation.is_cancelled() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(!retirement.is_finished());
        fixture.assert_selection_held();
        assert_refused(fixture.add_runtime(ALIAS));
        release.send(()).unwrap();
        assert_refused(worker.join().unwrap());
        retirement.join().unwrap().unwrap();
    });
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    fixture.assert_empty();
    fixture.assert_selection_released();
    assert_eq!(fixture.backend.counts(), (0, 0, 0, 0, 0));
    assert_refused(fixture.add_runtime(EXECUTABLE));
}

#[test]
fn repeated_cleanup_failure_preserves_original_probe_and_selection_until_same_owner_joins() {
    let fixture = Fixture::new();
    fixture
        .backend
        .cleanup_failures
        .store(3, std::sync::atomic::Ordering::SeqCst);
    let before = fixture.store.home_revision().unwrap();
    let RuntimeAdmissionOutcome::NotCommitted {
        error: beryl_app::runtime_admission::AdmissionError::Cleanup(mut cleanup),
    } = fixture.add_runtime(EXECUTABLE)
    else {
        panic!("failed cleanup must retain the original qualification resource");
    };
    assert_eq!(cleanup.dispose_cleanup(), Err(ValidationIssue::Cleanup));
    fixture.assert_selection_held();
    assert_refused(fixture.add_runtime(ALIAS));
    drop(cleanup);
    assert_eq!(fixture.service.retire(), Err(ValidationIssue::Cleanup));
    fixture.assert_selection_held();
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 0));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    fixture.assert_empty();
    fixture.service.retire().unwrap();
    fixture.assert_selection_released();
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
    assert_refused(fixture.add_runtime(ALIAS));
}

#[test]
fn retirement_preserves_exact_indeterminate_intent_and_original_reconciliation_owner() {
    let fixture = Fixture::new();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let reconciliation = indeterminate(fixture.add_runtime(EXECUTABLE));
    let original = fixture.store.pending_reconciliations();
    fixture.service.retire().unwrap();
    fixture.assert_selection_held();
    assert_refused(fixture.add_runtime(ALIAS));
    let AdmissionReconciliationOutcome::Committed { admission, receipt } =
        reconciliation.retry(&fixture.store)
    else {
        panic!("retirement must retain the exact command for reconciliation");
    };
    fixture.assert_closure(&admission);
    assert!(admission.validate_publication().is_err());
    assert!(fixture.store.pending_reconciliations().is_empty());
    let committed_revision = fixture.store.home_revision().unwrap();
    assert_eq!(
        fixture.store.reconcile(&original[0]).unwrap(),
        beryl_home_store::ReconciliationResolution::ExactNew { receipt }
    );
    assert_eq!(fixture.store.home_revision().unwrap(), committed_revision);
    fixture.assert_selection_held();
    drop(admission);
    fixture.assert_selection_released();
    assert_eq!(fixture.backend.counts(), (1, 1, 1, 1, 1));
}

#[test]
fn unavailable_home_root_publishes_no_registry_catalog_or_session_rows() {
    let fixture = Fixture::new();
    let before = fixture.store.home_revision().unwrap();
    *fixture.filesystem.home_failure.lock().unwrap() = Some(ValidationIssue::HomeUnavailable);
    assert_refused(fixture.add_runtime(EXECUTABLE));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    fixture.assert_empty();
    fixture.assert_selection_released();
    assert_eq!(fixture.backend.counts(), (0, 0, 0, 0, 0));
}

#[test]
fn committed_publication_rejects_lost_original_window_reservation() {
    let mut fixture = Fixture::new();
    let admission = committed(fixture.add_runtime(EXECUTABLE));
    assert!(admission.validate_publication().is_ok());
    fixture.release_window_reservation();
    assert!(admission.validate_publication().is_err());
    fixture.assert_closure(&admission);
    drop(admission);
}

#[test]
fn committed_publication_rejects_original_process_gate_retirement() {
    let fixture = Fixture::new();
    let admission = committed(fixture.add_runtime(EXECUTABLE));
    assert!(admission.validate_publication().is_ok());
    let fence = fixture.process_gate.test_fence().unwrap();
    assert!(admission.validate_publication().is_err());
    assert!(fence.try_reopen(true).is_err());
    fixture.assert_closure(&admission);
    drop(admission);
    fence.try_reopen(true).unwrap();
    fixture.assert_selection_released();
}

#[test]
fn proven_original_cleanup_completion_allows_a_new_validation_without_reusing_failure() {
    let fixture = Fixture::new();
    fixture
        .backend
        .cleanup_failures
        .store(1, std::sync::atomic::Ordering::SeqCst);
    let RuntimeAdmissionOutcome::NotCommitted {
        error: beryl_app::runtime_admission::AdmissionError::Cleanup(mut cleanup),
    } = fixture.add_runtime(EXECUTABLE)
    else {
        panic!("first failed cleanup must retain the original candidate");
    };
    fixture.assert_selection_held();
    cleanup.dispose_cleanup().unwrap();
    fixture.assert_selection_released();
    fixture.assert_empty();
    let admission = committed(fixture.add_runtime(EXECUTABLE));
    fixture.assert_closure(&admission);
    assert_eq!(fixture.backend.counts(), (2, 2, 2, 2, 2));
    drop(admission);
    fixture.assert_selection_released();
}
