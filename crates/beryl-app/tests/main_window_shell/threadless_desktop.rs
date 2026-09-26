use super::*;
use beryl_app::main_window::MainWindowShell;
use gpui::{AppContext, Application};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

#[path = "../support/shell_desktop_flight.rs"]
mod flight;
use flight::{Case, native};

#[test]
fn native_startup_shell_ignores_close_until_interaction_release() {
    let (mut fixture, prepared, geometry, before) = support::worker(|| {
        let mut fixture = Fixture::with_placement(190, placement());
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
        (fixture, prepared, geometry, before)
    })
    .join()
    .unwrap();
    let completed = Arc::new(AtomicBool::new(false));
    let result = completed.clone();
    Application::new().run(move |app| {
        let (control, _) = native::open(app, "startup-interaction-control");
        app.spawn(async move |cx| {
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
            cx.update(|app| shell.gate_startup_interaction(app))
                .unwrap()
                .unwrap();
            let (mut shell, raw) = flight::flight(shell, Case::Ready, cx).await;
            cx.update(|app| shell.publish(app)).unwrap().unwrap();
            assert!(native::visible(raw));
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    native::hwnd(raw),
                    windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                    Some(windows::Win32::Foundation::WPARAM(0)),
                    Some(windows::Win32::Foundation::LPARAM(0)),
                );
            }
            native::pump(cx).await;
            assert!(native::alive(raw));
            assert_eq!(fixture.process.main_window_occupancy(), 1);
            cx.update(|app| {
                MainWindowShell::release_startup_interaction(std::slice::from_ref(&shell), app)
            })
            .unwrap()
            .unwrap();
            cx.update(|app| shell.release_published_handle(app))
                .unwrap()
                .unwrap_or_else(|_| panic!("ordinary shell handoff"));
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    native::hwnd(raw),
                    windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                    Some(windows::Win32::Foundation::WPARAM(0)),
                    Some(windows::Win32::Foundation::LPARAM(0)),
                );
            }
            flight::wait_destroyed(raw, cx).await;
            assert_eq!(fixture.process.main_window_occupancy(), 0);
            cx.background_executor()
                .spawn(async move {
                    assert_eq!(fixture.store.home_revision().unwrap(), before);
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

#[test]
fn native_threadless_desktop_flights_preserve_shell_and_saved_state() {
    let fixtures = support::worker(|| {
        [
            Case::Ready,
            Case::MissingDesktop,
            Case::CancelBefore,
            Case::LeaseRefused,
            Case::CancelDuring,
            Case::CancelAfter,
            Case::CloseDuring,
            Case::CloseAfter,
            Case::RemoveDuring,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, case)| {
            let saved = if matches!(case, Case::MissingDesktop) {
                let mut absent = [0; 16];
                getrandom::fill(&mut absent).unwrap();
                WindowPlacement::new(
                    WindowBounds::new(10, 20, 900, 700).unwrap(),
                    WindowDisplayState::Normal,
                    None,
                    Some(beryl_model::VirtualDesktopId::from_bytes(absent)),
                )
            } else {
                placement()
            };
            let mut fixture = Fixture::with_placement(150 + index as u8, saved);
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
    let destroyed = Rc::new(RefCell::new(Vec::new()));
    let observed = destroyed.clone();
    let expected = Rc::new(RefCell::new(Vec::new()));
    let expected_in_app = expected.clone();
    gpui::with_windows_window_destruction_observer_for_test(
        move |raw| observed.borrow_mut().push(raw),
        || {
            Application::new().run(move |app| {
            let (control, control_raw) = native::open(app, "threadless-flight-control");
            app.spawn(async move |cx| {
                for (case, mut fixture, prepared, geometry, before) in fixtures {
                    let (shell, owner) = cx.update(|app| {
                        let owner = GpuiAppearanceWindowSet::new(fixture.coordinator.as_ref().unwrap().current(), NonZeroUsize::new(4).unwrap(), app);
                        let target = owner.read(app).target();
                        fixture.coordinator.as_mut().unwrap().attach_publication_target(target).unwrap();
                        let shell = GpuiMainWindowShellHost::new(app, owner.clone()).with_prepared_placement(geometry).construct_threadless_hidden(prepared).unwrap();
                        (shell, owner)
                    }).unwrap();
                    let (mut shell, raw) = flight::flight(shell, case, cx).await;
                    expected_in_app.borrow_mut().push(raw);
                    if matches!(case, Case::Ready | Case::MissingDesktop) {
                        cx.update(|app| assert!(shell.ready_to_publish(app))).unwrap();
                        let failure = cx.update(|app| shell.start_desktop_placement(CommandCancellation::new(), |_, _| panic!("duplicate completion"), app)).unwrap().err().expect("single admission");
                        assert!(matches!(failure.reason, beryl_app::main_window::MainWindowDesktopPlacementRejection::AlreadyEnrolled));
                        shell = failure.shell;
                        let window = shell.window();
                        cx.update(|app| shell.publish(app)).unwrap().unwrap();
                        assert!(native::visible(raw));
                        let failure = cx.update(|app| shell.start_desktop_placement(CommandCancellation::new(), |_, _| panic!("published completion"), app)).unwrap().err().expect("published admission refused");
                        assert!(matches!(failure.reason, beryl_app::main_window::MainWindowDesktopPlacementRejection::AlreadyPublished));
                        cx.update(|app| failure.shell.release_published_handle(app)).unwrap().unwrap_or_else(|_| panic!("published handle handoff"));
                        window.update(cx, |_, window, _| window.remove_window()).unwrap();
                    } else {
                        cx.update(|app| shell.close_threadless_before_publication(app)).unwrap().unwrap_or_else(|_| panic!("original threadless disposal"));
                    }
                    flight::wait_destroyed(raw, cx).await;
                    assert_eq!(fixture.process.main_window_occupancy(), 0);
                    cx.update(|app| {
                        use beryl_app::theme_runtime::AppearancePublicationTarget;
                        assert_eq!(owner.read(app).target().snapshot().count, 0);
                    }).unwrap();
                    cx.background_executor().spawn(async move {
                        assert_eq!(fixture.store.home_revision().unwrap(), before);
                        let saved = fixture.state.session().minimal_bootstrap(&fixture.store).unwrap().unwrap();
                        assert_eq!(saved.windows().len(), 1);
                        assert_eq!(saved.windows()[0].window_id(), fixture.window);
                        assert!(saved.windows()[0].selected_thread().is_none());
                    }).await;
                }
                expected_in_app.borrow_mut().push(control_raw);
                result.store(true, Ordering::SeqCst);
                control.update(cx, |_, window, _| window.remove_window()).unwrap();
            }).detach();
        });
        },
    );
    assert!(completed.load(Ordering::SeqCst));
    assert_eq!(*destroyed.borrow(), *expected.borrow());
    assert!(expected.borrow().iter().all(|raw| !native::alive(*raw)));
}
