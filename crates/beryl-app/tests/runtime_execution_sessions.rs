#![cfg(all(feature = "test-faults", target_os = "windows"))]

#[allow(dead_code)]
#[path = "support/managed_runtime.rs"]
mod support;

use std::{fs, thread};

use beryl_app::cas_projection::{
    OrdinaryDynamicToolAuthority, OrdinaryDynamicToolHandlers, OrdinaryTurnExecutionRequest,
    ProcessScheduledExecutionProvider, ProjectionConnectionServiceCloseOutcome, RuntimeFailure,
    RuntimeInterestError, RuntimeInterestKind, RuntimeInterestStatus, RuntimeSessionAdmissionError,
    ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryRequestPolicy,
    ScheduledSessionRegistrationError,
};
use beryl_backend::{ThreadStartOptions, TurnStartOptions};
use beryl_model::{ExecutionBinding, RootId, SyndicThreadId};
use support::{Fixture, ProcessWitness, TIMEOUT, ready, wait_until};

struct NoToolInvocation;

impl OrdinaryDynamicToolAuthority for NoToolInvocation {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        panic!("custody verification does not dispatch tools")
    }
}

fn policy() -> ScheduledOrdinaryRequestPolicy {
    ScheduledOrdinaryRequestPolicy::new(
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        TIMEOUT,
        OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
    )
}

fn process(fixture: &Fixture) -> ProcessWitness {
    ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32)
}

fn close(fixture: &mut Fixture) {
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(fixture.token_count(), 0);
}

#[test]
fn admitted_session_keeps_shared_runtime_after_view_detachment_and_releases_it_last() {
    let mut fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let initial = ready(&view);
    let process = process(&fixture);
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    assert_eq!(ready(&work), initial);
    let session = fixture
        .service()
        .admit_runtime_session(work, TIMEOUT)
        .unwrap();
    assert_eq!(session.process_generation(), initial.process_generation());
    let evidence: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.root(1).join("runtime-session-evidence-1.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(evidence["foreground"], true);
    assert_eq!(evidence["authenticated"], true);
    assert!(!fixture.root(2).join("runtime-evidence.json").exists());
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 4);
    drop(view);
    assert!(process.running());
    let reattached = fixture.acquire(2, RuntimeInterestKind::View).unwrap();
    assert_eq!(ready(&reattached), initial);
    drop(reattached);
    assert!(process.running());
    drop(session);
    wait_until(|| {
        fixture.token_count() == 0 && fixture.service().worker_pool_diagnostics().active() == 0
    });
    process.assert_exited();
    close(&mut fixture);
}

#[test]
fn provider_checkout_and_return_keep_the_exact_runtime_interest_until_retirement() {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let mut fixture = Fixture::with_provider(Box::new(provider));
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let initial = ready(&view);
    let process = process(&fixture);
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let binding = work.binding().clone();
    let session = fixture
        .service()
        .admit_runtime_session(work, TIMEOUT)
        .unwrap();
    let thread_id = SyndicThreadId::from_bytes([123; 16]);
    let registration = sessions
        .register(
            thread_id,
            binding.clone(),
            session,
            policy(),
            fixture.state.assets(),
            Box::new(NoToolInvocation),
        )
        .unwrap();
    drop(view);
    assert_eq!(sessions.diagnostics().available, 1);
    let ScheduledOrdinaryAdmissionResult::Issued(lease) = fixture
        .service()
        .checkout_scheduled_session_for_test(thread_id, binding.clone())
        .unwrap()
    else {
        panic!("registered session should issue its exact checkout")
    };
    assert_eq!(lease.process_generation(), initial.process_generation());
    assert_eq!(sessions.diagnostics().checked_out, 1);
    assert_eq!(sessions.diagnostics().available, 0);
    let reattached = fixture.acquire(2, RuntimeInterestKind::View).unwrap();
    assert_eq!(ready(&reattached), initial);
    drop(reattached);
    assert!(process.running());
    drop(lease);
    assert_eq!(sessions.diagnostics().available, 1);
    assert_eq!(sessions.diagnostics().checked_out, 0);
    let ScheduledOrdinaryAdmissionResult::Issued(lease) = fixture
        .service()
        .checkout_scheduled_session_for_test(thread_id, binding)
        .unwrap()
    else {
        panic!("returned session should be usable once again")
    };
    assert_eq!(lease.process_generation(), initial.process_generation());
    assert!(sessions.retire(registration));
    assert_eq!(sessions.diagnostics().retiring, 1);
    assert!(process.running());
    drop(lease);
    wait_until(|| sessions.diagnostics().retained == 0 && fixture.token_count() == 0);
    process.assert_exited();
    close(&mut fixture);
}

