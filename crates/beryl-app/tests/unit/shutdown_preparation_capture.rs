use super::*;
use crate::cas_projection::*;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

#[test]
fn preparation_only_and_completed_unreaped_workers_remain_in_shutdown_capture() {
    let fixture = Fixture::idle();
    let _fence = fixture.gate.fence().unwrap();
    let (_provider, sessions) = ProcessScheduledExecutionProvider::new();
    sessions.lock().context = fixture.sessions.lock().context.clone();
    let binding = ExecutionBinding::new(
        RuntimeId::from_bytes([71; 16]),
        RootId::from_bytes([72; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl",
        )
        .unwrap(),
    );
    let before = fixture.service.shutdown_work_revision(&sessions).unwrap();
    {
        let mut state = sessions.lock();
        state.work_changed();
        state.preparing.insert(
            fixture.thread,
            PreparationWorker {
                handle: std::thread::spawn(|| {}),
                complete: false,
                binding,
            },
        );
    }
    assert!(
        fixture
            .service
            .validate_shutdown_work_revision(&sessions, &before)
            .is_err()
    );
    for complete in [false, true] {
        {
            let mut state = sessions.lock();
            state.work_changed();
            state.preparing.get_mut(&fixture.thread).unwrap().complete = complete;
        }
        let revision = fixture.service.shutdown_work_revision(&sessions).unwrap();
        let page = fixture
            .service
            .shutdown_work_page(
                &sessions,
                &revision,
                None,
                ProcessWorkPageLimits::new(4, 65_536).unwrap(),
                &ProjectionCancellationToken::new(),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "{error:?}; expected {revision:?}; actual {:?}",
                    fixture.service.shutdown_work_revision(&sessions)
                )
            });
        assert_eq!(page.records.len(), 1);
        let row = &page.records[0];
        assert!(row.preparation_retained);
        assert!(!row.session_registered);
        assert!(!row.projection_flight);
        assert_eq!(row.current_turn_id, None);
        assert_eq!(row.work.preparing, !complete);
    }
    sessions.reap_preparation();
    let revision = fixture.service.shutdown_work_revision(&sessions).unwrap();
    let page = fixture
        .service
        .shutdown_work_page(
            &sessions,
            &revision,
            None,
            ProcessWorkPageLimits::new(4, 65_536).unwrap(),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert!(page.records.is_empty());
}
