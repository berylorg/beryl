use super::*;
use beryl_app::{
    cas_projection::{
        ProjectionCoordinatorError, RuntimeInterestError, RuntimeInterestKind,
        ScheduledOrdinaryAdmissionError,
    },
    process_admission::ProcessAdmissionError,
};

#[test]
fn shutdown_fence_refuses_new_preparation_and_runtime_launch() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let fence = fixture.process_admission.test_fence().unwrap();
    assert!(matches!(
        fixture
            .service()
            .checkout_scheduled_session_for_test(thread_id(1), binding(&fixture, 1)),
        Err(ScheduledOrdinaryAdmissionError::Authority(
            ProjectionCoordinatorError::AcquisitionFenced(ProcessAdmissionError::Fenced)
        ))
    ));
    assert!(matches!(
        fixture.acquire(1, RuntimeInterestKind::View),
        Err(RuntimeInterestError::Closed)
    ));
    let revision = sessions.work_revision().unwrap();
    assert!(
        sessions
            .work_page(
                &revision,
                None,
                beryl_app::cas_projection::ScheduledSessionWorkPageLimits::new(16, 65_536).unwrap(),
            )
            .unwrap()
            .records()
            .is_empty()
    );
    assert_eq!(sessions.diagnostics().retained, 0);
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 0);
    assert_eq!(fixture.token_count(), 0);
    assert!(!fixture.root(1).join("runtime-evidence.json").exists());
    fence.try_reopen(true).unwrap();
    close(&mut fixture, &sessions);
}

#[test]
fn shutdown_joins_winning_preparation_through_runtime_and_session_publication() {
    for mode in ["pause-config", "pause-session-config"] {
        let (mut fixture, sessions, _attention) = fixture(8);
        fs::write(fixture.root(1).join("fixture-mode"), mode).unwrap();
        begin(&fixture, 1);
        let evidence = if mode == "pause-config" {
            "runtime-evidence.json"
        } else {
            "runtime-session-evidence-1.json"
        };
        wait_until(|| fixture.root(1).join(evidence).exists());
        let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
        let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
        let fence = fixture.process_admission.test_fence().unwrap();
        assert_eq!(
            fence.try_reopen(true),
            Err(ProcessAdmissionError::Unsettled)
        );
        assert!(process.running());
        fs::write(fixture.root(1).join("release-config"), "ready").unwrap();
        support::ready(&view);
        wait_until(|| sessions.diagnostics().available == 1);
        wait_until(|| match fence.try_reopen(true) {
            Ok(()) => true,
            Err(ProcessAdmissionError::Unsettled) => false,
            Err(error) => panic!("acquisition fence failed: {error:?}"),
        });
        assert!(
            fixture
                .root(1)
                .join("runtime-session-evidence-1.json")
                .exists()
        );
        drop(view);
        close(&mut fixture, &sessions);
        process.assert_exited();
    }
}

#[test]
fn shutdown_waits_for_failed_runtime_admission_to_dispose_its_process() {
    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "pause-reject-config").unwrap();
    begin(&fixture, 1);
    wait_until(|| fixture.root(1).join("runtime-evidence.json").exists());
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let fence = fixture.process_admission.test_fence().unwrap();
    assert_eq!(
        fence.try_reopen(true),
        Err(ProcessAdmissionError::Unsettled)
    );
    fs::write(fixture.root(1).join("release-config"), "ready").unwrap();
    process.assert_exited();
    wait_until(|| fixture.service().worker_pool_diagnostics().active() == 0);
    wait_until(|| match fence.try_reopen(true) {
        Ok(()) => true,
        Err(ProcessAdmissionError::Unsettled) => false,
        Err(error) => panic!("failed admission did not settle: {error:?}"),
    });
    assert_eq!(fixture.token_count(), 0);
    assert_eq!(sessions.diagnostics().retained, 0);
    close(&mut fixture, &sessions);
}
