use super::*;
use beryl_app::main_window::{
    MainWindowStartupConstructionFault as Fault, MainWindowStartupDisposalCompletion as Completion,
    MainWindowStartupRetirement, MainWindowStartupShellHostFailure as Failure,
    MainWindowStartupShellPrepared as Prepared,
};
use gpui::{AppContext, Application};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows::{Win32::UI::WindowsAndMessaging::FindWindowW, core::PCWSTR};

#[path = "../support/shell_desktop_flight.rs"]
mod flight;
use flight::native;

#[test]
fn native_threadless_startup_construction_retains_reservation_through_failure_and_destruction() {
    let fixtures = support::worker(|| {
        (0..4)
            .map(|case| {
                let mut fixture = Fixture::with_placement(150 + case, placement());
                fixture.coordinator = Some(AppearanceCoordinator::new(
                    AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
                    flight::system_font_appearance(&fixture.state),
                ));
                let prepared = fixture.prepare().unwrap();
                let geometry = beryl_app::main_window::prepare_windows_window_placement(
                    prepared.window_id(),
                    prepared.placement().clone(),
                )
                .unwrap();
                let before = fixture.store.home_revision().unwrap();
                (case, fixture, prepared, geometry, before)
            })
            .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let result = finished.clone();
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
                let (control, _) = native::open(app, "threadless-construction-control");
                app.spawn(async move |cx| {
                    for (case, fixture, prepared, geometry, before) in fixtures {
                        let construction = cx
                            .update(|app| {
                                let owner = GpuiAppearanceWindowSet::new(
                                    fixture.coordinator.as_ref().unwrap().current(),
                                    NonZeroUsize::new(4).unwrap(),
                                    app,
                                );
                                let mut host = GpuiMainWindowShellHost::new(app, owner);
                                if case != 0 {
                                    host = host.with_prepared_placement(geometry);
                                }
                                if case == 1 {
                                    host.test_fail_startup_construction_at(
                                        Fault::AppearanceRegistration,
                                    );
                                }
                                if case == 2 {
                                    host.test_fail_startup_construction_at(
                                        Fault::ReceiptRegistration,
                                    );
                                }
                                host.construct_startup_hidden(Prepared::Threadless(prepared))
                            })
                            .unwrap();
                        if case == 0 {
                            match construction {
                                Err(Failure::BeforeConstruction {
                                    prepared: Prepared::Threadless(prepared),
                                    error,
                                }) => {
                                    assert!(!error.is_empty());
                                    assert_eq!(prepared.window_id(), fixture.window);
                                    assert_eq!(fixture.process.main_window_occupancy(), 1);
                                    drop(prepared);
                                    assert_eq!(fixture.process.main_window_occupancy(), 0);
                                }
                                _ => panic!("preallocation failure retains threadless preparation"),
                            }
                        } else {
                            let mut shell = match (case, construction) {
                                (3, Ok(shell)) => shell,
                                (_, Err(Failure::Native { shell, error })) => {
                                    assert!(!error.is_empty());
                                    shell
                                }
                                _ => panic!("threadless construction outcome mismatch"),
                            };
                            let title =
                                format!("Beryl threadless construction {}", std::process::id());
                            shell
                                .window()
                                .update(cx, |root, window, _| {
                                    assert!(root.startup_interaction_gated());
                                    assert!(root.controller().unwrap().composer_mount().is_none());
                                    window.set_window_title(&title);
                                })
                                .unwrap();
                            let wide = title.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
                            let raw = unsafe { FindWindowW(None, PCWSTR(wide.as_ptr())) }
                                .unwrap()
                                .0 as usize;
                            assert!(native::alive(raw));
                            assert!(!native::visible(raw));
                            assert_eq!(fixture.process.main_window_occupancy(), 1);
                            assert!(
                                cx.update(|app| shell.enroll_startup_disposal(app))
                                    .unwrap()
                                    .is_err()
                            );
                            shell = cx
                                .update(|app| shell.close_threadless_before_publication(app))
                                .unwrap()
                                .err()
                                .expect("native failure retains the reservation");
                            *watched.borrow_mut() = Some((raw, fixture.process.clone()));
                            let completion = Arc::new(AtomicBool::new(false));
                            let delivered = completion.clone();
                            let observer = Rc::new(());
                            let retained_observer = observer.clone();
                            let process = fixture.process.clone();
                            let admitted = cx
                                .update(|app| {
                                    shell.start_startup_disposal(
                                        move |outcome, _| {
                                            assert_eq!(Rc::strong_count(&retained_observer), 1);
                                            assert!(!native::alive(raw));
                                            assert!(matches!(
                                                outcome,
                                                Completion::Ready {
                                                    retirement:
                                                        MainWindowStartupRetirement::Threadless
                                                }
                                            ));
                                            assert_eq!(process.main_window_occupancy(), 0);
                                            delivered.store(true, Ordering::SeqCst);
                                        },
                                        app,
                                    )
                                })
                                .unwrap();
                            drop(observer);
                            if case == 2 {
                                let failure = admitted
                                    .err()
                                    .expect("missing receipt remains a retained failure");
                                assert!(!completion.load(Ordering::SeqCst));
                                failure
                                    .shell
                                    .window()
                                    .update(cx, |_, window, _| window.remove_window())
                                    .unwrap();
                                flight::wait_destroyed(raw, cx).await;
                                assert_eq!(fixture.process.main_window_occupancy(), 1);
                                *watched.borrow_mut() = None;
                                drop(failure);
                            } else {
                                admitted.unwrap_or_else(|failure| panic!("{}", failure.error));
                                let deadline = Instant::now() + Duration::from_secs(10);
                                while !completion.load(Ordering::SeqCst) {
                                    assert!(
                                        Instant::now() < deadline,
                                        "threadless construction cleanup timeout"
                                    );
                                    native::pump(cx).await;
                                }
                                assert_eq!(fixture.process.main_window_occupancy(), 0);
                                *watched.borrow_mut() = None;
                            }
                        }
                        cx.background_executor()
                            .spawn(async move {
                                assert_eq!(fixture.store.home_revision().unwrap(), before);
                                let saved = fixture
                                    .state
                                    .session()
                                    .minimal_bootstrap(&fixture.store)
                                    .unwrap()
                                    .unwrap();
                                assert_eq!(saved.windows().len(), 1);
                                assert_eq!(saved.windows()[0].window_id(), fixture.window);
                                assert!(saved.windows()[0].selected_thread().is_none());
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
    assert!(finished.load(Ordering::SeqCst));
}
