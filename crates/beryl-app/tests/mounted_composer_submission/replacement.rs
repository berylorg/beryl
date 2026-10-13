use super::*;
#[path = "../historical_replacement_host/support.rs"]
mod history;
#[path = "../syndic_composer_host/support.rs"]
mod host_support;
use syndic_storage::{ComposerAtom, ComposerPayload, DraftSubmissionIntent};

#[gpui::test]
fn replacement_enter_below_reserve_preserves_the_edit_and_selected_path(
    cx: &mut gpui::TestAppContext,
) {
    denied_replacement(cx, false);
}

#[gpui::test]
fn replacement_enter_with_unavailable_free_space_preserves_the_edit_and_selected_path(
    cx: &mut gpui::TestAppContext,
) {
    denied_replacement(cx, true);
}

fn denied_replacement(cx: &mut gpui::TestAppContext, unavailable: bool) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("mounted-replacement-admission-denial", 241);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let faults = fixture.faults.clone();
    let assets = fixture.assets();
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(assets.clone());
    let marker_seals = fixture.marker_seals();
    let target = history::seed_source(
        &fixture.store,
        &fixture.storage,
        thread,
        &ComposerPayload::new(vec![ComposerAtom::text("historical input").unwrap()]).unwrap(),
    );
    let mut host = fixture.activated_host(thread, 211, 212, 1);
    let ticket = host
        .begin_historical_replacement(
            &fixture.store,
            host.binding().unwrap(),
            target,
            history::operation(221),
        )
        .unwrap();
    let completion = history::finish(&mut host, &fixture.store, &assets, ticket);
    host.acknowledge_historical_replacement(ticket, completion.binding())
        .unwrap();
    let (directory, store, storage) = fixture.into_store();
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage.clone(), marker_authority)
            .unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        store.service_reference(),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(configure),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount: Some(mount) }
    });
    drive(cx, 16);
    let mount = root.read_with(cx, |root, _| root.mount.clone().unwrap());
    let composer = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let imported = service.selected_identity().unwrap();
    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.focus(window);
            input.replace_and_mark_text_in_range(
                None,
                " edited replacement",
                None,
                window,
                input_cx,
            )
        })
    });
    drive_until(cx, 512, "replacement edit", |_| {
        service
            .selected_identity()
            .is_some_and(|selection| selection != imported)
    });
    cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.publish_autosave_interval(
                1,
                ComposerHostAutosaveInterval::new(5).unwrap(),
                window,
                mount_cx,
            )
        })
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_secs(5));
    drive_until(cx, 512, "replacement edit publication", |cx| {
        mount.read_with(cx, |mount, _| mount.autosave_diagnostics().phase())
            == MainWindowConversationComposerAutosavePhase::Idle
    });
    let retained = service.selected_identity().unwrap();
    let before = input.read_with(cx, |input, _| {
        input
            .export_restoration(Some(input.history_frontier()))
            .unwrap()
    });
    let draft_before = host_support::current(storage.clone(), &store, thread);
    assert!(matches!(
        draft_before.draft().submission_intent(),
        DraftSubmissionIntent::Replacement(_)
    ));
    assert_eq!(
        storage
            .thread(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(target.turn_id)
    );
    faults.push_free_space_observation(if unavailable {
        FreeSpaceTestObservation::Unavailable
    } else {
        FreeSpaceTestObservation::Observed {
            available_bytes: 0,
            total_free_bytes: 0,
            total_bytes: u64::MAX,
        }
    });
    composer.update(cx, |composer, cx| {
        composer.set_submission_availability(Ok(()), cx)
    });
    cx.simulate_keystrokes("enter");
    drive_until(cx, 2_048, "replacement direct admission denial", |cx| {
        matches!(
            mount.read_with(cx, |mount, _| mount.submission_status()),
            MainWindowConversationComposerSubmissionStatus::DirectAdmissionDenied(_)
        )
    });
    let status = mount.read_with(cx, |mount, _| mount.submission_status());
    if unavailable {
        assert_eq!(
            status,
            MainWindowConversationComposerSubmissionStatus::DirectAdmissionDenied(
                FreeSpaceOutcome::Unavailable
            )
        );
    } else {
        assert!(matches!(
            status,
            MainWindowConversationComposerSubmissionStatus::DirectAdmissionDenied(
                FreeSpaceOutcome::BelowReserve { .. }
            )
        ));
    }
    assert_eq!(faults.free_space_observation_count(), 1);
    assert_eq!(service.selected_identity(), Some(retained));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        composer.entity_id()
    );
    input.read_with(cx, |input, _| {
        assert!(input.is_surface_current_and_interactive());
        assert_eq!(
            input
                .export_restoration(Some(input.history_frontier()))
                .unwrap(),
            before
        );
    });
    let draft_after = host_support::current(storage.clone(), &store, thread);
    assert_eq!(
        draft_after.draft().root_history(),
        draft_before.draft().root_history()
    );
    assert_eq!(
        draft_after.draft().submission_intent(),
        draft_before.draft().submission_intent()
    );
    assert_eq!(
        storage
            .thread(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(target.turn_id)
    );
    assert!(
        storage
            .accepted_input(
                &store,
                retained
                    .binding()
                    .candidate()
                    .draft_id()
                    .accepted_input_id(),
                SyndicPointReadLimit::new(65_536).unwrap()
            )
            .unwrap()
            .is_none()
    );
    cx.update(|window, _| window.remove_window());
    drop((root, mount, composer, input, service, assets, storage));
    cx.run_until_parked();
    drop(store);
    directory.close().unwrap();
}
