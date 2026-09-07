use super::*;

#[gpui::test]
fn pending_edit_yields_then_submits_the_newest_same_editor_candidate(
    cx: &mut gpui::TestAppContext,
) {
    pending_submission(cx, false);
}

#[gpui::test]
fn pending_page_flight_yields_then_submits_without_duplicate_admission(
    cx: &mut gpui::TestAppContext,
) {
    pending_submission(cx, true);
}

fn pending_submission(cx: &mut gpui::TestAppContext, page_only: bool) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "submission-start-progress", 191);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let original = fixture.service.selected_identity().unwrap();
    let gate = if page_only {
        fixture.service.test_block_next_selected_page_dispatch()
    } else {
        fixture.service.test_block_next_selected_dispatch()
    };
    let text = "newest admitted editor content";
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.focus(window);
            input.replace_and_mark_text_in_range(None, text, None, window, cx);
        })
    });
    drive_until(cx, 512, "held editor flight", |_| gate.is_blocked());
    assert!(composer.read_with(cx, |composer, _| composer.test_has_active_flight()));
    let selection = fixture.service.selected_identity().unwrap();
    begin(&fixture.mount, selection, cx).unwrap();
    let generation = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_start_generation());
    assert_preparing(&fixture.mount, cx);
    begin(&fixture.mount, selection, cx).unwrap();
    drive(cx, 32);
    assert_preparing(&fixture.mount, cx);
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.test_submission_start_generation()),
        generation
    );
    assert_no_host_submission(&fixture.service);

    let submission_gate = fixture
        .mount
        .update(cx, |mount, _| mount.test_block_next_submission_advance());
    gate.release();
    drive_until(cx, 1024, "submission after editor completion", |_| {
        submission_gate.is_blocked()
    });
    let admitted = fixture.service.selected_identity().unwrap();
    assert_eq!(admitted.window_id(), original.window_id());
    assert_eq!(admitted.claim(), original.claim());
    assert_eq!(
        admitted.binding().host_generation(),
        original.binding().host_generation()
    );
    assert_eq!(
        admitted.binding().candidate().session_id(),
        original.binding().candidate().session_id()
    );
    assert_eq!(
        admitted.binding().presentation_generation(),
        original.binding().presentation_generation()
    );
    assert_ne!(admitted.binding().root(), original.binding().root());
    assert_eq!(
        admitted.binding().logical_extent().logical_utf8_bytes(),
        text.len() as u64
    );
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        None
    );
    let ticket = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_advance_token())
        .unwrap();
    apply_start(&fixture.mount, generation, cx).unwrap();
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.test_submission_advance_token()),
        Some(ticket)
    );
    submission_gate.release();
    drive_until(cx, 2048, "newest candidate submitted", |cx| {
        fixture
            .service
            .selected_identity()
            .is_some_and(|current| current.binding().presentation_generation().get() == 2)
            && fixture
                .mount
                .read_with(cx, |mount, _| mount.submission_status())
                == MainWindowConversationComposerSubmissionStatus::Idle
    });
    let successor = fixture.service.selected_identity().unwrap();
    assert_ne!(
        successor.binding().candidate().draft_id(),
        original.binding().candidate().draft_id()
    );
    assert_eq!(successor.binding().logical_extent().logical_utf8_bytes(), 0);
    apply_start(&fixture.mount, generation, cx).unwrap();
    assert_eq!(fixture.service.selected_identity(), Some(successor));
    assert_no_host_submission(&fixture.service);
    drop((composer, input));
    finish(fixture, cx);
}

#[gpui::test]
fn terminal_editor_error_settles_submission_start_and_keeps_the_editor_unavailable(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "submission-start-error", 201);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let selection = fixture.service.selected_identity().unwrap();
    composer.update(cx, |composer, cx| {
        composer.test_set_terminal_error("editor terminal failure".to_owned(), cx)
    });
    assert!(begin(&fixture.mount, selection, cx).is_err());
    drive(cx, 32);
    let diagnostics = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_diagnostics());
    assert_eq!(
        diagnostics.status(),
        MainWindowConversationComposerSubmissionStatus::Failed
    );
    assert!(!diagnostics.active_task());
    assert!(!diagnostics.active_ticket());
    assert!(!diagnostics.prepared_request());
    assert_eq!(fixture.service.selected_identity(), Some(selection));
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        Some("editor terminal failure".to_owned())
    );
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    assert_no_host_submission(&fixture.service);
    drop((composer, input));
    finish(fixture, cx);
}

