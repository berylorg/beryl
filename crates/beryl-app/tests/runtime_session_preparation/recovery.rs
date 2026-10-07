use super::*;
use beryl_app::cas_projection::{
    ProjectionCancellationToken, RuntimeFailureSnapshot, RuntimeInterestKind,
};

#[test]
fn ordinary_and_same_home_recovery_preserve_both_explicit_launch_forms() {
    use beryl_model::RuntimeLaunchForm;

    for launch_form in [
        RuntimeLaunchForm::StandaloneAppServer,
        RuntimeLaunchForm::CodexCli,
    ] {
        let (mut fixture, sessions, _attention) =
            fixture_with_launch_form(10, std::num::NonZeroUsize::new(1).unwrap(), launch_form);
        let lease = checkout(&fixture, 1);
        let expected = format!("{launch_form:?}");
        let evidence: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.root(1).join("runtime-launch-form.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(evidence["launch_form"], expected);
        drop(lease);
        close(&mut fixture, &sessions);

        let (mut fixture, sessions, _attention) =
            fixture_with_launch_form(10, std::num::NonZeroUsize::new(1).unwrap(), launch_form);
        let (window, claim) = runtime_notice::select(&fixture);
        fs::write(fixture.root(1).join("fixture-mode"), "reject-config").unwrap();
        begin(&fixture, 1);
        wait_until(|| {
            sessions
                .runtime_failure(binding(&fixture, 1).runtime_id())
                .is_some_and(|failure| failure.retry_ready())
        });
        let failure = sessions
            .runtime_failure(binding(&fixture, 1).runtime_id())
            .unwrap();
        fs::remove_file(fixture.root(1).join("runtime-launch-form.json")).unwrap();
        fs::write(fixture.root(1).join("fixture-mode"), "projection-lifetime").unwrap();
        let proof = recover_with_idle_election_paused(
            &fixture,
            &sessions,
            window,
            claim,
            failure,
            &binding(&fixture, 1),
        );
        let evidence: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.root(1).join("runtime-launch-form.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(evidence["launch_form"], expected);
        let worker = fixture.service().selected_runtime_retry_worker(&sessions);
        assert!(worker.usability_current(&fixture.state.session(), &proof));
        close(&mut fixture, &sessions);
    }
}

fn failed_idle(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
    mode: &str,
) -> RuntimeFailureSnapshot {
    fs::write(fixture.root(1).join("fixture-mode"), "reject-config").unwrap();
    let interest = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    wait_until(|| {
        sessions
            .runtime_failure(binding(fixture, 1).runtime_id())
            .is_some_and(|failure| failure.retry_ready())
    });
    let failure = sessions
        .runtime_failure(binding(fixture, 1).runtime_id())
        .unwrap();
    drop(interest);
    fs::write(fixture.root(1).join("fixture-mode"), mode).unwrap();
    failure
}

pub(super) fn recover_with_idle_election_paused(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
    window: beryl_model::WindowId,
    claim: beryl_state::WindowClaimSelection,
    failure: RuntimeFailureSnapshot,
    execution: &ExecutionBinding,
) -> beryl_app::cas_projection::SelectedRuntimeUsability {
    let (proof, pause) =
        recover_before_idle_election(fixture, sessions, window, claim, failure, execution);
    let idle_passes = fixture
        .service()
        .accepted_input_scheduler_diagnostics()
        .idle_pass_count();
    pause.release();
    wait_until(|| {
        fixture
            .service()
            .accepted_input_scheduler_diagnostics()
            .idle_pass_count()
            > idle_passes
    });
    assert!(
        !sessions
            .retire_idle_thread_for_test(claim.thread_id())
            .unwrap()
    );
    let worker = fixture.service().selected_runtime_retry_worker(sessions);
    let session = fixture.state.session();
    wait_until(|| worker.usability_current(&session, &proof));
    proof
}

fn recover_before_idle_election(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
    window: beryl_model::WindowId,
    claim: beryl_state::WindowClaimSelection,
    failure: RuntimeFailureSnapshot,
    execution: &ExecutionBinding,
) -> (
    beryl_app::cas_projection::SelectedRuntimeUsability,
    beryl_app::cas_projection::IdleSessionElectionPause,
) {
    let pause = sessions.install_idle_election_pause_for_test(thread_id(1));
    let worker = fixture.service().selected_runtime_retry_worker(sessions);
    let session = fixture.state.session();
    let recovered = worker.recover(
        &session,
        window,
        claim,
        execution,
        failure,
        &ProjectionCancellationToken::new(),
    );
    let proof = match recovered {
        Ok(proof) => {
            pause.wait(TIMEOUT);
            proof
        }
        Err(beryl_app::cas_projection::SelectedRuntimeRetryError::Revoked) => {
            pause.wait(TIMEOUT);
            worker
                .recover(
                    &session,
                    window,
                    claim,
                    execution,
                    failure,
                    &ProjectionCancellationToken::new(),
                )
                .unwrap()
        }
        Err(error) => panic!("selected projection recovery failed: {error}"),
    };
    wait_until(|| worker.usability_current(&session, &proof));
    (proof, pause)
}

fn select_second(fixture: &Fixture) -> (beryl_model::WindowId, beryl_state::WindowClaimSelection) {
    use beryl_model::{WindowBounds, WindowDisplayState, WindowId, WindowPlacement};
    use beryl_state::{CreateClaimedWindow, RememberedTarget};
    let live = fixture.service().live_home_command().unwrap();
    let home = live.home();
    let session = fixture.state.session();
    let window = WindowId::from_bytes([202; 16]);
    let placement = WindowPlacement::new(
        WindowBounds::new(0, 0, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    );
    let bootstrap = session.minimal_bootstrap(home).unwrap().unwrap();
    let execution = binding(fixture, 2);
    notice_shell::commit(
        home,
        session.create_claimed_window(
            session.revision(home).unwrap(),
            CreateClaimedWindow::new(
                bootstrap.header().revision(),
                window,
                RememberedTarget::new(execution.runtime_id(), execution.root_id()),
                thread_id(2),
                placement,
            ),
        ),
    );
    let bootstrap = session.minimal_bootstrap(home).unwrap().unwrap();
    (
        window,
        bootstrap
            .windows()
            .iter()
            .find(|record| record.window_id() == window)
            .unwrap()
            .selected_thread()
            .unwrap(),
    )
}

#[test]
fn separate_selected_bindings_recover_on_one_retried_runtime_with_their_own_projection_proofs() {
    let (mut fixture, sessions, _attention) = fixture(10);
    let (first_window, first_claim) = runtime_notice::select(&fixture);
    let (second_window, second_claim) = select_second(&fixture);
    let failure = failed_idle(&fixture, &sessions, "projection-multiple-bindings");
    let worker = fixture.service().selected_runtime_retry_worker(&sessions);
    let session = fixture.state.session();
    let first = recover_with_idle_election_paused(
        &fixture,
        &sessions,
        first_window,
        first_claim,
        failure,
        &binding(&fixture, 1),
    );
    wait_until(|| worker.usability_current(&session, &first));
    let first_pid = fixture.evidence(1)["pid"].as_u64().unwrap();
    let second = recover_with_idle_election_paused(
        &fixture,
        &sessions,
        second_window,
        second_claim,
        failure,
        &binding(&fixture, 2),
    );
    wait_until(|| worker.usability_current(&session, &second));
    assert_eq!(fixture.evidence(1)["pid"].as_u64().unwrap(), first_pid);
    assert!(!fixture.root(2).join("runtime-evidence.json").exists());
    assert_eq!(sessions.diagnostics().available, 2);
    let refreshed_first = recover_with_idle_election_paused(
        &fixture,
        &sessions,
        first_window,
        first_claim,
        failure,
        &binding(&fixture, 1),
    );
    wait_until(|| worker.usability_current(&session, &refreshed_first));
    wait_until(|| worker.usability_current(&session, &second));
    close(&mut fixture, &sessions);
}

#[test]
fn selected_retry_capacity_refuses_without_session_or_input() {
    let (mut fixture, sessions, _attention) = fixture(4);
    let (window, claim) = runtime_notice::select(&fixture);
    let failure = failed_idle(&fixture, &sessions, "projection-lifetime");
    let worker = fixture.service().selected_runtime_retry_worker(&sessions);
    let session = fixture.state.session();
    assert!(!worker.eligible(&session, window, claim, &binding(&fixture, 1), failure));
    assert!(
        worker
            .recover(
                &session,
                window,
                claim,
                &binding(&fixture, 1),
                failure,
                &ProjectionCancellationToken::new()
            )
            .is_err()
    );
    assert_eq!(sessions.diagnostics().retained, 0);
    assert!(
        !fixture
            .root(1)
            .join("runtime-projection-evidence.json")
            .exists()
    );
    close(&mut fixture, &sessions);
}

#[test]
fn pending_selected_retry_refuses_duplicates_and_keeps_preparation_custody_until_response_settlement()
 {
    let (mut fixture, sessions, _attention) = fixture(8);
    let (window, claim) = runtime_notice::select(&fixture);
    let failure = failed_idle(&fixture, &sessions, "pause-projection");
    let worker = fixture.service().selected_runtime_retry_worker(&sessions);
    let session = fixture.state.session();
    let cancel = ProjectionCancellationToken::new();
    let recovering_worker = worker.clone();
    let recovering_session = session.clone();
    let recovering_cancel = cancel.clone();
    let execution = binding(&fixture, 1);
    let recovering_execution = execution.clone();
    let running = thread::spawn(move || {
        recovering_worker.recover(
            &recovering_session,
            window,
            claim,
            &recovering_execution,
            failure,
            &recovering_cancel,
        )
    });
    wait_until(|| {
        fixture
            .root(1)
            .join("runtime-projection-evidence.json")
            .exists()
    });
    assert!(!worker.eligible(&session, window, claim, &execution, failure));
    assert!(
        worker
            .recover(
                &session,
                window,
                claim,
                &execution,
                failure,
                &ProjectionCancellationToken::new()
            )
            .is_err()
    );
    let revision = sessions.work_revision().unwrap();
    let page = sessions
        .work_page(
            &revision,
            None,
            beryl_app::cas_projection::ScheduledSessionWorkPageLimits::new(16, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 1);
    assert!(!page.records()[0].preparation().unwrap().is_complete());
    cancel.cancel();
    assert!(!running.is_finished());
    fs::write(fixture.root(1).join("release-projection"), "ready").unwrap();
    assert!(running.join().unwrap().is_err());
    let revision = sessions.work_revision().unwrap();
    let page = sessions
        .work_page(
            &revision,
            None,
            beryl_app::cas_projection::ScheduledSessionWorkPageLimits::new(16, 65_536).unwrap(),
        )
        .unwrap();
    assert!(
        page.records()
            .iter()
            .all(|record| record.preparation().is_none())
    );
    assert_eq!(sessions.diagnostics().available, 0);
    close(&mut fixture, &sessions);
}

#[test]
fn repeated_selected_retry_reuses_the_retained_projection_and_checkout_revokes_its_proof() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let (window, claim) = runtime_notice::select(&fixture);
    let failure = failed_idle(&fixture, &sessions, "projection-lifetime");
    let worker = fixture.service().selected_runtime_retry_worker(&sessions);
    let session = fixture.state.session();
    let execution = binding(&fixture, 1);
    let first =
        recover_with_idle_election_paused(&fixture, &sessions, window, claim, failure, &execution);
    wait_until(|| worker.usability_current(&session, &first));
    let second =
        recover_with_idle_election_paused(&fixture, &sessions, window, claim, failure, &execution);
    wait_until(|| worker.usability_current(&session, &first));
    wait_until(|| worker.usability_current(&session, &second));
    assert_eq!(sessions.diagnostics().retained, 1);
    let checked_out = checkout(&fixture, 1);
    assert!(!worker.usability_current(&session, &second));
    drop(checked_out);
    assert!(!worker.usability_current(&session, &first));
    close(&mut fixture, &sessions);
}

#[test]
fn rejected_native_selected_retry_keeps_native_binding_and_never_starts_a_fallback_target() {
    use syndic_storage::{
        CasLineageProof, NativeCasLineage, NativeProjectionRecoveryPlan, NativeProjectionRequest,
        PublishValidBinding, SelectedPathProof, SyndicPointReadLimit,
    };
    let (mut fixture, sessions, _attention) = fixture(8);
    let (window, claim) = runtime_notice::select(&fixture);
    let live = fixture.service().live_home_command().unwrap();
    let home = live.home();
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let thread = fixture
        .storage
        .thread(home, thread_id(1), limit)
        .unwrap()
        .unwrap();
    let selected = SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    );
    let NativeProjectionRecoveryPlan::Ready { basis, .. } = fixture
        .storage
        .prepare_native_projection_recovery(
            home,
            &NativeProjectionRequest::new(
                thread_id(1),
                selected,
                binding(&fixture, 1),
                beryl_app::conversation_tools::ConversationToolRegistry::canonical().profile(),
            ),
            limit,
        )
        .unwrap()
    else {
        panic!("fresh idle source must be eligible");
    };
    let native = basis.native_basis();
    let publication = PublishValidBinding::from_native(
        native,
        binding(&fixture, 1),
        beryl_model::CasThreadId::new("selected-native-source").unwrap(),
        beryl_model::CasNativeTurnCount::new(0),
        CasLineageProof::native(NativeCasLineage::Fresh, native.represented_prefix()).unwrap(),
    );
    assert!(matches!(
        home.execute_current(fixture.storage.current_publish_valid_binding(publication)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let before = fixture
        .storage
        .current_binding(home, thread_id(1), limit)
        .unwrap();
    drop(live);
    let failure = failed_idle(&fixture, &sessions, "reject-native-recovery");
    let worker = fixture.service().selected_runtime_retry_worker(&sessions);
    let session = fixture.state.session();
    assert!(worker.eligible(&session, window, claim, &binding(&fixture, 1), failure));
    assert!(
        worker
            .recover(
                &session,
                window,
                claim,
                &binding(&fixture, 1),
                failure,
                &ProjectionCancellationToken::new()
            )
            .is_err()
    );
    let after = fixture
        .storage
        .current_binding(&fixture.home_reference, thread_id(1), limit)
        .unwrap();
    assert_eq!(before, after);
    let request: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.root(1).join("runtime-native-request-0.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(request["method"], "thread/resume");
    assert!(
        !fixture
            .root(1)
            .join("runtime-projection-evidence.json")
            .exists()
    );
    close(&mut fixture, &sessions);
}

#[test]
fn failed_retained_unsubscribe_preserves_input_and_independent_runtime_admission() {
    let (mut fixture, sessions, _attention) =
        fixture_with_runtime_capacity(20, std::num::NonZeroUsize::new(2).unwrap());
    fs::create_dir(fixture.root(3)).unwrap();
    fs::write(fixture.root(3).join("fixture-mode"), "projection-lifetime").unwrap();
    let root_path = canonical_path(&fixture.root(3));
    let runtime_id = RuntimeId::from_bytes([98; 16]);
    let healthy_binding =
        ExecutionBinding::new(runtime_id, RootId::from_bytes([3; 16]), native(&root_path));
    let executable_path = fixture.root(3).join("independent-runtime-fixture.exe");
    fs::copy(
        env!("CARGO_BIN_EXE_managed-runtime-fixture"),
        &executable_path,
    )
    .unwrap();
    let executable = canonical_path(&executable_path);
    let live = fixture.service().live_home_command().unwrap();
    let home = live.home();
    let runtime = RuntimeRegistration::new(
        runtime_id,
        host(&executable),
        RuntimeMode::Host,
        beryl_model::RuntimeLaunchForm::CodexCli,
        native(&executable),
        UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    )
    .unwrap();
    let root = RootRegistration::new(
        RootId::from_bytes([3; 16]),
        native(&root_path),
        host(&root_path),
        UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    );
    let mut runtime_command = HomeCommand::new(home.home_revision().unwrap());
    runtime_command
        .add(fixture.state.runtime_roots().create_runtime_with_home_root(
            fixture.state.runtime_roots().revision(home).unwrap(),
            CreateRuntimeWithHomeRoot::new(runtime, root).unwrap(),
        ))
        .unwrap();
    let runtime_outcome = home.execute(runtime_command);
    assert!(
        matches!(
            runtime_outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "independent runtime setup failed: {runtime_outcome:?}"
    );
    let mut thread_command = HomeCommand::new(home.home_revision().unwrap());
    thread_command
        .add(fixture.storage.create_thread(
            fixture.storage.revision(home).unwrap(),
            CreateThread::ordinary(
                thread_id(3),
                SyndicDraftId::from_bytes([113; 16]),
                healthy_binding.clone(),
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(64 * 1024 * 1024, 1).unwrap(),
            ),
        ))
        .unwrap();
    let thread_outcome = home.execute(thread_command);
    assert!(
        matches!(
            thread_outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "independent thread setup failed: {thread_outcome:?}"
    );
    drop(live);
    let tokens = canonical_path(&fixture.tokens());
    let healthy_interest = fixture
        .service()
        .acquire_runtime_interest(
            beryl_backend::ManagedBackendLaunchSpec::new(
                runtime_id,
                host(&executable),
                RuntimeMode::Host,
                beryl_model::RuntimeLaunchForm::CodexCli,
                native(&executable),
                native(&root_path),
                host(&tokens),
                native(&tokens),
            )
            .unwrap(),
            healthy_binding.clone(),
            RuntimeInterestKind::View,
        )
        .unwrap();
    support::ready(&healthy_interest);
    let mut healthy = checkout_binding(&fixture, thread_id(3), healthy_binding.clone());
    drop(healthy_interest);
    assert!(healthy.session_authority_current_for_test());
    let healthy_process = ProcessWitness::open(fixture.evidence(3)["pid"].as_u64().unwrap() as u32);
    assert!(healthy_process.running());
    let (window, claim) = runtime_notice::select(&fixture);
    let failure = failed_idle(&fixture, &sessions, "projection-reject-unsubscribe");
    let execution = binding(&fixture, 1);
    let (proof, pause) =
        recover_before_idle_election(&fixture, &sessions, window, claim, failure, &execution);
    let primary_process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let successor_admission = beryl_app::cas_projection::test_faults::install_acquisition_barrier(
        fixture.service().service_generation(),
        beryl_app::cas_projection::test_faults::AcquisitionBarrierStage::SessionPrepared,
    );
    submission::submit(&fixture, thread_id(1));
    let limit = syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap();
    let before = fixture
        .storage
        .input_gate(&fixture.home_reference, thread_id(1), limit)
        .unwrap();
    let unavailable = fixture
        .service()
        .accepted_input_scheduler_diagnostics()
        .recovered_pending_execution_unavailable();
    pause.release();
    wait_until(|| {
        let diagnostics = fixture.service().accepted_input_scheduler_diagnostics();
        diagnostics.fatal()
            || (diagnostics.recovered_pending_execution_unavailable() > unavailable
                && fixture
                    .root(1)
                    .join("runtime-unsubscribe-evidence.json")
                    .exists())
    });
    primary_process.assert_exited();
    assert_eq!(
        before,
        fixture
            .storage
            .input_gate(&fixture.home_reference, thread_id(1), limit)
            .unwrap()
    );
    let request: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.root(1).join("runtime-unsubscribe-evidence.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(request["method"], "thread/unsubscribe");
    assert!(
        !fixture
            .service()
            .accepted_input_scheduler_diagnostics()
            .fatal()
    );
    assert!(
        !fixture
            .service()
            .accepted_input_scheduler_diagnostics()
            .stopped()
    );
    assert!(fixture.service().live_home_command().is_ok());
    assert_eq!(
        fixture.home_reference.health().state(),
        beryl_home_store::HomeHealthState::Healthy
    );
    assert!(
        !fixture
            .service()
            .selected_runtime_retry_worker(&sessions)
            .usability_current(&fixture.state.session(), &proof)
    );
    assert_eq!(healthy.execution_binding(), &healthy_binding);
    assert!(healthy.session_authority_current_for_test());
    assert!(healthy_process.running());
    drop(healthy);
    thread::scope(|scope| {
        let closing = scope.spawn(|| sessions.close());
        wait_until(|| sessions.diagnostics().closed);
        successor_admission.release();
        closing.join().unwrap();
    });
    close(&mut fixture, &sessions);
    primary_process.assert_exited();
    healthy_process.assert_exited();
}

#[test]
fn runtime_loss_before_selected_usability_publication_revokes_the_original_projection_proof() {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
    };
    let (mut fixture, sessions, _attention) = fixture(8);
    let (window, claim) = runtime_notice::select(&fixture);
    let failure = failed_idle(&fixture, &sessions, "projection-lifetime");
    let worker = fixture.service().selected_runtime_retry_worker(&sessions);
    let session = fixture.state.session();
    let proof = recover_with_idle_election_paused(
        &fixture,
        &sessions,
        window,
        claim,
        failure,
        &binding(&fixture, 1),
    );
    wait_until(|| worker.usability_current(&session, &proof));
    let pid = fixture.evidence(1)["pid"].as_u64().unwrap() as u32;
    let process = ProcessWitness::open(pid);
    let termination = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }.unwrap();
    unsafe { TerminateProcess(termination, 3) }.unwrap();
    unsafe { CloseHandle(termination) }.unwrap();
    process.assert_exited();
    wait_until(|| {
        sessions
            .runtime_failure(binding(&fixture, 1).runtime_id())
            .is_some()
    });
    assert!(!worker.usability_current(&session, &proof));
    assert!(!worker.eligible(&session, window, claim, &binding(&fixture, 1), failure));
    assert_eq!(
        fixture.home_reference.health().state(),
        beryl_home_store::HomeHealthState::Healthy
    );
    let _ = fixture.service.take().unwrap().close();
    assert_eq!(sessions.diagnostics().retained, 0);
    assert_eq!(fixture.token_count(), 0);
}

#[test]
fn failed_home_revokes_selected_runtime_usability_without_crossing_to_a_replacement() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let (window, claim) = runtime_notice::select(&fixture);
    let failure = failed_idle(&fixture, &sessions, "projection-lifetime");
    let worker = fixture.service().selected_runtime_retry_worker(&sessions);
    let session = fixture.state.session();
    let proof = recover_with_idle_election_paused(
        &fixture,
        &sessions,
        window,
        claim,
        failure,
        &binding(&fixture, 1),
    );
    wait_until(|| worker.usability_current(&session, &proof));
    fixture.fail_home();
    assert!(!worker.usability_current(&session, &proof));
    assert!(!worker.eligible(&session, window, claim, &binding(&fixture, 1), failure));
    let _ = fixture.service.take().unwrap().close();
    assert_eq!(sessions.diagnostics().retained, 0);
    assert_eq!(fixture.token_count(), 0);
}
