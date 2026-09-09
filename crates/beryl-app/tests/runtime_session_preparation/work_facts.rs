use super::*;
use beryl_app::cas_projection::{
    ScheduledSessionWorkError, ScheduledSessionWorkPageLimits, ScheduledSessionWorkState,
};

#[test]
fn preparation_handoff_and_exact_checkout_invalidate_read_only_session_pages() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let limits = ScheduledSessionWorkPageLimits::new(16, 65_536).unwrap();
    let empty = sessions.work_revision().unwrap();
    assert!(
        sessions
            .work_page(&empty, None, limits)
            .unwrap()
            .records()
            .is_empty()
    );
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    begin(&fixture, 1);
    wait_until(|| fixture.root(1).join("runtime-evidence.json").exists());
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    assert!(matches!(
        sessions.work_page(&empty, None, limits),
        Err(ScheduledSessionWorkError::StaleRevision)
    ));
    let preparing = sessions.work_revision().unwrap();
    let page = sessions.work_page(&preparing, None, limits).unwrap();
    assert_eq!(page.records().len(), 1);
    assert_eq!(page.records()[0].thread_id(), thread_id(1));
    assert!(page.records()[0].session().is_none());
    let preparation = page.records()[0].preparation().unwrap();
    assert_eq!(preparation.execution_binding(), &binding(&fixture, 1));
    assert!(!preparation.is_complete());
    assert_eq!(sessions.work_page(&preparing, None, limits).unwrap(), page);
    assert_eq!(sessions.work_revision().unwrap(), preparing);
    assert!(process.running());

    fs::write(fixture.root(1).join("release-config"), "ready").unwrap();
    wait_until(|| sessions.diagnostics().available == 1);
    assert!(matches!(
        sessions.work_page(&preparing, None, limits),
        Err(ScheduledSessionWorkError::StaleRevision)
    ));
    let registered = sessions.work_revision().unwrap();
    let page = sessions.work_page(&registered, None, limits).unwrap();
    assert_eq!(page.records().len(), 1);
    assert_eq!(
        page.records()[0].session().unwrap().state(),
        ScheduledSessionWorkState::Available
    );
    assert_eq!(
        page.records()[0].session().unwrap().execution_binding(),
        &binding(&fixture, 1)
    );
    let lease = checkout(&fixture, 1);
    let checked_out = sessions.work_revision().unwrap();
    assert_ne!(registered, checked_out);
    let page = sessions.work_page(&checked_out, None, limits).unwrap();
    assert_eq!(
        page.records()[0].session().unwrap().state(),
        ScheduledSessionWorkState::CheckedOut
    );
    drop(lease);
    assert_ne!(sessions.work_revision().unwrap(), checked_out);
    close(&mut fixture, &sessions);
    assert!(matches!(
        sessions.work_revision(),
        Err(ScheduledSessionWorkError::Closed)
    ));
    process.assert_exited();
}