#[test]
fn view_and_foreign_owner_interests_cannot_create_execution_sessions() {
    let mut fixture = Fixture::new();
    let mut foreign = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let retained = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    assert!(matches!(
        fixture.service().admit_runtime_session(view, TIMEOUT),
        Err(RuntimeSessionAdmissionError::RequiredWorkInterest)
    ));
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    assert!(matches!(
        foreign.service().admit_runtime_session(work, TIMEOUT),
        Err(RuntimeSessionAdmissionError::OwnerMismatch)
    ));
    assert!(
        !fixture
            .root(1)
            .join("runtime-session-evidence-1.json")
            .exists()
    );
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 2);
    drop(retained);
    close(&mut fixture);
    close(&mut foreign);
}

#[test]
fn registry_rejects_an_admitted_session_for_another_root_without_provider_effects() {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let mut fixture = Fixture::with_provider(Box::new(provider));
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let binding = work.binding().clone();
    let session = fixture
        .service()
        .admit_runtime_session(work, TIMEOUT)
        .unwrap();
    let mismatched = ExecutionBinding::new(
        binding.runtime_id(),
        RootId::from_bytes([19; 16]),
        binding.root_path().clone(),
    );
    assert_eq!(
        sessions.register(
            SyndicThreadId::from_bytes([123; 16]),
            mismatched,
            session,
            policy(),
            fixture.state.assets(),
            Box::new(NoToolInvocation),
        ),
        Err(ScheduledSessionRegistrationError::SessionAuthorityUnavailable)
    );
    assert_eq!(sessions.diagnostics().retained, 0);
    wait_until(|| fixture.service().worker_pool_diagnostics().active() == 2);
    drop(view);
    close(&mut fixture);
}

#[test]
fn contradictory_session_configuration_ends_the_runtime_period_and_requires_a_fresh_process() {
    let mut fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let initial = ready(&view);
    let process = process(&fixture);
    let existing = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let existing = fixture
        .service()
        .admit_runtime_session(existing, TIMEOUT)
        .unwrap();
    fs::write(fixture.root(1).join("fixture-mode"), "reject-config").unwrap();
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let error = fixture
        .service()
        .admit_runtime_session(work, TIMEOUT)
        .unwrap_err();
    assert!(
        matches!(&error, RuntimeSessionAdmissionError::Admission(source)
        if matches!(source.backend_error(), Some(beryl_backend::ManagedBackendError::ForegroundIngress {
            method, source: beryl_backend::ForegroundIngressError::MalformedResponse
        }) if method == "config/read")),
        "unexpected admission error: {error:?}"
    );
    assert!(!view.is_current(initial.activity_period()));
    assert_eq!(
        view.status(),
        RuntimeInterestStatus::Unavailable(RuntimeFailure::Admission)
    );
    fs::write(fixture.root(1).join("fixture-mode"), "").unwrap();
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    assert!(matches!(
        fixture.service().admit_runtime_session(work, TIMEOUT),
        Err(RuntimeSessionAdmissionError::RuntimeUnavailable)
    ));
    wait_until(|| {
        fixture.token_count() == 0 && fixture.service().worker_pool_diagnostics().active() == 0
    });
    process.assert_exited();
    drop(view);
    drop(existing);
    let mut replacement = None;
    wait_until(|| match fixture.acquire(2, RuntimeInterestKind::View) {
        Ok(view) => {
            replacement = Some(view);
            true
        }
        Err(RuntimeInterestError::Retiring) => false,
        Err(error) => panic!("fresh runtime admission failed: {error}"),
    });
    let replacement = replacement.unwrap();
    let current = ready(&replacement);
    assert_ne!(current.process_generation(), initial.process_generation());
    assert_ne!(current.activity_period(), initial.activity_period());
    drop(replacement);
    close(&mut fixture);
}

