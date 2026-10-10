use super::*;
use beryl_app::cas_projection::SubmissionUnavailable;

#[gpui::test]
fn final_writer_noncommit_paints_rejection_after_stale_ticket_cleanup(
    cx: &mut gpui::TestAppContext,
) {
    use beryl_home_store::{CommandOutcome, HomeCommand};
    use syndic_storage::{
        InputGateRecord,
        test_faults::{FixtureBatch, FixtureRecord},
    };
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "writer-rejection", 238);
    let (composer, retained) =
        prepare_flushed_submission(cx, &fixture.mount, &fixture.service, "retain");
    let thread = retained.claim().thread_id();
    fixture.service.test_with_selected_host(|host| {
        host.test_arm_submission_before_execute_fault(move |store, storage| {
            let gate = storage
                .input_gate(store, thread, SyndicPointReadLimit::new(1_000_000).unwrap())
                .unwrap()
                .unwrap();
            let updated = InputGateRecord::new(
                thread,
                gate.revision().checked_next().unwrap(),
                gate.state().clone(),
                gate.accepted_high_water(),
                gate.route_generation_high_water(),
                gate.selected_route(),
                gate.live_steering_count(),
                gate.live_next_turn_count(),
                gate.live_logical_utf8_bytes(),
            )
            .unwrap();
            let mut batch = FixtureBatch::new();
            batch.put(FixtureRecord::InputGate(updated)).unwrap();
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(storage.fixture_contribution(storage.revision(store).unwrap(), batch))
                .unwrap();
            assert!(matches!(
                store.execute(command),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
        });
    });
    cx.simulate_keystrokes("enter");
    drive_until(cx, 512, "final writer rejection cleanup", |cx| {
        fixture
            .mount
            .read_with(cx, |mount, _| mount.submission_status())
            == MainWindowConversationComposerSubmissionStatus::NotCommitted
            && !fixture.mount.read_with(cx, |mount, _| {
                mount.test_submission_diagnostics().active_ticket()
            })
    });
    drive(cx, 16);
    assert_eq!(fixture.service.selected_identity(), Some(retained));
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.submission_disabled_outcome()),
        Some(SubmissionUnavailable::AuthorityUnavailable)
    );
    let (reason, bounds) = composer
        .read_with(cx, |composer, _| {
            composer.test_submission_outcome_rendered()
        })
        .unwrap();
    assert_eq!(reason, SubmissionUnavailable::AuthorityUnavailable);
    assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
    assert!(composer.read_with(cx, |composer, app| {
        composer
            .gpui_input()
            .read(app)
            .is_surface_current_and_interactive()
    }));
}

#[gpui::test]
fn disabled_enter_reports_the_owner_reason_preserves_editor_and_keeps_autosave_writable(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "disabled-submission", 237);
    let (composer, retained) =
        prepare_flushed_submission(cx, &fixture.mount, &fixture.service, "retained draft");
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.simulate_keystrokes("ctrl-a");
    drive(cx, 16);
    let selection = input.read_with(cx, |input, _| input.surface().unwrap().source_selection());
    assert_ne!(selection.anchor, selection.head);
    for reason in [
        SubmissionUnavailable::RepairRequired,
        SubmissionUnavailable::BindingUnavailable,
        SubmissionUnavailable::RuntimeUnavailable,
        SubmissionUnavailable::AuthorityUnavailable,
    ] {
        composer.update(cx, |composer, cx| {
            composer.set_submission_availability(Err(reason), cx)
        });
        cx.simulate_keystrokes("enter");
        drive(cx, 16);
        assert!(
            cx.debug_bounds("conversation-composer-submission-outcome")
                .is_some()
        );
        let (rendered, bounds) = composer
            .read_with(cx, |composer, _| {
                composer.test_submission_outcome_rendered()
            })
            .unwrap();
        assert_eq!(rendered, reason);
        assert!(bounds.size.width > gpui::px(0.) && bounds.size.height > gpui::px(0.));
        assert_eq!(
            composer.read_with(cx, |composer, _| composer.submission_disabled_outcome()),
            Some(reason)
        );
        assert_eq!(fixture.service.selected_identity(), Some(retained));
        assert_eq!(
            fixture
                .mount
                .read_with(cx, |mount, _| mount.submission_status()),
            MainWindowConversationComposerSubmissionStatus::Idle
        );
        assert!(!fixture.mount.read_with(cx, |mount, _| {
            mount.test_submission_diagnostics().active_ticket()
        }));
        assert!(input.read_with(cx, |input, _| input.is_surface_current_and_interactive()));
        assert_eq!(
            input.read_with(cx, |input, _| input.surface().unwrap().source_selection()),
            selection
        );
    }
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_and_mark_text_in_range(None, " more", None, window, cx)
        })
    });
    drive_until(cx, 256, "editing while submission disabled", |_| {
        fixture
            .service
            .selected_identity()
            .is_some_and(|identity| identity.binding() != retained.binding())
    });
    cx.executor().advance_clock(Duration::from_secs(5));
    drive_until(cx, 512, "disabled submission autosave", |cx| {
        fixture
            .mount
            .read_with(cx, |mount, _| mount.autosave_diagnostics().phase())
            == MainWindowConversationComposerAutosavePhase::Idle
    });
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.submission_unavailable()),
        Some(SubmissionUnavailable::AuthorityUnavailable)
    );
    let edited_extent = fixture
        .service
        .selected_identity()
        .unwrap()
        .binding()
        .logical_extent();
    cx.simulate_keystrokes("ctrl-z");
    drive_until(cx, 256, "undo while submission disabled", |_| {
        fixture.service.selected_identity().is_some_and(|identity| {
            identity.binding().logical_extent() == retained.binding().logical_extent()
        })
    });
    assert_eq!(
        input.read_with(cx, |input, _| input.surface().unwrap().source_selection()),
        selection
    );
    cx.simulate_keystrokes("ctrl-y");
    drive_until(cx, 256, "redo while submission disabled", |_| {
        fixture
            .service
            .selected_identity()
            .is_some_and(|identity| identity.binding().logical_extent() == edited_extent)
    });
    composer.update(cx, |composer, cx| {
        composer.set_submission_availability(Ok(()), cx)
    });
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.submission_disabled_outcome()),
        None
    );
    assert!(input.read_with(cx, |input, _| input.is_surface_current_and_interactive()));
}
