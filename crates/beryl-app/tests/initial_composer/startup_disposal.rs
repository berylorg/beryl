use super::*;
use gpui::Application;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[path = "../support/shell_desktop_flight.rs"]
mod flight;
use MainWindowStartupDisposalCompletion as Completion;
use flight::{Case, native};

#[test]
fn native_acquired_startup_disposal_preserves_set_seal_and_drains_admitted_editor_work() {
    let fixtures = home_support::worker(|| {
        (0..4)
            .map(|case| {
                let seed = 201 + case * 3;
                let fixture = Fixture::new(seed);
                let mut initial = fixture.begin(seed + 1);
                initial.advance(&CommandCancellation::new()).unwrap();
                let draft = initial.acquisition().draft_id();
                let placement = initial.acquisition().placement().clone();
                let appearance = AppearanceCoordinator::new(
                    AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
                    flight::system_font_appearance(&fixture.state),
                )
                .current();
                let prepared = initial
                    .prepare(&mut config)
                    .unwrap_or_else(|failure| panic!("{}", failure.error));
                let prepared = prepared
                    .into_shell(
                        Box::new(config),
                        seals(&fixture),
                        submission(),
                        appearance.clone(),
                    )
                    .unwrap_or_else(|failure| panic!("{}", failure.error));
                let window_id = prepared.window_id();
                let geometry = prepare_windows_window_placement(window_id, placement).unwrap();
                let before = snapshot(&fixture);
                (
                    case,
                    fixture,
                    prepared,
                    geometry,
                    appearance,
                    before,
                    draft,
                    window_id,
                    seed + 1,
                )
            })
            .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    let completed = Arc::new(AtomicBool::new(false));
    let result = completed.clone();
    Application::new().run(move |app| {
        gpui_text_input::ensure_text_input_bindings(app);
        let (control, _) = native::open(app, "acquired-startup-disposal-control");
        app.spawn(async move |cx| {
            for (
                case,
                fixture,
                prepared,
                geometry,
                appearance,
                before,
                draft,
                window_id,
                session_seed,
            ) in fixtures
            {
                let (mut shell, composer, gate) = cx
                    .update(|app| {
                        let owner = GpuiAppearanceWindowSet::new(
                            appearance,
                            NonZeroUsize::new(4).unwrap(),
                            app,
                        );
                        let shell = GpuiMainWindowShellHost::new(app, owner)
                            .with_prepared_placement(geometry)
                            .construct_hidden(prepared)
                            .unwrap_or_else(|_| panic!("acquired native mount"));
                        let composer = shell
                            .window()
                            .read_with(app, |root, app| {
                                root.controller()
                                    .unwrap()
                                    .composer_mount()
                                    .unwrap()
                                    .read(app)
                                    .contribution()
                                    .unwrap()
                            })
                            .unwrap();
                        let gate = (case == 3)
                            .then(|| composer.read(app).test_block_next_selected_dispatch());
                        (shell, composer, gate)
                    })
                    .unwrap();
                cx.update(|app| {
                    shell.gate_startup_interaction(app).unwrap();
                    shell.enroll_startup_disposal(app).unwrap();
                })
                .unwrap();
                shell = cx
                    .update(|app| shell.close_before_publication(app))
                    .unwrap()
                    .err()
                    .expect("enrolled acquired shell refuses early retirement custody");
                let (mut shell, raw) = flight::flight(shell, Case::Ready, cx).await;
                if let Some(gate) = gate.as_ref() {
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while !gate.is_blocked() {
                        assert!(Instant::now() < deadline, "initial dispatch gate timeout");
                        shell
                            .window()
                            .update(cx, |_, window, _| window.refresh())
                            .unwrap();
                        native::pump(cx).await;
                    }
                    assert!(
                        composer
                            .read_with(cx, |composer, _| composer.test_has_active_flight())
                            .unwrap()
                    );
                } else {
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while !cx.update(|app| shell.ready_to_publish(app)).unwrap() {
                        assert!(
                            Instant::now() < deadline,
                            "acquired first-presentable timeout"
                        );
                        shell
                            .window()
                            .update(cx, |_, window, _| window.refresh())
                            .unwrap();
                        native::pump(cx).await;
                    }
                }
                if case != 0 {
                    shell.seal_startup_publication().unwrap();
                }
                if case == 2 {
                    cx.update(|app| shell.publish(app)).unwrap().unwrap();
                    assert!(native::visible(raw));
                    shell = cx
                        .update(|app| shell.release_published_handle(app))
                        .unwrap()
                        .err()
                        .expect("enrolled published acquired shell stays complete");
                } else {
                    assert!(!native::visible(raw));
                }
                let completion = Rc::new(RefCell::new(None));
                let retained = completion.clone();
                let process = fixture.process.clone();
                cx.update(|app| {
                    shell.start_startup_disposal(
                        move |outcome, _| {
                            assert!(retained.borrow().is_none());
                            assert!(
                                !native::alive(raw),
                                "retirement custody follows native destruction"
                            );
                            assert_eq!(
                                process.main_window_occupancy(),
                                1,
                                "worker retirement retains reservation"
                            );
                            *retained.borrow_mut() = Some(outcome);
                        },
                        app,
                    )
                })
                .unwrap()
                .unwrap_or_else(|failure| panic!("{}", failure.error));
                if let Some(gate) = gate {
                    native::pump(cx).await;
                    assert!(completion.borrow().is_none());
                    assert!(
                        native::alive(raw),
                        "pending editor dispatch must retain its exact native window"
                    );
                    assert_eq!(fixture.process.main_window_occupancy(), 1);
                    assert!(
                        !composer
                            .read_with(cx, |composer, _| composer.test_widget_released())
                            .unwrap()
                    );
                    gate.release();
                }
                let deadline = Instant::now() + Duration::from_secs(15);
                while completion.borrow().is_none() {
                    assert!(
                        Instant::now() < deadline,
                        "acquired startup disposal timeout"
                    );
                    native::pump(cx).await;
                }
                let retirement = match completion.borrow_mut().take().unwrap() {
                    Completion::Ready { retirement } => retirement,
                    Completion::Rejected(failure) => panic!("{}", failure.error),
                };
                assert!(!native::alive(raw));
                assert!(
                    composer
                        .read_with(cx, |composer, _| composer.test_widget_released())
                        .unwrap()
                );
                cx.background_executor().spawn(async move {
                    assert_eq!(
                        snapshot(&fixture),
                        before,
                        "native disposal makes no durable edit"
                    );
                    match retirement {
                        MainWindowStartupRetirement::AcquiredUnpublished(unpublished)
                            if case == 0 =>
                        {
                            assert_eq!(unpublished.window_id(), window_id);
                            let MainWindowShellAbandonmentPreparationOutcome::ExactAcquired {
                                abandonment,
                            } = unpublished
                                .prepare_abandonment(&fixture.service, CommandCancellation::new())
                            else {
                                panic!(
                                    "unsealed acquired member retains exact abandonment authority"
                                )
                            };
                            assert!(matches!(
                                abandonment.abandon(&fixture.service, CommandCancellation::new()),
                                MainWindowShellAbandonmentOutcome::Committed { .. }
                            ));
                            assert!(snapshot(&fixture).windows().is_empty());
                        }
                        MainWindowStartupRetirement::AcquiredPreserved(custody) if case != 0 => {
                            assert_eq!(custody.window_id(), window_id);
                            assert!(matches!(
                                custody.retire(CommandCancellation::new()),
                                MainWindowShellRecordPreservingRetirementOutcome::Retired
                            ));
                            assert_eq!(
                                snapshot(&fixture),
                                before,
                                "seal preserves records even for an unshown member"
                            );
                            assert!(matches!(
                                fixture.session(draft, session_seed),
                                DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
                            ));
                        }
                        _ => {
                            panic!("startup retirement kind must reflect the set preservation seal")
                        }
                    }
                    assert_eq!(fixture.process.main_window_occupancy(), 0);
                }).await;
            }
            result.store(true, Ordering::SeqCst);
            control
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        })
        .detach();
    });
    assert!(completed.load(Ordering::SeqCst));
}

#[test]
fn native_restored_startup_disposal_transfers_original_retirement_after_destruction() {
    let (fixture, attempt, service, prepared, geometry, appearance, before, draft, selection) =
        home_support::worker(|| {
            let PreparedFixture {
                prepared,
                attempt,
                _service,
                fixture,
                appearance: _,
                seals,
                snapshot,
                draft,
            } = prepared_fixture(220);
            let appearance = AppearanceCoordinator::new(
                AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
                flight::system_font_appearance(&fixture.state),
            )
            .current();
            let selection = prepared.selection_identity();
            let shell = RestoredWindowShellPrepared::prepare(
                prepared,
                &attempt,
                &fixture.process,
                Box::new(config),
                seals,
                submission(),
                appearance.clone(),
            )
            .unwrap_or_else(|failure| panic!("{}", failure.error));
            let geometry =
                prepare_windows_window_placement(shell.window_id(), shell.placement().clone())
                    .unwrap();
            (
                fixture, attempt, _service, shell, geometry, appearance, snapshot, draft, selection,
            )
        })
        .join()
        .unwrap();
    let completed = Arc::new(AtomicBool::new(false));
    let result = completed.clone();
    Application::new().run(move |app| {
        gpui_text_input::ensure_text_input_bindings(app);
        let (control, _) = native::open(app, "restored-startup-disposal-control");
        app.spawn(async move |cx| {
            let mut shell = cx
                .update(|app| {
                    let owner = GpuiAppearanceWindowSet::new(
                        appearance,
                        NonZeroUsize::new(4).unwrap(),
                        app,
                    );
                    GpuiMainWindowShellHost::new(app, owner)
                        .with_prepared_placement(geometry)
                        .construct_restored_hidden(prepared)
                        .unwrap_or_else(|_| panic!("restored native mount"))
                })
                .unwrap();
            cx.update(|app| {
                shell.gate_startup_interaction(app).unwrap();
                shell.enroll_startup_disposal(app).unwrap();
            })
            .unwrap();
            shell = cx
                .update(|app| shell.close_restored_before_publication(app))
                .unwrap()
                .err()
                .expect("enrolled restoration retains original editor custody");
            let (mut shell, raw) = flight::flight(shell, Case::Ready, cx).await;
            let deadline = Instant::now() + Duration::from_secs(10);
            while !cx.update(|app| shell.ready_to_publish(app)).unwrap() {
                assert!(
                    Instant::now() < deadline,
                    "restored first-presentable timeout"
                );
                shell
                    .window()
                    .update(cx, |_, window, _| window.refresh())
                    .unwrap();
                native::pump(cx).await;
            }
            shell.seal_startup_publication().unwrap();
            cx.update(|app| shell.publish(app)).unwrap().unwrap();
            assert!(native::visible(raw));
            let completion = Rc::new(RefCell::new(None));
            let retained = completion.clone();
            let process = fixture.process.clone();
            cx.update(|app| {
                shell.start_startup_disposal(
                    move |outcome, _| {
                        assert!(retained.borrow().is_none());
                        assert!(!native::alive(raw));
                        assert_eq!(process.main_window_occupancy(), 1);
                        *retained.borrow_mut() = Some(outcome);
                    },
                    app,
                )
            })
            .unwrap()
            .unwrap_or_else(|failure| panic!("{}", failure.error));
            let deadline = Instant::now() + Duration::from_secs(15);
            while completion.borrow().is_none() {
                assert!(Instant::now() < deadline, "restored disposal timeout");
                native::pump(cx).await;
            }
            let unpublished = match completion.borrow_mut().take().unwrap() {
                Completion::Ready {
                    retirement: MainWindowStartupRetirement::Restored(unpublished),
                } => unpublished,
                Completion::Rejected(failure) => panic!("{}", failure.error),
                _ => panic!("restoration preserves its original retirement kind"),
            };
            let (fixture, before) = cx
                .background_executor()
                .spawn(async move {
                    assert_eq!(snapshot(&fixture), before);
                    (fixture, before)
                })
                .await;
            let cancellation = CommandCancellation::new();
            let retirement = cx
                .background_executor()
                .spawn(async move { unpublished.retire(cancellation) })
                .await;
            assert!(matches!(retirement, RestoredWindowShellRetirement::Retired));
            cx.background_executor()
                .spawn(async move {
                    assert_eq!(fixture.process.main_window_occupancy(), 0);
                    assert_eq!(snapshot(&fixture), before);
                    let current = fixture
                        .storage
                        .current_draft(
                            &fixture.store,
                            selection.claim().thread_id(),
                            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
                        )
                        .unwrap()
                        .unwrap();
                    assert_eq!(current.draft().id(), draft);
                    assert_eq!(
                        current.draft().piece_root().summary().logical_utf8_bytes(),
                        SAVED_TEXT.len() as u64
                    );
                    drop((attempt, service));
                })
                .await;
            result.store(true, Ordering::SeqCst);
            control
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        })
        .detach();
    });
    assert!(completed.load(Ordering::SeqCst));
}
