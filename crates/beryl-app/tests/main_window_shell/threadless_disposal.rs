use super::*;
use beryl_app::main_window::{
    MainWindowStartupDisposalCompletion as Completion, MainWindowStartupRetirement,
};
use gpui::{AppContext, Application};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[path = "../support/shell_desktop_flight.rs"]
mod flight;
use flight::{Case, native};

#[test]
fn native_threadless_startup_disposal_joins_destruction_before_reservation_release() {
    let fixtures = support::worker(|| {
        (0..3)
            .map(|case| {
                let mut fixture = Fixture::with_placement(210 + case, placement());
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
    let completed = Arc::new(AtomicBool::new(false));
    let result = completed.clone();
    let watched = Rc::new(RefCell::new(
        None::<(usize, RuntimeBackedWindowProcessRegistry)>,
    ));
    let observed = watched.clone();
    gpui::with_windows_window_destruction_observer_for_test(
        move |raw| {
            if let Some((expected, process)) = observed.borrow().as_ref() {
                if *expected == raw {
                    assert!(native::alive(raw));
                    assert_eq!(
                        process.main_window_occupancy(),
                        1,
                        "reservation remains held before DestroyWindow"
                    );
                }
            }
        },
        || {
            Application::new().run(move |app| {
                let (control, _) = native::open(app, "threadless-disposal-control");
                app.spawn(async move |cx| {
                    for (case, mut fixture, prepared, geometry, before) in fixtures {
                        let mut shell = cx
                            .update(|app| {
                                let owner = GpuiAppearanceWindowSet::new(
                                    fixture.coordinator.as_ref().unwrap().current(),
                                    NonZeroUsize::new(4).unwrap(),
                                    app,
                                );
                                fixture
                                    .coordinator
                                    .as_mut()
                                    .unwrap()
                                    .attach_publication_target(owner.read(app).target())
                                    .unwrap();
                                GpuiMainWindowShellHost::new(app, owner)
                                    .with_prepared_placement(geometry)
                                    .construct_threadless_hidden(prepared)
                                    .unwrap()
                            })
                            .unwrap();
                        assert!(
                            cx.update(|app| shell.enroll_startup_disposal(app))
                                .unwrap()
                                .is_err(),
                            "ungated enrollment is refused"
                        );
                        cx.update(|app| shell.gate_startup_interaction(app))
                            .unwrap()
                            .unwrap();
                        cx.update(|app| shell.enroll_startup_disposal(app))
                            .unwrap()
                            .unwrap();
                        assert!(
                            cx.update(|app| shell.enroll_startup_disposal(app))
                                .unwrap()
                                .is_err()
                        );
                        shell = cx
                            .update(|app| shell.close_threadless_before_publication(app))
                            .unwrap()
                            .err()
                            .expect("enrollment retains complete threadless custody");
                        let (mut shell, raw) = flight::flight(shell, Case::Ready, cx).await;
                        assert!(
                            cx.update(|app| shell.publish(app)).unwrap().is_err(),
                            "enrolled publication needs the set preservation seal"
                        );
                        if case == 1 {
                            shell.seal_startup_publication().unwrap();
                            shell.seal_startup_publication().unwrap();
                            cx.update(|app| shell.publish(app)).unwrap().unwrap();
                            assert!(native::visible(raw));
                            shell = cx
                                .update(|app| shell.release_published_handle(app))
                                .unwrap()
                                .err()
                                .expect("enrolled shell cannot transfer a handle alone");
                        }
                        *watched.borrow_mut() = Some((raw, fixture.process.clone()));
                        if case == 2 {
                            shell
                                .window()
                                .update(cx, |_, window, _| window.remove_window())
                                .unwrap();
                            flight::wait_destroyed(raw, cx).await;
                        }
                        let completion = Rc::new(RefCell::new(None));
                        let retained = completion.clone();
                        let process = fixture.process.clone();
                        let admitted = cx
                            .update(|app| {
                                shell.start_startup_disposal(
                                    move |completion, _| {
                                        assert!(retained.borrow().is_none());
                                        assert!(
                                            !native::alive(raw),
                                            "callback follows terminal native destruction"
                                        );
                                        if matches!(&completion, Completion::Ready { .. }) {
                                            assert_eq!(process.main_window_occupancy(), 0);
                                        } else {
                                            assert_eq!(process.main_window_occupancy(), 1);
                                        }
                                        *retained.borrow_mut() = Some(completion);
                                    },
                                    app,
                                )
                            })
                            .unwrap();
                        if case == 2 {
                            let failure = match admitted {
                                Err(failure) => {
                                    assert!(completion.borrow().is_none());
                                    failure
                                }
                                Ok(()) => {
                                    let deadline = Instant::now() + Duration::from_secs(10);
                                    while completion.borrow().is_none() {
                                        assert!(
                                            Instant::now() < deadline,
                                            "native-loss rejection timeout"
                                        );
                                        native::pump(cx).await;
                                    }
                                    match completion.borrow_mut().take().unwrap() {
                                        Completion::Rejected(failure) => failure,
                                        _ => panic!(
                                            "unexpected native loss cannot certify startup disposal"
                                        ),
                                    }
                                }
                            };
                            assert!(!failure.error.is_empty());
                            assert_eq!(fixture.process.main_window_occupancy(), 1);
                            let failure_shell = cx
                                .update(|app| {
                                    failure.shell.close_threadless_before_publication(app)
                                })
                                .unwrap()
                                .err()
                                .expect("failed disposal keeps extraction closed");
                            *watched.borrow_mut() = None;
                            drop(failure_shell);
                        } else {
                            admitted.unwrap_or_else(|failure| panic!("{}", failure.error));
                            let deadline = Instant::now() + Duration::from_secs(10);
                            while completion.borrow().is_none() {
                                assert!(Instant::now() < deadline, "threadless disposal timeout");
                                native::pump(cx).await;
                            }
                            assert!(matches!(
                                completion.borrow_mut().take().unwrap(),
                                Completion::Ready {
                                    retirement: MainWindowStartupRetirement::Threadless
                                }
                            ));
                            assert!(!native::alive(raw));
                            assert_eq!(fixture.process.main_window_occupancy(), 0);
                            *watched.borrow_mut() = None;
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
    assert!(completed.load(Ordering::SeqCst));
}