#[gpui::test]
fn cancelled_waits_release_the_gate_and_late_starts_cannot_admit_or_replace_another_attempt(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "submission-start-cancel", 211);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let selection = fixture.service.selected_identity().unwrap();
    let gate = fixture.service.test_block_next_selected_dispatch();
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_and_mark_text_in_range(None, "preserved edit", None, window, cx);
        })
    });
    drive_until(cx, 512, "held edit before cancellation", |_| {
        gate.is_blocked()
    });
    begin(&fixture.mount, selection, cx).unwrap();
    let first = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_start_generation());
    assert_preparing(&fixture.mount, cx);
    assert!(
        fixture
            .mount
            .update(cx, |mount, _| mount.test_cancel_submission_start())
    );
    drive_until(cx, 64, "cancelled start", |cx| {
        fixture
            .mount
            .read_with(cx, |mount, _| mount.submission_status())
            == MainWindowConversationComposerSubmissionStatus::Cancelled
    });
    assert!(input.read_with(cx, |input, _| input.is_enabled()));
    assert_no_host_submission(&fixture.service);
    apply_start(&fixture.mount, first, cx).unwrap();
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.submission_status()),
        MainWindowConversationComposerSubmissionStatus::Cancelled
    );
    begin(&fixture.mount, selection, cx).unwrap();
    let second = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_start_generation());
    assert_eq!(second, first + 1);
    apply_start(&fixture.mount, first, cx).unwrap();
    assert_preparing(&fixture.mount, cx);
    assert!(
        fixture
            .mount
            .update(cx, |mount, _| mount.test_cancel_submission_start())
    );
    drive_until(cx, 64, "second cancelled start", |cx| {
        fixture
            .mount
            .read_with(cx, |mount, _| mount.submission_status())
            == MainWindowConversationComposerSubmissionStatus::Cancelled
    });
    gate.release();
    drive_until(cx, 512, "edit preserved after cancellation", |cx| {
        fixture
            .service
            .selected_identity()
            .is_some_and(|current| current.binding().logical_extent().logical_utf8_bytes() == 14)
            && input.read_with(cx, |input, _| input.is_semantically_quiescent())
    });
    assert_no_host_submission(&fixture.service);
    assert!(!fixture.mount.read_with(cx, |mount, _| {
        mount.test_submission_diagnostics().active_task()
    }));
    drop((composer, input));
    finish(fixture, cx);
}

#[gpui::test]
fn dropping_a_start_wait_cancels_the_timer_without_admitting_submission(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "submission-start-drop", 221);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let selection = fixture.service.selected_identity().unwrap();
    let gate = fixture.service.test_block_next_selected_dispatch();
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_and_mark_text_in_range(None, "retained", None, window, cx);
        })
    });
    drive_until(cx, 512, "held edit before drop", |_| gate.is_blocked());
    begin(&fixture.mount, selection, cx).unwrap();
    assert_preparing(&fixture.mount, cx);
    let weak = fixture.mount.downgrade();
    let MountedSubmissionFixture {
        _directory,
        root,
        mount,
        service,
    } = fixture;
    root.update(cx, |root, _| root.mount = None);
    drop(mount);
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
    gate.release();
    drive_until(cx, 512, "editor flight after mount drop", |cx| {
        !composer.read_with(cx, |composer, _| composer.test_has_active_flight())
    });
    assert_no_host_submission(&service);
    assert_eq!(
        service
            .selected_identity()
            .unwrap()
            .binding()
            .candidate()
            .draft_id(),
        selection.binding().candidate().draft_id()
    );
    cx.update(|window, _| window.remove_window());
    drop((composer, input, root, service));
    cx.run_until_parked();
    _directory.close().unwrap();
}

fn begin(
    mount: &Entity<MainWindowConversationComposerMount>,
    selection: beryl_app::main_window::MainWindowComposerSelectionIdentity,
    cx: &mut gpui::VisualTestContext,
) -> Result<(), String> {
    cx.update(|window, app| {
        mount.update(app, |mount, cx| {
            mount.test_begin_submission_start(selection, window, cx)
        })
    })
}

fn apply_start(
    mount: &Entity<MainWindowConversationComposerMount>,
    generation: u64,
    cx: &mut gpui::VisualTestContext,
) -> Result<(), String> {
    cx.update(|window, app| {
        mount.update(app, |mount, cx| {
            mount.test_apply_late_submission_start(generation, window, cx)
        })
    })
}

fn assert_preparing(
    mount: &Entity<MainWindowConversationComposerMount>,
    cx: &mut gpui::VisualTestContext,
) {
    let diagnostics = mount.read_with(cx, |mount, _| mount.test_submission_diagnostics());
    assert_eq!(
        diagnostics.status(),
        MainWindowConversationComposerSubmissionStatus::Preparing
    );
    assert!(diagnostics.active_task());
    assert!(!diagnostics.active_ticket());
    assert!(!diagnostics.prepared_request());
}

fn assert_no_host_submission(service: &MainWindowConversationComposerService) {
    let diagnostics = service.test_submission_diagnostics().unwrap();
    let submission = diagnostics.selected_submission().unwrap();
    assert!(!submission.pending());
    assert_eq!(submission.retained_roots(), 0);
    assert_eq!(submission.retained_materializations(), 0);
    assert!(!diagnostics.submission_successor_reserved());
    assert!(!diagnostics.pending_activation_reserved());
}

fn finish(fixture: MountedSubmissionFixture, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, _| window.remove_window());
    let MountedSubmissionFixture {
        _directory,
        root,
        mount,
        service,
    } = fixture;
    drop((root, mount, service));
    cx.run_until_parked();
    _directory.close().unwrap();
}
