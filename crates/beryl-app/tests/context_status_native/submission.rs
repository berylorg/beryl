use super::*;
use gpui::EntityInputHandler;
use syndic_storage::{
    InputGateState, RepairCaptureGap, RepairCaptureGapReason, RepairRequestDisposition,
    RepairRequiredTarget, RequireTerminalRepair, SourceEventPayload, SourceEventSequence,
    TurnEndStatus, TurnIncompleteReason, TurnTerminalOutcome,
};

#[derive(Clone, Copy)]
enum Case {
    Repair,
    Incomplete,
    Unknown,
}

#[test]
fn native_repair_disabled_enter_preserves_writable_draft_and_exact_reason() {
    run(Case::Repair);
}

#[test]
fn native_incomplete_without_usable_binding_preserves_writable_draft() {
    run(Case::Incomplete);
}

#[test]
fn native_unknown_terminal_without_usable_binding_preserves_writable_draft() {
    run(Case::Unknown);
}

fn limit() -> syndic_storage::SyndicPointReadLimit {
    syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn prepare(fixture: &mut syndic::Fixture, case: Case, submitted: syndic::SubmittedTurn) {
    let source = fixture.activate_without_terminal(submitted);
    let outcome = match case {
        Case::Unknown => TurnTerminalOutcome::UnknownTerminal,
        Case::Incomplete => TurnTerminalOutcome::Incomplete,
        Case::Repair => TurnTerminalOutcome::Interrupted,
    };
    let status = TurnEndStatus::new(outcome, Some(TurnIncompleteReason::StreamLost)).unwrap();
    fixture.publish_terminal_without_convergence(submitted, &source, status);
    if matches!(case, Case::Repair) {
        let home = fixture.home();
        let state = fixture
            .storage
            .turn_state(&home, submitted.turn, limit())
            .unwrap()
            .unwrap();
        let terminal = fixture
            .storage
            .source_event(
                &home,
                submitted.turn,
                SourceEventSequence::new(state.source_event_count()).unwrap(),
                limit(),
            )
            .unwrap()
            .unwrap();
        assert!(matches!(
            terminal.payload(),
            SourceEventPayload::TurnEnded(_)
        ));
        let target = RepairRequiredTarget::new(
            submitted.turn,
            source,
            RepairCaptureGap::new(
                terminal.repair_witness(),
                status,
                RepairCaptureGapReason::ForcedAbortOrderingUnproven,
                None,
            )
            .unwrap(),
            RepairRequestDisposition::Available,
        );
        let gate = fixture
            .storage
            .input_gate(&home, fixture.thread, limit())
            .unwrap()
            .unwrap();
        clean(
            home.execute_current(fixture.storage.current_require_terminal_repair(
                RequireTerminalRepair::new(fixture.thread, gate.revision(), target),
            )),
        );
    }
}

fn clean(outcome: CommandOutcome) {
    assert!(matches!(
        outcome,
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn composer(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &AsyncApp,
) -> gpui::Entity<MainWindowConversationComposer> {
    window
        .read_with(cx, |root, app| root.test_submission_composer(app).unwrap())
        .unwrap()
}

pub(super) async fn submit_native_text(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    text: &str,
    cx: &mut AsyncApp,
) -> MainWindowComposerSelectionIdentity {
    wait_availability(window, Ok(()), cx).await;
    let editor = composer(window, cx);
    let input = editor
        .read_with(cx, |editor, _| editor.gpui_input())
        .unwrap();
    window
        .update(cx, |_, window, app| {
            input.update(app, |input, app| {
                input.focus(window);
                input.replace_and_mark_text_in_range(None, text, None, window, app);
            });
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while editor
        .read_with(cx, |editor, _| {
            editor
                .selection_identity()
                .binding()
                .logical_extent()
                .logical_utf8_bytes()
        })
        .unwrap()
        != text.len() as u64
    {
        assert!(
            Instant::now() < deadline,
            "native submission edit did not settle"
        );
        native::pump(cx).await;
    }
    wait_availability(window, Ok(()), cx).await;
    let identity = editor
        .read_with(cx, |editor, _| editor.selection_identity())
        .unwrap();
    cx.update_window(window.into(), |_, window, app| {
        window.dispatch_input_for_test(
            gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse("enter").unwrap(),
                is_held: false,
            }),
            app,
        );
    })
    .unwrap();
    loop {
        let current = composer(window, cx)
            .read_with(cx, |editor, _| editor.selection_identity())
            .unwrap();
        if current.binding().candidate().draft_id() != identity.binding().candidate().draft_id() {
            assert_eq!(current.binding().logical_extent().logical_utf8_bytes(), 0);
            return identity;
        }
        assert!(
            Instant::now() < deadline,
            "native submission did not publish successor"
        );
        native::pump(cx).await;
    }
}

pub(super) async fn reject_retained_proof(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    stamp: (
        MainWindowComposerSelectionIdentity,
        u64,
        Option<std::sync::Weak<()>>,
    ),
    proof: SubmissionProof,
    reason: SubmissionUnavailable,
    cx: &mut AsyncApp,
) {
    let editor = composer(window, cx);
    let identity = editor
        .read_with(cx, |editor, _| editor.selection_identity())
        .unwrap();
    assert_eq!(identity, stamp.0);
    window
        .update(cx, |root, window, app| {
            root.test_apply_submission_observation(stamp, Ok(proof), window, app);
            assert_eq!(editor.read(app).submission_unavailable(), Some(reason));
            editor.read(app).gpui_input().read(app).focus(window);
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, app| {
        window.dispatch_input_for_test(
            gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse("enter").unwrap(),
                is_held: false,
            }),
            app,
        );
        window.draw_and_present_for_test(app);
    })
    .unwrap();
    window
        .read_with(cx, |root, app| {
            assert_eq!(editor.read(app).selection_identity(), identity);
            assert_eq!(editor.read(app).submission_disabled_outcome(), Some(reason));
            assert!(
                !root
                    .test_submission_mount()
                    .unwrap()
                    .read(app)
                    .test_submission_diagnostics()
                    .active_ticket()
            );
        })
        .unwrap();
    wait_painted_outcome(window, reason, Instant::now() + Duration::from_secs(10), cx).await;
    window
        .read_with(cx, |root, app| {
            assert_eq!(editor.read(app).selection_identity(), identity);
            assert_eq!(editor.read(app).submission_disabled_outcome(), Some(reason));
            let (painted, bounds) = editor.read(app).test_submission_outcome_rendered().unwrap();
            assert_eq!(painted, reason);
            assert!(bounds.size.width > gpui::px(0.) && bounds.size.height > gpui::px(0.));
            assert!(
                !root
                    .test_submission_mount()
                    .unwrap()
                    .read(app)
                    .test_submission_diagnostics()
                    .active_ticket()
            );
        })
        .unwrap();
}

pub(super) async fn wait_availability(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    expected: Result<(), SubmissionUnavailable>,
    cx: &mut AsyncApp,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    native_geometry::restore(window, cx);
    loop {
        cx.update_window(window.into(), |_, window, app| {
            window.draw_and_present_for_test(app)
        })
        .unwrap();
        let observed = window
            .read_with(cx, |root, app| {
                let composer = root.test_submission_composer(app)?;
                let editor = composer.read(app);
                let input = editor.gpui_input();
                let input = input.read(app);
                Some((
                    editor.submission_unavailable(),
                    input.is_surface_current_and_interactive(),
                    input.is_quiescent(),
                    root.test_submission_mount()
                        .and_then(|mount| mount.read(app).selected_identity())
                        == Some(editor.selection_identity()),
                    root.test_submission_observation(app).map(|_| ()),
                ))
            })
            .unwrap();
        if observed
            .as_ref()
            .is_some_and(|(reason, interactive, quiescent, synchronized, proof)| {
                *reason == expected.err()
                    && (expected.is_err()
                        || (*interactive && *quiescent && *synchronized && proof.is_ok()))
            })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native submission observation {observed:?}, expected {expected:?}"
        );
        native::pump(cx).await;
    }
}

pub(super) async fn wait_painted_outcome(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    reason: SubmissionUnavailable,
    deadline: Instant,
    cx: &mut AsyncApp,
) {
    native_geometry::restore(window, cx);
    loop {
        cx.update_window(window.into(), |_, window, app| {
            window.draw_and_present_for_test(app)
        })
        .unwrap();
        let observed = composer(window, cx)
            .read_with(cx, |editor, _| editor.test_submission_outcome_rendered())
            .unwrap();
        if observed.is_some_and(|(painted, bounds)| {
            painted == reason
                && bounds.size.width > gpui::px(0.)
                && bounds.size.height > gpui::px(0.)
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native submission outcome {reason:?} did not paint: {observed:?}"
        );
        native::pump(cx).await;
    }
}

fn run(case: Case) {
    let (fixture, prepared, acquisition, mut runtime) = support::worker(move || {
        let mut fixture = syndic::Fixture::new(198);
        eprintln!(
            "native submission fixture: {}",
            fixture.home_path().display()
        );
        let theme = appearance::system_font_appearance(&fixture.state);
        let runtime::Prepared {
            shell: prepared,
            acquisition,
            composer: _,
            runtime,
        } = runtime::prepare(&mut fixture, theme, false);
        (fixture, prepared, acquisition, runtime)
    })
    .join()
    .unwrap();
    let completed = Arc::new(AtomicBool::new(false));
    let result = completed.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            gpui_text_input::ensure_text_input_bindings(app);
            let owner = beryl_app::theme_runtime::GpuiAppearanceWindowSet::new(
                prepared.appearance().clone(),
                std::num::NonZeroUsize::new(4).unwrap(),
                app,
            );
            let mut shell = GpuiMainWindowShellHost::new(app, owner)
                .construct_hidden(prepared)
                .unwrap_or_else(|_| panic!("native submission shell"));
            let window = shell.window();
            let worker = fixture.store.exact_stop_worker();
            let publication = Arc::new(());
            window
                .update(app, |root, window, app| {
                    root.test_mount_exact_status_worker(
                        worker.clone(),
                        Arc::downgrade(&publication),
                        fixture.state.session(),
                        window,
                        app,
                    )
                })
                .unwrap();
            app.spawn(async move |cx| {
                let deadline = Instant::now() + Duration::from_secs(15);
                while !cx.update(|app| shell.ready_to_publish(app)).unwrap() {
                    assert!(
                        Instant::now() < deadline,
                        "native submission shell not ready"
                    );
                    window.update(cx, |_, window, _| window.refresh()).unwrap();
                    native::pump(cx).await;
                }
                cx.update(|app| shell.publish(app)).unwrap().unwrap();
                window
                    .update(cx, |_, window, _| window.minimize_window())
                    .unwrap();
                let (fixture, runtime) = join(
                    support::worker(move || {
                        runtime.activate(&fixture);
                        (fixture, runtime)
                    }),
                    cx,
                )
                .await;
                wait_availability(window, Ok(()), cx).await;
                let initial_editor = composer(window, cx);
                let initial_identity = initial_editor
                    .read_with(cx, |editor, _| editor.selection_identity())
                    .unwrap();
                let initial_input = initial_editor
                    .read_with(cx, |editor, _| editor.gpui_input())
                    .unwrap();
                window
                    .update(cx, |_, window, app| {
                        initial_input.update(app, |input, app| {
                            input.focus(window);
                            input.replace_and_mark_text_in_range(
                                None,
                                "retained terminal input",
                                None,
                                window,
                                app,
                            );
                        });
                    })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                while initial_editor
                    .read_with(cx, |editor, _| {
                        editor
                            .selection_identity()
                            .binding()
                            .logical_extent()
                            .logical_utf8_bytes()
                    })
                    .unwrap()
                    != 23
                {
                    assert!(
                        Instant::now() < deadline,
                        "initial native input did not settle"
                    );
                    native::pump(cx).await;
                }
                wait_availability(window, Ok(()), cx).await;
                cx.update_window(window.into(), |_, window, app| {
                    window.dispatch_input_for_test(
                        gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("enter").unwrap(),
                            is_held: false,
                        }),
                        app,
                    );
                })
                .unwrap();
                let deadline = Instant::now() + Duration::from_secs(15);
                loop {
                    let editor = composer(window, cx);
                    if editor
                        .read_with(cx, |editor, _| {
                            editor.selection_identity().binding().candidate().draft_id()
                        })
                        .unwrap()
                        != initial_identity.binding().candidate().draft_id()
                    {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "initial native submission did not publish successor"
                    );
                    native::pump(cx).await;
                }
                let fixture = join(
                    support::worker(move || {
                        let mut fixture = fixture;
                        fixture.advance_clock(
                            u64::try_from(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis(),
                            )
                            .unwrap()
                                + 1_000,
                        );
                        let turn = initial_identity
                            .binding()
                            .candidate()
                            .draft_id()
                            .submitted_turn_id();
                        let items = fixture
                            .storage
                            .turn_items(
                                &fixture.home(),
                                turn,
                                None,
                                beryl_home_store::CursorReadLimits::new(1, 1_000_000).unwrap(),
                            )
                            .unwrap();
                        let submitted = syndic::SubmittedTurn {
                            turn,
                            user_item: items.records()[0].item_id(),
                        };
                        prepare(&mut fixture, case, submitted);
                        fixture
                    }),
                    cx,
                )
                .await;
                let denial = if matches!(case, Case::Repair) {
                    SubmissionUnavailable::RepairRequired
                } else {
                    SubmissionUnavailable::BindingUnavailable
                };
                wait_availability(window, Err(denial), cx).await;
                let editor = composer(window, cx);
                window
                    .update(cx, |root, window, app| {
                        root.test_submission_mount()
                            .unwrap()
                            .update(app, |mount, app| {
                                mount.publish_autosave_interval(
                                    1,
                                    beryl_app::composer_host::ComposerHostAutosaveInterval::new(5)
                                        .unwrap(),
                                    window,
                                    app,
                                )
                            })
                            .unwrap();
                    })
                    .unwrap();
                let input = editor
                    .read_with(cx, |editor, _| editor.gpui_input())
                    .unwrap();
                window
                    .update(cx, |_, window, app| {
                        input.update(app, |input, app| {
                            input.focus(window);
                            input.replace_and_mark_text_in_range(
                                None,
                                "writable during recovery",
                                None,
                                window,
                                app,
                            );
                        });
                    })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                while editor
                    .read_with(cx, |editor, _| {
                        editor
                            .selection_identity()
                            .binding()
                            .logical_extent()
                            .logical_utf8_bytes()
                    })
                    .unwrap()
                    != 24
                {
                    assert!(
                        Instant::now() < deadline,
                        "native disabled editing did not settle"
                    );
                    native::pump(cx).await;
                }
                wait_availability(window, Err(denial), cx).await;
                let retained = editor
                    .read_with(cx, |editor, _| editor.selection_identity())
                    .unwrap();
                let label = match case {
                    Case::Repair => "repair pending",
                    Case::Incomplete => "incomplete",
                    Case::Unknown => "unknown terminal",
                };
                assert_eq!(
                    window
                        .read_with(cx, |root, _| root.test_exact_status_diagnostics().0)
                        .unwrap(),
                    label
                );
                cx.update_window(window.into(), |_, window, app| {
                    window.dispatch_input_for_test(
                        gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("enter").unwrap(),
                            is_held: false,
                        }),
                        app,
                    );
                })
                .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                wait_painted_outcome(window, denial, deadline, cx).await;
                assert_eq!(
                    editor
                        .read_with(cx, |editor, _| editor.submission_disabled_outcome())
                        .unwrap(),
                    Some(denial)
                );
                assert_eq!(
                    editor
                        .read_with(cx, |editor, _| editor.selection_identity())
                        .unwrap(),
                    retained
                );
                cx.update_window(window.into(), |_, window, app| {
                    window.draw_and_present_for_test(app)
                })
                .unwrap();
                let (rendered, bounds) = editor
                    .read_with(cx, |editor, _| editor.test_submission_outcome_rendered())
                    .unwrap()
                    .expect("native local outcome painted");
                assert_eq!(rendered, denial);
                assert!(bounds.size.width > gpui::px(0.) && bounds.size.height > gpui::px(0.));
                assert!(
                    input
                        .read_with(cx, |input, _| input.is_surface_current_and_interactive())
                        .unwrap()
                );
                let deadline = Instant::now() + Duration::from_secs(10);
                while window
                    .read_with(cx, |root, app| {
                        root.test_submission_mount()
                            .unwrap()
                            .read(app)
                            .autosave_diagnostics()
                            .phase()
                    })
                    .unwrap()
                    != MainWindowConversationComposerAutosavePhase::Idle
                {
                    assert!(
                        Instant::now() < deadline,
                        "native disabled autosave did not settle"
                    );
                    native::pump(cx).await;
                }
                let stale = window
                    .read_with(cx, |root, app| root.test_submission_observation_stamp(app))
                    .unwrap();
                let observation = window
                    .read_with(cx, |root, app| root.test_submission_observation(app))
                    .unwrap();
                drop(publication);
                wait_availability(window, Err(SubmissionUnavailable::AuthorityUnavailable), cx)
                    .await;
                window
                    .update(cx, |root, window, app| {
                        root.test_apply_submission_observation(stale, observation, window, app)
                    })
                    .unwrap();
                assert_eq!(
                    editor
                        .read_with(cx, |editor, _| editor.submission_unavailable())
                        .unwrap(),
                    Some(SubmissionUnavailable::AuthorityUnavailable)
                );
                let fixture = join(
                    support::worker(move || {
                        let home = fixture.home();
                        let current = fixture
                            .storage
                            .current_draft(&home, fixture.thread, limit())
                            .unwrap()
                            .unwrap();
                        assert_eq!(
                            current.draft().id(),
                            retained.binding().candidate().draft_id()
                        );
                        assert_eq!(
                            current.draft().piece_root().summary().logical_utf8_bytes(),
                            24
                        );
                        assert!(
                            fixture
                                .storage
                                .accepted_input(
                                    &home,
                                    current.draft().id().accepted_input_id(),
                                    limit()
                                )
                                .unwrap()
                                .is_none()
                        );
                        let gate = fixture
                            .storage
                            .input_gate(&home, fixture.thread, limit())
                            .unwrap()
                            .unwrap();
                        assert_eq!(gate.live_next_turn_count(), 0);
                        if matches!(case, Case::Repair) {
                            assert!(matches!(gate.state(), InputGateState::RepairRequired(_)));
                        }
                        drop(home);
                        fixture
                    }),
                    cx,
                )
                .await;
                window
                    .update(cx, |_, window, _| window.remove_window())
                    .unwrap();
                drop(shell);
                join(
                    support::worker(move || {
                        runtime.close();
                        drop(acquisition);
                        let (directory, service) = fixture.into_service();
                        assert!(matches!(
                            service.close().unwrap(),
                            ProjectionConnectionServiceCloseOutcome::Closed
                        ));
                        drop(directory);
                    }),
                    cx,
                )
                .await;
                result.store(true, Ordering::Release);
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
        });
    assert!(completed.load(Ordering::Acquire));
}
