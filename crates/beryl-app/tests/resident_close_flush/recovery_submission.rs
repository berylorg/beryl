use super::support::{self, drive, drive_until};
use beryl_app::{
    cas_projection::{ProjectionServiceConfig, SubmissionExecutionWake},
    main_window::{
        MainWindowComposerSubmissionRequestSource,
        MainWindowConversationComposerCloseAdvance as Advance,
        MainWindowConversationComposerCloseTicket as Ticket,
        MainWindowConversationComposerSubmissionStatus as Status,
    },
    process_admission::ProcessAdmissionGate,
};
use beryl_home_store::MinimumTurnCaptureReserve;
use gpui::TestAppContext;

#[gpui::test]
fn recovery_transfers_submission_source_once_without_waking_execution(cx: &mut TestAppContext) {
    let mut probe = None;
    let (fixture, cx) = support::mounted_with_sources(
        cx,
        "resident-submission-source-detachment",
        253,
        Box::new(support::configure),
        |home| {
            let (wake, observed) =
                SubmissionExecutionWake::test_for_home(home, ProcessAdmissionGate::new());
            probe = Some(observed);
            MainWindowComposerSubmissionRequestSource::new(
                wake,
                ProjectionServiceConfig::try_new(
                    1,
                    4,
                    MinimumTurnCaptureReserve::try_new(1).unwrap(),
                )
                .unwrap()
                .turn_start_admission_requirement(),
            )
        },
    );
    let probe = probe.unwrap();
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let close = cx.update(|window, app| {
        fixture
            .mount
            .update(app, |mount, cx| mount.begin_window_close(window, cx))
            .unwrap()
    });
    assert!(
        fixture
            .mount
            .update(cx, |mount, cx| {
                mount.detach_interrupted_exit_submission_source(close.ticket, cx)
            })
            .is_err()
    );
    drive_until(cx, "submission source close becomes ready", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == Advance::Ready
    });
    drive_until(cx, "submission source editor settles", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    assert!(
        fixture
            .mount
            .update(cx, |mount, cx| {
                mount.detach_interrupted_exit_submission_source(close.ticket, cx)
            })
            .is_err()
    );
    composer
        .update(cx, |composer, cx| {
            composer.test_set_shutdown_interaction_gated(true, cx)
        })
        .unwrap();
    assert!(
        fixture
            .mount
            .update(cx, |mount, cx| {
                mount.fence_interrupted_exit_resident(close.ticket, cx)
            })
            .unwrap()
    );
    let selection = composer.read_with(cx, |composer, _| composer.selection_identity());
    let stale = Ticket::for_test(fixture.mount.entity_id(), 99, selection);
    let before = composer.read_with(cx, |composer, _| {
        composer.recovery_snapshot().unwrap().restoration().clone()
    });
    let source = fixture.service.test_with_close_slot_locked(|| {
        fixture.mount.update(cx, |mount, cx| {
            assert!(
                mount
                    .detach_interrupted_exit_submission_source(stale, cx)
                    .is_err()
            );
            let source = mount
                .detach_interrupted_exit_submission_source(close.ticket, cx)
                .unwrap()
                .unwrap();
            assert!(
                mount
                    .detach_interrupted_exit_submission_source(close.ticket, cx)
                    .unwrap()
                    .is_none()
            );
            assert!(
                mount
                    .detach_interrupted_exit_submission_source(stale, cx)
                    .is_err()
            );
            source
        })
    });
    assert_eq!(probe.wake_count(), 0);
    drop(source);
    let generation = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_start_generation());
    cx.update(|window, app| {
        fixture.mount.update(app, |mount, cx| {
            mount
                .test_begin_submission_start(selection, window, cx)
                .unwrap();
        });
    });
    cx.simulate_keystrokes("x enter ctrl-z ctrl-v");
    drive(cx, 12);
    assert_eq!(probe.wake_count(), 0);
    fixture.mount.read_with(cx, |mount, _| {
        assert_eq!(mount.test_submission_start_generation(), generation);
        assert_eq!(
            mount.contribution().unwrap().entity_id(),
            composer.entity_id()
        );
        let diagnostics = mount.test_submission_diagnostics();
        assert_eq!(diagnostics.status(), Status::Idle);
        assert!(!diagnostics.active_ticket());
        assert!(!diagnostics.active_task());
        assert!(!diagnostics.prepared_request());
        assert!(!diagnostics.successor());
    });
    composer.read_with(cx, |composer, _| {
        assert_eq!(composer.recovery_snapshot().unwrap().restoration(), &before);
        assert_eq!(composer.gpui_input().entity_id(), input.entity_id());
        assert!(!composer.test_widget_released());
    });
    assert_eq!(
        input
            .update(cx, |input, _| {
                input.export_restoration(Some(selection.binding().range_history_frontier()))
            })
            .unwrap(),
        before
    );
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    assert!(fixture.service.test_window_close_is_current(close.ticket));
    drop((probe, input, composer));
    support::finish(fixture, cx);
}
