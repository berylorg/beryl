use super::support::{self, drive, drive_until};
use beryl_app::main_window::{
    MainWindowConversationComposerCloseAdvance as Advance,
    MainWindowConversationComposerCloseTicket as Ticket,
};
use gpui::{EntityInputHandler, TestAppContext};

#[gpui::test]
fn recovery_detaches_mount_service_once_without_releasing_the_resident(cx: &mut TestAppContext) {
    let (fixture, cx) = support::mounted(cx, "resident-mount-service-detachment", 180);
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
    fixture.mount.update(cx, |mount, cx| {
        assert!(
            mount
                .detach_interrupted_exit_service(close.ticket, cx)
                .is_err()
        );
    });
    drive_until(cx, "publication detachment close becomes ready", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == Advance::Ready
    });
    drive_until(cx, "publication detachment editor settles", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    let selection = composer.read_with(cx, |composer, _| composer.selection_identity());
    let stale = Ticket::for_test(fixture.mount.entity_id(), 99, selection);
    fixture.mount.update(cx, |mount, cx| {
        assert!(
            mount
                .detach_interrupted_exit_service(close.ticket, cx)
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
    let worker = fixture.mount.read_with(cx, |mount, _| {
        mount.test_native_disposal_worker(std::future::pending())
    });
    fixture.service.test_with_close_slot_locked(|| {
        fixture.mount.update(cx, |mount, cx| {
            assert!(
                mount
                    .detach_interrupted_exit_service(close.ticket, cx)
                    .is_err()
            );
        });
    });
    drop(worker);
    let references = std::sync::Arc::strong_count(&fixture.service);
    let detached = fixture.service.test_with_close_slot_locked(|| {
        fixture.mount.update(cx, |mount, cx| {
            assert!(mount.detach_interrupted_exit_service(stale, cx).is_err());
            let detached = mount
                .detach_interrupted_exit_service(close.ticket, cx)
                .unwrap()
                .unwrap();
            assert!(std::sync::Arc::ptr_eq(&detached, &fixture.service));
            assert!(mount.selected_identity().is_none());
            assert!(
                mount
                    .detach_interrupted_exit_service(close.ticket, cx)
                    .unwrap()
                    .is_none()
            );
            assert!(mount.detach_interrupted_exit_service(stale, cx).is_err());
            assert_eq!(
                mount.contribution().unwrap().entity_id(),
                composer.entity_id()
            );
            detached
        })
    });
    assert_eq!(std::sync::Arc::strong_count(&fixture.service), references);
    cx.update(|window, app| {
        fixture.mount.update(app, |mount, cx| {
            assert!(
                mount
                    .publish_autosave_interval(
                        1,
                        beryl_app::composer_host::ComposerHostAutosaveInterval::new(30).unwrap(),
                        window,
                        cx,
                    )
                    .is_err()
            );
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
    drop(detached);
    assert_eq!(
        std::sync::Arc::strong_count(&fixture.service),
        references - 1
    );
    drop(input);
    drop(composer);
    cx.update(|window, _| window.remove_window());
    let support::Mounted {
        root,
        mount,
        service,
        store,
        storage,
        assets,
        seals,
        directory,
    } = fixture;
    service.test_with_close_slot_locked(|| {
        drop((root, mount));
        cx.run_until_parked();
    });
    assert!(service.test_window_close_is_current(close.ticket));
    drop((service, store, storage, assets, seals));
    cx.run_until_parked();
    directory.close().unwrap();
}
