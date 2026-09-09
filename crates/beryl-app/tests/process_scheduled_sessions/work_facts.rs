use super::*;
use beryl_app::cas_projection::{
    ScheduledOrdinaryAdmissionResult, ScheduledSessionWorkError, ScheduledSessionWorkPageLimits,
    ScheduledSessionWorkState,
};

fn limits(records: usize) -> ScheduledSessionWorkPageLimits {
    ScheduledSessionWorkPageLimits::new(records, 65_536).unwrap()
}

#[test]
fn session_fact_pages_are_ordered_bounded_and_do_not_reap_or_authorize_work() {
    let (mut fixture, sessions) = fixture(211);
    let first_thread = fixture.thread;
    let second_thread = fixture.create_ordinary(213);
    let first_server = NormalTerminalServer::spawn_admission_only();
    let second_server = NormalTerminalServer::spawn_admission_only();
    let empty_revision = sessions.work_revision().unwrap();
    assert!(
        sessions
            .work_page(&empty_revision, None, limits(1))
            .unwrap()
            .records()
            .is_empty()
    );
    let first_registration = install_session(
        &fixture,
        &sessions,
        first_thread,
        first_server.endpoint(),
        73_001,
    );
    install_session(
        &fixture,
        &sessions,
        second_thread,
        second_server.endpoint(),
        73_002,
    );
    first_server.wait_for_admission();
    second_server.wait_for_admission();
    assert!(matches!(
        sessions.work_page(&empty_revision, None, limits(1)),
        Err(ScheduledSessionWorkError::StaleRevision)
    ));
    let revision = sessions.work_revision().unwrap();
    let first = sessions.work_page(&revision, None, limits(1)).unwrap();
    assert_eq!(first.records().len(), 1);
    assert_eq!(
        first.records()[0].thread_id(),
        first_thread.min(second_thread)
    );
    assert_eq!(
        first.records()[0].session().unwrap().state(),
        ScheduledSessionWorkState::Available
    );
    assert!(first.records()[0].preparation().is_none());
    let second = sessions
        .work_page(&revision, first.next_cursor(), limits(1))
        .unwrap();
    assert_eq!(second.records().len(), 1);
    assert_eq!(
        second.records()[0].thread_id(),
        first_thread.max(second_thread)
    );
    assert!(second.next_cursor().is_none());
    let byte_limited = sessions
        .work_page(
            &revision,
            None,
            ScheduledSessionWorkPageLimits::new(256, first.bytes()).unwrap(),
        )
        .unwrap();
    assert_eq!(byte_limited.records().len(), 1);
    assert!(byte_limited.next_cursor().is_some());
    assert!(matches!(
        sessions.work_page(
            &revision,
            None,
            ScheduledSessionWorkPageLimits::new(256, first.bytes() - 1).unwrap()
        ),
        Err(ScheduledSessionWorkError::ByteLimit)
    ));
    assert_eq!(sessions.work_revision().unwrap(), revision);

    let lease = match fixture
        .store
        .checkout_scheduled_session_for_test(first_thread, syndic::execution_binding())
        .unwrap()
    {
        ScheduledOrdinaryAdmissionResult::Issued(lease) => lease,
        _ => panic!("registered session was not available"),
    };
    assert!(matches!(
        sessions.work_page(&revision, first.next_cursor(), limits(1)),
        Err(ScheduledSessionWorkError::StaleRevision)
    ));
    let checked_out = sessions.work_revision().unwrap();
    let page = sessions.work_page(&checked_out, None, limits(2)).unwrap();
    assert_eq!(
        page.records()
            .iter()
            .find(|row| row.thread_id() == first_thread)
            .unwrap()
            .session()
            .unwrap()
            .state(),
        ScheduledSessionWorkState::CheckedOut
    );
    assert!(sessions.retire(first_registration));
    let retiring = sessions.work_revision().unwrap();
    assert_ne!(checked_out, retiring);
    let page = sessions.work_page(&retiring, None, limits(2)).unwrap();
    assert_eq!(
        page.records()
            .iter()
            .find(|row| row.thread_id() == first_thread)
            .unwrap()
            .session()
            .unwrap()
            .state(),
        ScheduledSessionWorkState::Retiring { checked_out: true }
    );
    drop(lease);
    let returned = sessions.work_revision().unwrap();
    assert_ne!(returned, retiring);
    let page = sessions.work_page(&returned, None, limits(2)).unwrap();
    assert_eq!(
        page.records()
            .iter()
            .find(|row| row.thread_id() == first_thread)
            .unwrap()
            .session()
            .unwrap()
            .state(),
        ScheduledSessionWorkState::Retiring { checked_out: false }
    );
    assert_eq!(
        sessions.work_page(&returned, None, limits(2)).unwrap(),
        page
    );
    assert_eq!(sessions.work_revision().unwrap(), returned);

    wait_until("explicit session retirement reaping", || {
        (sessions.diagnostics().retained == 1).then_some(())
    });
    assert!(matches!(
        sessions.work_page(&returned, None, limits(2)),
        Err(ScheduledSessionWorkError::StaleRevision)
    ));
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    first_server.join();
    second_server.join();
    assert!(matches!(
        sessions.work_revision(),
        Err(ScheduledSessionWorkError::Closed)
    ));
    drop(directory);
}

#[test]
fn session_fact_revisions_are_owner_bound_and_observation_rejects_unattached_owners() {
    let (provider, unattached) = ProcessScheduledExecutionProvider::new();
    assert!(matches!(
        unattached.work_revision(),
        Err(ScheduledSessionWorkError::Unattached)
    ));
    assert!(ScheduledSessionWorkPageLimits::new(0, 1).is_err());
    assert!(ScheduledSessionWorkPageLimits::new(1, 0).is_err());
    drop(provider);
    let (first, first_sessions) = fixture(215);
    let (second, second_sessions) = fixture(217);
    let revision = first_sessions.work_revision().unwrap();
    assert!(matches!(
        second_sessions.work_page(&revision, None, limits(1)),
        Err(ScheduledSessionWorkError::ForeignCursor)
    ));
    let (first_directory, first_service) = first.into_service();
    let (second_directory, second_service) = second.into_service();
    first_service.close().unwrap();
    second_service.close().unwrap();
    assert!(matches!(
        first_sessions.work_page(&revision, None, limits(1)),
        Err(ScheduledSessionWorkError::Closed)
    ));
    drop((first_directory, second_directory));
}