#[test]
fn connection_capacity_refuses_session_admission_before_another_handshake() {
    let mut fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let first = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let first = fixture
        .service()
        .admit_runtime_session(first, TIMEOUT)
        .unwrap();
    let second = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let second = fixture
        .service()
        .admit_runtime_session(second, TIMEOUT)
        .unwrap();
    let excess = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    assert!(matches!(
        fixture.service().admit_runtime_session(excess, TIMEOUT),
        Err(RuntimeSessionAdmissionError::Admission(_))
    ));
    assert!(
        !fixture
            .root(1)
            .join("runtime-session-evidence-3.json")
            .exists()
    );
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 6);
    drop(first);
    wait_until(|| fixture.service().worker_pool_diagnostics().active() == 4);
    let replacement = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let replacement = fixture
        .service()
        .admit_runtime_session(replacement, TIMEOUT)
        .unwrap();
    drop(view);
    drop(second);
    drop(replacement);
    close(&mut fixture);
}

#[test]
fn candidate_transport_failure_keeps_the_current_runtime_period_and_releases_capacity() {
    let mut fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let initial = ready(&view);
    let process = process(&fixture);
    fs::write(fixture.root(1).join("fixture-mode"), "drop-config").unwrap();
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    assert!(matches!(
        fixture.service().admit_runtime_session(work, TIMEOUT),
        Err(RuntimeSessionAdmissionError::Admission(_))
    ));
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 2);
    assert!(view.is_current(initial.activity_period()));
    assert!(process.running());
    fs::write(fixture.root(1).join("fixture-mode"), "").unwrap();
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let session = fixture
        .service()
        .admit_runtime_session(work, TIMEOUT)
        .unwrap();
    assert_eq!(session.process_generation(), initial.process_generation());
    drop(view);
    drop(session);
    close(&mut fixture);
    process.assert_exited();
}

#[test]
fn generation_loss_during_session_admission_rejects_completion_and_retires_the_process() {
    let mut fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let process = process(&fixture);
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    thread::scope(|scope| {
        let admission = scope.spawn(|| fixture.service().admit_runtime_session(work, TIMEOUT));
        wait_until(|| {
            fixture
                .root(1)
                .join("runtime-session-evidence-1.json")
                .exists()
        });
        fixture.fail_home();
        wait_until(|| view.status() == RuntimeInterestStatus::Retired);
        fs::write(fixture.root(1).join("release-config"), "release").unwrap();
        assert!(admission.join().unwrap().is_err());
    });
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(_)
    ));
    process.assert_exited();
    assert_eq!(fixture.token_count(), 0);
}

#[test]
fn last_view_release_during_foreground_admission_preserves_the_pending_required_interest() {
    let mut fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let initial = ready(&view);
    let process = process(&fixture);
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let session = thread::scope(|scope| {
        let admission = scope.spawn(|| fixture.service().admit_runtime_session(work, TIMEOUT));
        wait_until(|| {
            fixture
                .root(1)
                .join("runtime-session-evidence-1.json")
                .exists()
        });
        drop(view);
        assert!(process.running());
        fs::write(fixture.root(1).join("release-config"), "release").unwrap();
        admission.join().unwrap().unwrap()
    });
    assert_eq!(session.process_generation(), initial.process_generation());
    assert!(process.running());
    drop(session);
    close(&mut fixture);
    process.assert_exited();
}
