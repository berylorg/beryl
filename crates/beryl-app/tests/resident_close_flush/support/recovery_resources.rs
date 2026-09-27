use super as support;
use super::{drive, drive_until};
use beryl_app::{
    cas_projection::NativeLineageRecoveryControl,
    composer_host::ComposerHostMutationAdmissionFailure,
    main_window::{
        MainWindowConversationComposerCloseAdvance as Advance,
        MainWindowConversationComposerCloseTicket as Ticket,
    },
};
use gpui::{EntityInputHandler, TestAppContext};
use std::{cell::Cell, num::NonZeroUsize, rc::Rc, sync::Arc};

pub fn transfer_resources(cx: &mut TestAppContext, prior_handoff: bool) {
    let calls = Rc::new(Cell::new(0));
    let captured = calls.clone();
    let (fixture, cx) = support::mounted_with_configurator(
        cx,
        "resident-resource-handoff",
        184,
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
    let captured = calls.clone();
    let failure = Arc::new(ComposerHostMutationAdmissionFailure::Cancelled);
    composer.update(cx, |composer, _| {
        composer.test_set_clipboard_writer(Box::new(move |_, _| {
            captured.set(captured.get() + 1);
            gpui_text_input::ClipboardWriteOutcome::Written
        }));
        composer.test_set_mutation_admission_failure(failure.clone());
    });
    fixture.mount.update(cx, |mount, cx| {
        mount.attach_native_lineage_recovery(
            NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap()),
            cx,
        );
    });
    let calls_before = calls.get();
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
                mount.detach_interrupted_exit_resources(close.ticket, cx)
            })
            .is_err()
    );
    drive_until(cx, "bundle close becomes ready", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == Advance::Ready
    });
    drive_until(cx, "bundle editor settles", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    assert!(
        fixture
            .mount
            .update(cx, |mount, cx| {
                mount.detach_interrupted_exit_resources(close.ticket, cx)
            })
            .is_err()
    );
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
    let selection = composer.read_with(cx, |composer, _| composer.selection_identity());
    let seed = composer.read_with(cx, |composer, _| {
        composer.recovery_snapshot().unwrap().restoration().clone()
    });
    let stale = Ticket::for_test(fixture.mount.entity_id(), 99, selection);
    let worker = fixture.mount.read_with(cx, |mount, _| {
        mount.test_window_close_cleanup(std::future::pending())
    });
    let references = Arc::strong_count(&fixture.service);
    fixture.service.test_with_close_slot_locked(|| {
        fixture.mount.update(cx, |mount, cx| {
            assert!(mount.detach_interrupted_exit_resources(stale, cx).is_err());
            assert!(
                mount
                    .detach_interrupted_exit_resources(close.ticket, cx)
                    .is_err()
            );
        });
    });
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.selected_identity()),
        Some(selection)
    );
    assert_eq!(Arc::strong_count(&fixture.service), references);
    assert_eq!(Rc::strong_count(&calls), 3);
    assert_eq!(Arc::strong_count(&failure), 2);
    drop(worker);
    let prior_writer = if prior_handoff {
        composer
            .update(cx, |composer, cx| {
                composer.detach_recovery_clipboard_writer(close.ticket, cx)
            })
            .unwrap()
    } else {
        None
    };
    let resources = fixture.service.test_with_close_slot_locked(|| {
        fixture.mount.update(cx, |mount, cx| {
            assert!(mount.detach_interrupted_exit_resources(stale, cx).is_err());
            let resources = mount
                .detach_interrupted_exit_resources(close.ticket, cx)
                .unwrap()
                .unwrap();
            assert!(
                mount
                    .detach_interrupted_exit_resources(close.ticket, cx)
                    .unwrap()
                    .is_none()
            );
            assert!(mount.detach_interrupted_exit_resources(stale, cx).is_err());
            assert!(mount.selected_identity().is_none());
            resources
        })
    });
    assert!(Arc::ptr_eq(
        resources.service.as_ref().unwrap(),
        &fixture.service
    ));
    assert!(Arc::ptr_eq(
        resources.resident.service.as_ref().unwrap(),
        &fixture.service
    ));
    assert!(Arc::ptr_eq(
        resources.resident.mutation_failure.as_ref().unwrap(),
        &failure
    ));
    assert_eq!(resources.resident.clipboard_writer.is_none(), prior_handoff);
    assert!(resources.publication_adapters.is_some());
    assert!(resources.configurator.is_some());
    assert!(resources.submission_source.is_some());
    assert!(resources.native_lineage_control.is_some());
    assert_eq!(Rc::strong_count(&calls), 3);
    assert_eq!(Arc::strong_count(&fixture.service), references - 1);
    cx.update(|window, app| {
        fixture.mount.update(app, |mount, cx| {
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
        });
    });
    cx.simulate_keystrokes("x ctrl-z ctrl-v");
    drive(cx, 12);
    assert_eq!(calls.get(), calls_before);
    assert_eq!(
        input
            .update(cx, |input, _| input.export_restoration(Some(
                selection.binding().range_history_frontier()
            )))
            .unwrap(),
        seed
    );
    composer.read_with(cx, |composer, _| {
        assert_eq!(composer.recovery_snapshot().unwrap().restoration(), &seed);
        assert_eq!(composer.gpui_input().entity_id(), input.entity_id());
        assert!(!composer.test_widget_released());
    });
    assert!(fixture.service.test_window_close_is_current(close.ticket));
    drop(resources);
    assert_eq!(Rc::strong_count(&calls), if prior_handoff { 2 } else { 1 });
    assert_eq!(Arc::strong_count(&fixture.service), references - 3);
    assert_eq!(Arc::strong_count(&failure), 1);
    drop(prior_writer);
    assert_eq!(Rc::strong_count(&calls), 1);
    drop((input, composer));
    support::finish(fixture, cx);
}
