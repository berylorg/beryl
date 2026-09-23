use super::*;
use beryl_app::process_admission::ProcessAdmissionError;

#[test]
fn prepared_creation_remains_stale_after_process_fence_reopens() {
    let fixture = Fixture::new(2);
    let fenced = fixture.prepare(210, CommandCancellation::new());
    let stale = fixture.prepare(212, CommandCancellation::new());
    let retained = stale.audit();
    let fence = fixture.process.test_fence().unwrap();
    assert!(matches!(
        fenced.execute(),
        DiscussionCreationOutcome::NotCommitted {
            evidence: DiscussionCreationError::Process(ProcessAdmissionError::Fenced),
        }
    ));
    fence.try_reopen(true).unwrap();
    assert!(matches!(
        stale.execute(),
        DiscussionCreationOutcome::NotCommitted {
            evidence: DiscussionCreationError::Process(ProcessAdmissionError::Stale),
        }
    ));
    no_child(&fixture.store, &fixture.syndic, &fixture.state, 210);
    no_child(&fixture.store, &fixture.syndic, &fixture.state, 212);
    assert_eq!(
        retained
            .reconcile(&fixture.store, &fixture.syndic, &fixture.state)
            .unwrap(),
        DiscussionCreationAuditOutcome::NotCommitted
    );
    assert!(matches!(
        fixture.service().prepare(
            source(&fixture.store, &fixture.syndic),
            request(212),
            CommandCancellation::new()
        ),
        Err(DiscussionCreationError::DuplicateIdentity)
    ));
    drop(retained);
    let fresh = fixture.prepare(212, CommandCancellation::new());
    assert!(matches!(
        fresh.execute(),
        DiscussionCreationOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    fixture.store.close().unwrap();
}
