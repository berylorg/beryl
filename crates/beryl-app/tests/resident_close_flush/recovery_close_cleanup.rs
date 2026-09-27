use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
};

use beryl_app::main_window::{
    MainWindowConversationComposerAutosavePhase as Phase,
    MainWindowConversationComposerCloseAdvance as Advance,
};
use gpui::TestAppContext;

use super::support::{self, drive_until};

#[gpui::test]
fn recovery_waits_for_close_cleanup_between_polls(cx: &mut TestAppContext) {
    let (fixture, cx) = support::mounted(cx, "resident-close-cleanup-drain", 176);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let service_references = std::sync::Arc::strong_count(&fixture.service);
    let released = Arc::new(AtomicBool::new(false));
    let ready = released.clone();
    let mut worker = fixture.mount.read_with(cx, |mount, _| {
        mount.test_window_close_cleanup(std::future::poll_fn(move |_| {
            if ready.load(Ordering::Acquire) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }))
    });
    let mut context = Context::from_waker(Waker::noop());
    assert!(worker.as_mut().poll(&mut context).is_pending());
    assert_eq!(
        std::sync::Arc::strong_count(&fixture.service),
        service_references + 1
    );
    let close = cx.update(|window, app| {
        fixture
            .mount
            .update(app, |mount, cx| mount.begin_window_close(window, cx))
            .unwrap()
    });
    drive_until(cx, "close recovery close becomes ready", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == Advance::Ready
    });
    drive_until(cx, "close recovery editor settles", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
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
    let before = composer.read_with(cx, |composer, _| {
        composer.recovery_snapshot().unwrap().restoration().clone()
    });
    fixture.service.test_with_close_slot_locked(|| {
        fixture.mount.update(cx, |mount, cx| {
            let diagnostics = mount.autosave_diagnostics();
            assert_eq!(diagnostics.phase(), Phase::Idle);
            assert_eq!(diagnostics.retained_tasks(), 0);
            assert_eq!(mount.test_window_close_retained_workers(), 1);
            assert!(
                mount
                    .detach_interrupted_exit_publication_adapters(close.ticket, cx)
                    .is_err()
            );
            assert!(
                mount
                    .detach_interrupted_exit_configurator(close.ticket, cx)
                    .is_err()
            );
            assert!(
                mount
                    .detach_interrupted_exit_submission_source(close.ticket, cx)
                    .is_err()
            );
        });
    });
    released.store(true, Ordering::Release);
    assert!(worker.as_mut().poll(&mut context).is_ready());
    drop(worker);
    assert_eq!(
        std::sync::Arc::strong_count(&fixture.service),
        service_references
    );
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.test_window_close_retained_workers()),
        0
    );

    let abandoned = fixture.mount.read_with(cx, |mount, _| {
        mount.test_window_close_cleanup(async { panic!("abandoned close must not execute") })
    });
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.test_window_close_retained_workers()),
        1
    );
    drop(abandoned);
    assert_eq!(
        std::sync::Arc::strong_count(&fixture.service),
        service_references
    );
    let mut cancelled = fixture.mount.read_with(cx, |mount, _| {
        mount.test_window_close_cleanup(std::future::pending())
    });
    assert!(cancelled.as_mut().poll(&mut context).is_pending());
    drop(cancelled);
    let mut unwinding = fixture.mount.read_with(cx, |mount, _| {
        mount.test_window_close_cleanup(async { panic!("injected close cleanup unwind") })
    });
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = unwinding.as_mut().poll(&mut context);
        }))
        .is_err()
    );
    drop(unwinding);
    assert_eq!(
        std::sync::Arc::strong_count(&fixture.service),
        service_references
    );
    fixture.service.test_with_close_slot_locked(|| {
        fixture.mount.update(cx, |mount, cx| {
            assert_eq!(mount.test_window_close_retained_workers(), 0);
            assert!(
                mount
                    .detach_interrupted_exit_publication_adapters(close.ticket, cx)
                    .unwrap()
                    .is_some()
            );
            assert!(
                mount
                    .detach_interrupted_exit_configurator(close.ticket, cx)
                    .unwrap()
                    .is_some()
            );
            assert!(
                mount
                    .detach_interrupted_exit_submission_source(close.ticket, cx)
                    .unwrap()
                    .is_some()
            );
        });
    });
    composer.read_with(cx, |composer, _| {
        assert_eq!(composer.recovery_snapshot().unwrap().restoration(), &before);
        assert_eq!(composer.gpui_input().entity_id(), input.entity_id());
        assert!(!composer.test_widget_released());
    });
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    assert!(fixture.service.test_window_close_is_current(close.ticket));
    drop((input, composer));
    support::finish(fixture, cx);
}
