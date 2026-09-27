use super::support::{self, drive, drive_until};
use beryl_app::cas_projection::{NativeLineageOperation, NativeLineageRecoveryControl};
use beryl_app::main_window::{
    MainWindowConversationComposerCloseAdvance as Advance,
    MainWindowConversationComposerCloseTicket as Ticket,
};
use beryl_model::BindingRevision;
use gpui::{EntityInputHandler, TestAppContext};
use std::num::NonZeroUsize;

#[gpui::test]
fn recovery_transfers_native_control_and_refuses_late_attachment(cx: &mut TestAppContext) {
    let (fixture, cx) = support::mounted(cx, "resident-native-control-detachment", 181);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let selection = composer.read_with(cx, |composer, _| composer.selection_identity());
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    fixture.mount.update(cx, |mount, cx| {
        mount.attach_native_lineage_recovery(control.clone(), cx);
    });
    drive(cx, 4);
    let close = cx.update(|window, app| {
        fixture
            .mount
            .update(app, |mount, cx| mount.begin_window_close(window, cx))
            .unwrap()
    });
    fixture.mount.update(cx, |mount, cx| {
        assert!(
            mount
                .detach_interrupted_exit_native_lineage_control(close.ticket, cx)
                .is_err()
        );
    });
    drive_until(cx, "native control close becomes ready", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == Advance::Ready
    });
    drive_until(cx, "native control editor settles", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    fixture.mount.update(cx, |mount, cx| {
        assert!(
            mount
                .detach_interrupted_exit_native_lineage_control(close.ticket, cx)
                .is_err()
        );
    });
    composer
        .update(cx, |composer, cx| {
            composer.test_set_shutdown_interaction_gated(true, cx)
        })
        .unwrap();
    fixture.mount.update(cx, |mount, cx| {
        assert!(
            mount
                .fence_interrupted_exit_resident(close.ticket, cx)
                .unwrap()
        );
    });
    let restoration = composer.read_with(cx, |composer, _| {
        composer.recovery_snapshot().unwrap().restoration().clone()
    });
    let key = control
        .install_route_for_test(
            selection.claim().thread_id(),
            selection.claim().thread_id(),
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            1,
            true,
        )
        .unwrap();
    let worker = fixture.mount.read_with(cx, |mount, _| {
        mount.test_native_lineage_worker(std::future::pending())
    });
    fixture.service.test_with_close_slot_locked(|| {
        fixture.mount.update(cx, |mount, cx| {
            assert!(
                mount
                    .detach_interrupted_exit_native_lineage_control(close.ticket, cx)
                    .is_err()
            );
        });
    });
    drop(worker);
    let stale = Ticket::for_test(fixture.mount.entity_id(), 99, selection);
    let detached = fixture.service.test_with_close_slot_locked(|| {
        fixture.mount.update(cx, |mount, cx| {
            assert!(
                mount
                    .detach_interrupted_exit_native_lineage_control(stale, cx)
                    .is_err()
            );
            let detached = mount
                .detach_interrupted_exit_native_lineage_control(close.ticket, cx)
                .unwrap()
                .unwrap();
            assert_eq!(
                detached
                    .snapshot_for_thread(selection.claim().thread_id())
                    .unwrap()
                    .key(),
                key
            );
            assert!(
                mount
                    .detach_interrupted_exit_native_lineage_control(close.ticket, cx)
                    .unwrap()
                    .is_none()
            );
            mount.attach_native_lineage_recovery(control.clone(), cx);
            assert!(
                mount
                    .detach_interrupted_exit_native_lineage_control(close.ticket, cx)
                    .unwrap()
                    .is_none()
            );
            assert!(
                mount
                    .detach_interrupted_exit_native_lineage_control(stale, cx)
                    .is_err()
            );
            detached
        })
    });
    drive(cx, 12);
    cx.update(|window, app| {
        fixture.mount.update(app, |mount, cx| {
            assert!(!mount.refresh_native_lineage_recovery(window, cx).unwrap());
            assert!(mount.native_lineage_recovery_snapshot().is_none());
            assert!(
                mount
                    .release_window_close(close.ticket, window, cx)
                    .is_err()
            );
            assert!(
                mount
                    .authorize_window_close_disposal(close.ticket, window, cx)
                    .is_err()
            );
        });
    });
    let detached_service = fixture.service.test_with_close_slot_locked(|| {
        fixture.mount.update(cx, |mount, cx| {
            mount
                .detach_interrupted_exit_service(close.ticket, cx)
                .unwrap()
                .unwrap()
        })
    });
    cx.simulate_keystrokes("x ctrl-z ctrl-v");
    drive(cx, 12);
    composer.read_with(cx, |composer, _| {
        assert_eq!(
            composer.recovery_snapshot().unwrap().restoration(),
            &restoration
        );
        assert_eq!(composer.gpui_input().entity_id(), input.entity_id());
        assert!(!composer.test_widget_released());
    });
    assert_eq!(
        input
            .update(cx, |input, _| input.export_restoration(Some(
                selection.binding().range_history_frontier()
            )))
            .unwrap(),
        restoration
    );
    assert!(fixture.service.test_window_close_is_current(close.ticket));
    drop((detached, control, detached_service, input, composer));
    support::finish(fixture, cx);
}
