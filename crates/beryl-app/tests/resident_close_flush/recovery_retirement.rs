use super::support::{self, drive_until};
use beryl_app::main_window::MainWindowConversationComposerCloseAdvance as Advance;
use gpui::TestAppContext;
use std::{cell::Cell, rc::Rc, sync::Arc};

#[gpui::test]
fn detached_retirement_releases_adapters_and_retries_without_releasing_the_editor(
    cx: &mut TestAppContext,
) {
    let calls = Rc::new(Cell::new(0));
    let captured = calls.clone();
    let (fixture, cx) = support::mounted_with_configurator(
        cx,
        "detached-resident-retirement",
        185,
        Box::new(move |selection| {
            captured.set(captured.get() + 1);
            support::configure(selection)
        }),
    );
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
    drive_until(cx, "retirement close becomes ready", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == Advance::Ready
    });
    drive_until(cx, "retirement editor settles", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    composer
        .update(cx, |composer, cx| {
            composer.test_set_shutdown_interaction_gated(true, cx)
        })
        .unwrap();
    let mut resources = fixture.mount.update(cx, |mount, cx| {
        assert!(
            mount
                .fence_interrupted_exit_resident(close.ticket, cx)
                .unwrap()
        );
        mount
            .detach_interrupted_exit_resources(close.ticket, cx)
            .unwrap()
            .unwrap()
    });
    let seed = composer.read_with(cx, |composer, _| {
        composer.recovery_snapshot().unwrap().restoration().clone()
    });
    let selection = composer.read_with(cx, |composer, _| composer.selection_identity());
    let calls_before = calls.get();
    let mount_service = resources.service.take();
    let resident_service = resources.resident.service.take();
    resources = resources.retire().err().unwrap();
    assert!(resources.configurator.is_some());
    assert_eq!(Rc::strong_count(&calls), 2);
    resources.service = mount_service;
    resources.resident.service = resident_service;

    let foreign = super::widget_support::fixture::Fixture::new("foreign-retirement-service", 186);
    let foreign_service = Arc::new(
        beryl_app::main_window::MainWindowConversationComposerService::new(
            foreign.store.service_reference(),
            *support::slot_close::slot(&foreign),
        ),
    );
    let resident_service = resources.resident.service.replace(foreign_service.clone());
    resources = resources.retire().err().unwrap();
    assert!(Arc::ptr_eq(
        resources.resident.service.as_ref().unwrap(),
        &foreign_service
    ));
    assert!(resources.configurator.is_some());
    assert_eq!(Rc::strong_count(&calls), 2);
    resources.resident.service = resident_service;
    drop((foreign_service, foreign));

    resources = fixture
        .service
        .test_with_close_slot_locked(|| resources.retire().err().unwrap());
    assert!(Arc::ptr_eq(
        resources.service.as_ref().unwrap(),
        &fixture.service
    ));
    assert!(resources.resident.service.is_none());
    assert!(resources.resident.clipboard_writer.is_none());
    assert!(resources.resident.mutation_failure.is_none());
    assert!(resources.publication_adapters.is_none());
    assert!(resources.configurator.is_none());
    assert!(resources.submission_source.is_none());
    assert!(resources.native_lineage_control.is_none());
    assert_eq!(Rc::strong_count(&calls), 1);
    assert_eq!(calls.get(), calls_before);
    let home_reference = fixture.service.test_retain_home_reference();
    let weak = Arc::downgrade(&fixture.service);
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
    drop(service);
    resources = resources.retire().err().unwrap();
    assert!(weak.upgrade().is_some());
    drop(weak);
    resources = resources.retire().err().unwrap();
    assert!(
        resources
            .service
            .as_ref()
            .unwrap()
            .test_window_close_is_current(close.ticket)
    );
    drop(home_reference);
    let retired = resources.retire().ok().unwrap();
    assert_eq!(retired.close_ticket(), close.ticket);
    assert_eq!(retired.selection(), selection);
    composer.read_with(cx, |composer, _| {
        let snapshot = composer.recovery_snapshot().unwrap();
        assert_eq!(retired.host().close_ticket(), snapshot.flush_ticket());
        assert_eq!(snapshot.restoration(), &seed);
        assert_eq!(composer.gpui_input().entity_id(), input.entity_id());
        assert!(!composer.test_widget_released());
    });
    cx.simulate_keystrokes("x ctrl-z ctrl-v");
    support::drive(cx, 12);
    assert_eq!(
        input
            .update(cx, |input, _| input.export_restoration(Some(
                selection.binding().range_history_frontier()
            )))
            .unwrap(),
        seed
    );
    cx.update(|window, app| {
        mount.update(app, |mount, cx| {
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
            assert_eq!(
                mount.contribution().unwrap().entity_id(),
                composer.entity_id()
            );
            window.remove_window();
        })
    });
    drop((input, composer, root, mount, store, storage, assets, seals));
    cx.run_until_parked();
    directory.close().unwrap();
}
