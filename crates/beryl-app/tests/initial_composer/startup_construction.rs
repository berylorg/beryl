use super::*;
use gpui::{Application, AsyncApp};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows::{Win32::UI::WindowsAndMessaging::FindWindowW, core::PCWSTR};

#[path = "../support/shell_desktop_flight.rs"]
mod flight;
use MainWindowStartupConstructionFault as Fault;
use MainWindowStartupDisposalCompletion as Completion;
use MainWindowStartupShellHostFailure as Failure;
use MainWindowStartupShellPrepared as Prepared;
use flight::native;

#[derive(Clone, Copy, Debug)]
enum Case {
    MissingPlacement,
    MissingReceipt,
    NoEditor,
    PartialEditor,
    Appearance,
    Ready,
}

impl Case {
    fn fault(self) -> Option<Fault> {
        match self {
            Self::MissingReceipt => Some(Fault::ReceiptRegistration),
            Self::NoEditor => Some(Fault::ComposerMount),
            Self::PartialEditor => Some(Fault::ComposerSetup),
            Self::Appearance => Some(Fault::AppearanceRegistration),
            Self::MissingPlacement | Self::Ready => None,
        }
    }
}

#[test]
fn native_restored_startup_construction_retains_each_failure_until_proven_cleanup() {
    let fixtures = home_support::worker(|| {
        [
            Case::MissingPlacement,
            Case::MissingReceipt,
            Case::NoEditor,
            Case::PartialEditor,
            Case::Appearance,
            Case::Ready,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, case)| {
            let PreparedFixture {
                prepared,
                attempt,
                _service,
                fixture,
                appearance: _,
                seals,
                snapshot,
                draft: _,
            } = prepared_fixture(110 + index as u8);
            let appearance = AppearanceCoordinator::new(
                AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
                flight::system_font_appearance(&fixture.state),
            )
            .current();
            let prepared = RestoredWindowShellPrepared::prepare(
                prepared,
                &attempt,
                &fixture.process,
                Box::new(config),
                seals,
                submission(),
                appearance.clone(),
            )
            .unwrap_or_else(|failure| panic!("{}", failure.error));
            let geometry = prepare_windows_window_placement(
                prepared.window_id(),
                prepared.placement().clone(),
            )
            .unwrap();
            (
                case, fixture, attempt, _service, prepared, geometry, appearance, snapshot,
            )
        })
        .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    let completed = Arc::new(AtomicBool::new(false));
    let result = completed.clone();
    let watched = Rc::new(RefCell::new(
        None::<(usize, RuntimeBackedWindowProcessRegistry)>,
    ));
    let observed = watched.clone();
    gpui::with_windows_window_destruction_observer_for_test(
        move |raw| {
            if let Some((expected, process)) = observed.borrow().as_ref() {
                if raw == *expected {
                    assert_eq!(process.main_window_occupancy(), 1);
                }
            }
        },
        || {
            Application::new().run(move |app| {
                gpui_text_input::ensure_text_input_bindings(app);
                let (control, _) = native::open(app, "startup-construction-control");
                app.spawn(async move |cx| {
                    for (case, fixture, attempt, service, prepared, geometry, appearance, before) in
                        fixtures
                    {
                        let construction = cx
                            .update(|app| {
                                let owner = GpuiAppearanceWindowSet::new(
                                    appearance,
                                    NonZeroUsize::new(4).unwrap(),
                                    app,
                                );
                                let mut host = GpuiMainWindowShellHost::new(app, owner);
                                if !matches!(case, Case::MissingPlacement) {
                                    host = host.with_prepared_placement(geometry);
                                }
                                if let Some(fault) = case.fault() {
                                    host.test_fail_startup_construction_at(fault);
                                }
                                host.construct_startup_hidden(Prepared::Restored(prepared))
                            })
                            .unwrap();
                        let unpublished = if matches!(case, Case::MissingPlacement) {
                            match construction {
                                Err(Failure::BeforeConstruction {
                                    prepared: Prepared::Restored(prepared),
                                    error,
                                }) => {
                                    assert!(!error.is_empty());
                                    assert_eq!(fixture.process.main_window_occupancy(), 1);
                                    Some(prepared.into_unpublished())
                                }
                                _ => panic!(
                                    "missing placement must return original prepared restoration"
                                ),
                            }
                        } else {
                            let mut shell = match (case, construction) {
                                (Case::Ready, Ok(shell)) => shell,
                                (_, Err(Failure::Native { shell, error })) => {
                                    assert!(!error.is_empty());
                                    shell
                                }
                                _ => panic!("wrong startup construction outcome for {case:?}"),
                            };
                            let raw = hidden_handle(&shell, cx);
                            assert_eq!(fixture.process.main_window_occupancy(), 1);
                            let composer = shell
                                .window()
                                .read_with(cx, |root, app| {
                                    assert!(root.startup_interaction_gated());
                                    root.controller()
                                        .unwrap()
                                        .composer_mount()
                                        .and_then(|mount| mount.read(app).contribution())
                                })
                                .unwrap();
                            assert_eq!(
                                composer.is_some(),
                                matches!(
                                    case,
                                    Case::PartialEditor | Case::Appearance | Case::Ready
                                )
                            );
                            assert!(
                                cx.update(|app| shell.enroll_startup_disposal(app))
                                    .unwrap()
                                    .is_err()
                            );
                            assert!(cx.update(|app| shell.publish(app)).unwrap().is_err());
                            shell = cx
                                .update(|app| shell.close_restored_before_publication(app))
                                .unwrap()
                                .err()
                                .expect("construction retains native cleanup ownership");
                            *watched.borrow_mut() = Some((raw, fixture.process.clone()));
                            let completion = Rc::new(RefCell::new(None));
                            let retained = completion.clone();
                            let process = fixture.process.clone();
                            let admitted = cx
                                .update(|app| {
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
                                .unwrap();
                            if matches!(case, Case::MissingReceipt) {
                                let failure = admitted
                                    .err()
                                    .expect("missing receipt cannot authorize cleanup");
                                assert!(completion.borrow().is_none());
                                assert_eq!(fixture.process.main_window_occupancy(), 1);
                                failure
                                    .shell
                                    .window()
                                    .update(cx, |_, window, _| window.remove_window())
                                    .unwrap();
                                flight::wait_destroyed(raw, cx).await;
                                assert_eq!(fixture.process.main_window_occupancy(), 1);
                                *watched.borrow_mut() = None;
                                drop(failure);
                                None
                            } else {
                                admitted.unwrap_or_else(|failure| panic!("{}", failure.error));
                                let deadline = Instant::now() + Duration::from_secs(15);
                                while completion.borrow().is_none() {
                                    assert!(
                                        Instant::now() < deadline,
                                        "construction cleanup timeout: {case:?}"
                                    );
                                    native::pump(cx).await;
                                }
                                if let Some(composer) = composer {
                                    assert!(
                                        composer
                                            .read_with(cx, |composer, _| composer
                                                .test_widget_released())
                                            .unwrap()
                                    );
                                }
                                *watched.borrow_mut() = None;
                                match completion.borrow_mut().take().unwrap() {
                                    Completion::Ready {
                                        retirement:
                                            MainWindowStartupRetirement::Restored(unpublished),
                                    } => Some(unpublished),
                                    Completion::Rejected(failure) => panic!("{}", failure.error),
                                    _ => panic!(
                                        "restored construction keeps restored retirement custody"
                                    ),
                                }
                            }
                        };
                        if let Some(unpublished) = unpublished {
                            let retirement = cx
                                .background_executor()
                                .spawn(
                                    async move { unpublished.retire(CommandCancellation::new()) },
                                )
                                .await;
                            assert!(matches!(retirement, RestoredWindowShellRetirement::Retired));
                            assert_eq!(fixture.process.main_window_occupancy(), 0);
                        }
                        cx.background_executor()
                            .spawn(async move {
                                assert_eq!(snapshot(&fixture), before);
                                drop((attempt, service));
                            })
                            .await;
                    }
                    result.store(true, Ordering::SeqCst);
                    control
                        .update(cx, |_, window, _| window.remove_window())
                        .unwrap();
                })
                .detach();
            })
        },
    );
    assert!(completed.load(Ordering::SeqCst));
}

#[test]
fn native_acquired_startup_construction_failure_keeps_original_abandonment_authority() {
    let (fixture, prepared, geometry, appearance, before, window_id) = home_support::worker(|| {
        let fixture = Fixture::new(130);
        let mut initial = fixture.begin(131);
        initial.advance(&CommandCancellation::new()).unwrap();
        let placement = initial.acquisition().placement().clone();
        let appearance = AppearanceCoordinator::new(
            AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
            flight::system_font_appearance(&fixture.state),
        )
        .current();
        let prepared = initial
            .prepare(&mut config)
            .unwrap_or_else(|failure| panic!("{}", failure.error))
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
        (fixture, prepared, geometry, appearance, before, window_id)
    })
    .join()
    .unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let result = finished.clone();
    Application::new().run(move |app| {
        gpui_text_input::ensure_text_input_bindings(app);
        let (control, _) = native::open(app, "acquired-construction-control");
        app.spawn(async move |cx| {
            let shell = cx
                .update(|app| {
                    let owner = GpuiAppearanceWindowSet::new(
                        appearance,
                        NonZeroUsize::new(4).unwrap(),
                        app,
                    );
                    let mut host =
                        GpuiMainWindowShellHost::new(app, owner).with_prepared_placement(geometry);
                    host.test_fail_startup_construction_at(Fault::ComposerSetup);
                    match host.construct_startup_hidden(Prepared::Acquired(prepared)) {
                        Err(Failure::Native { shell, error }) => {
                            assert!(!error.is_empty());
                            shell
                        }
                        _ => panic!("partial acquired editor setup retains native owner"),
                    }
                })
                .unwrap();
            let raw = hidden_handle(&shell, cx);
            let completion = Rc::new(RefCell::new(None));
            let retained = completion.clone();
            let process = fixture.process.clone();
            cx.update(|app| {
                shell.start_startup_disposal(
                    move |outcome, _| {
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
                assert!(
                    Instant::now() < deadline,
                    "acquired construction cleanup timeout"
                );
                native::pump(cx).await;
            }
            let unpublished = match completion.borrow_mut().take().unwrap() {
                Completion::Ready {
                    retirement: MainWindowStartupRetirement::AcquiredUnpublished(unpublished),
                } => unpublished,
                Completion::Rejected(failure) => panic!("{}", failure.error),
                _ => panic!("unexposed acquisition retains original abandonment kind"),
            };
            assert_eq!(unpublished.window_id(), window_id);
            cx.background_executor().spawn(async move {
                assert_eq!(snapshot(&fixture), before);
                let MainWindowShellAbandonmentPreparationOutcome::ExactAcquired { abandonment } =
                    unpublished.prepare_abandonment(&fixture.service, CommandCancellation::new())
                else { panic!("original acquisition authorizes exact abandonment") };
                assert!(matches!(abandonment.abandon(&fixture.service, CommandCancellation::new()),
                    MainWindowShellAbandonmentOutcome::Committed { .. }));
                assert_eq!(fixture.process.main_window_occupancy(), 0);
                assert!(snapshot(&fixture).windows().is_empty());
            }).await;
            result.store(true, Ordering::SeqCst);
            control
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        })
        .detach();
    });
    assert!(finished.load(Ordering::SeqCst));
}

fn hidden_handle(shell: &MainWindowShell, cx: &mut AsyncApp) -> usize {
    let title = format!("Beryl startup construction {}", std::process::id());
    shell
        .window()
        .update(cx, |_, window, _| window.set_window_title(&title))
        .unwrap();
    let wide = title.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let raw = unsafe { FindWindowW(None, PCWSTR(wide.as_ptr())) }
        .unwrap()
        .0 as usize;
    assert!(native::alive(raw));
    assert!(!native::visible(raw));
    raw
}
