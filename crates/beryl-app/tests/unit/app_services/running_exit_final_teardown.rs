use crate::running_owner::RunningExitCompletion;
use gpui::{AppContext, EntityInputHandler};
use windows::{
    Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::{
            Controls::TDM_CLICK_BUTTON,
            WindowsAndMessaging::{FindWindowW, IDCANCEL, IDOK, PostMessageW},
        },
    },
    core::PCWSTR,
};

#[derive(Clone, Copy, PartialEq)]
enum Failure {
    None,
    Service,
    Native,
}

#[test]
fn native_running_exit_completes_threadless_teardown_before_quit() {
    run(0, Failure::None);
}

#[test]
fn native_running_exit_completes_selected_multi_window_teardown_before_quit() {
    run(2, Failure::None);
}

#[test]
fn native_running_exit_service_failure_preserves_readable_residents_and_exact_quit() {
    run(2, Failure::Service);
}

#[test]
fn native_running_exit_native_failure_preserves_blocked_custody_and_exact_quit() {
    run(1, Failure::Native);
}

fn run(selected_count: u8, failure: Failure) {
    let directory = if selected_count == 0 {
        support::native_home()
    } else {
        resident_recovery::resident_fixture::selected_home_with_draft(
            selected_count,
            if failure == Failure::None { 0 } else { 4 },
        )
    };
    eprintln!(
        "native final teardown fixture: {}",
        directory.path().display()
    );
    let mut input = input(directory.path(), |path, _| support::open(path));
    if selected_count != 0 {
        input.windows = resident_recovery::resident_fixture::selected_inputs_with_extent(
            if failure == Failure::None { 0 } else { 768 * 4 },
        );
    }
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new().with_quit_on_last_window_close(false).run(move |app| {
        startup_owner::start(input, move |result, app| {
            eprintln!("native final teardown startup returned");
            let StartupCompletion::Running(running) = result else { panic!("startup failed") };
            let invoking = running.windows.window_ids()[0];
            let window = running.windows.shells()[0].window();
            let owner = RunningProcessOwner::start(running, app);
            if failure == Failure::Service {
                owner.borrow_mut().test_services_mut().test_fail_shutdown_completion();
            }
            let command = owner.borrow().window_exit_command(invoking, app).unwrap();
            if failure == Failure::None && selected_count == 0 {
                let completed = observed.clone();
                RunningProcessOwner::wait_for_exit_attempt(&owner, ProjectionCancellationToken::new(), app,
                    move |owner, result, app| {
                        assert!(matches!(result, RunningExitCompletion::Finished), "{result:?}");
                        assert!(app.windows().is_empty());
                        assert!(owner.borrow().test_process().windows.shells().is_empty());
                        assert!(owner.borrow().test_services().graph().is_none());
                        completed.set(true);
                    }).unwrap();
                command.request_exit();
                return;
            }
            app.spawn(async move |cx| {
                let copied = Rc::new(RefCell::new(Vec::<String>::new()));
                let mut chunk = vec![b'x'; 768];
                for offset in (63..768).step_by(64) { chunk[offset] = b'\n'; }
                let expected = String::from_utf8(chunk).unwrap().repeat(4);
                let resident = if failure != Failure::None {
                    Some(cx.update(|app| window.read(app).unwrap().controller().unwrap().composer_mount().unwrap()
                        .read(app).contribution().unwrap()).unwrap())
                } else { None };
                if let Some(composer) = &resident {
                    let deadline = Instant::now() + Duration::from_secs(10);
                    loop {
                        let ready = cx.update(|app| {
                            let input = composer.read(app).gpui_input();
                            input.read(app).surface().is_some() && input.read(app).is_quiescent()
                        }).unwrap();
                        if ready { break; }
                        assert!(Instant::now() < deadline, "initial fixture surface did not settle");
                        cx.background_executor().timer(Duration::from_millis(10)).await;
                    }
                    let writes = copied.clone();
                    eprintln!("native final teardown initial resident ready");
                    cx.update(|app| window.update(app, |_, window, app| {
                        composer.update(app, |composer, cx| {
                            composer.test_set_clipboard_writer(Box::new(move |text, _| { writes.borrow_mut().push(text.to_owned()); gpui_text_input::ClipboardWriteOutcome::Written }));
                        });
                    })).unwrap().unwrap();
                }
                command.request_exit();
                eprintln!("native final teardown Exit requested");
                let request = next_request(&owner, cx).await;
                let (request, refusal) = cx.update(|app| {
                    RunningProcessOwner::finish_ready_exit(&owner, request, app, |_, _, _| panic!("unready teardown"))
                }).unwrap().err().unwrap();
                assert!(!refusal.is_empty());
                let (sender, receiver) = futures_channel::oneshot::channel();
                assert!(cx.update(|app| RunningProcessOwner::run_exit_attempt(&owner, request,
                    ProjectionCancellationToken::new(), app, move |_, request, outcome, _| {
                        assert!(matches!(outcome.result, Ok(crate::running_owner::ExitAttemptCompletion::SessionReady)), "{outcome:?}");
                        assert!(!outcome.command_completed);
                        sender.send(request).ok().unwrap();
                    })).unwrap().is_ok());
                let request = receiver.await.unwrap();
                eprintln!("native final teardown session ready");
                let foreign = request.test_foreign();
                assert!(cx.update(|app| RunningProcessOwner::finish_ready_exit(&owner, foreign, app,
                    |_, _, _| panic!("foreign ready request admitted"))).unwrap().is_err());
                let identity = request.identity();
                let delivered = Rc::new(RefCell::new(None));
                let delivery = delivered.clone();
                let completed = observed.clone();
                let native_gate = cx.update(|app| {
                    assert!(RunningProcessOwner::finish_ready_exit(&owner, request, app, move |owner, result, app| {
                        assert!(!owner.borrow().test_services_on_worker());
                        if failure == Failure::None {
                            assert!(matches!(result, RunningExitCompletion::Finished), "{result:?}: {:?}", owner.borrow().test_final_teardown_detail());
                            assert!(app.windows().is_empty());
                            completed.set(true);
                        } else {
                            assert!(matches!(result, RunningExitCompletion::Blocked));
                            *delivery.borrow_mut() = Some(());
                        }
                    }).is_ok());
                    if failure == Failure::Native {
                        owner.borrow_mut().test_fail_final_native_cleanup();
                    }
                    if failure == Failure::None { Some(owner.borrow_mut().test_defer_final_native_receipt()) } else { None }
                }).unwrap();
                if let Some(native_gate) = native_gate {
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while cx.update(|app| app.windows().len()).unwrap() == usize::from(selected_count) {
                        assert!(Instant::now() < deadline, "native destruction never reached its receipt");
                        cx.background_executor().timer(Duration::from_millis(10)).await;
                    }
                    assert!(!observed.get());
                    assert!(owner.borrow().test_services().graph().is_none());
                    let weak = Rc::downgrade(&owner);
                    drop(owner);
                    assert!(weak.upgrade().is_some(), "caller abandonment lost retained final completion");
                    native_gate.send(()).unwrap();
                    return;
                }
                wait(&delivered, cx).await;
                eprintln!("native final teardown blocked read-only resident");
                let composer = resident.as_ref().unwrap();
                let binding = cx.update(|app| composer.read(app).selection_identity()).unwrap();
                let resident_bytes = cx.update(|app| composer.read(app).gpui_input().read(app).realization_diagnostics().current.resident_page_bytes).unwrap();
                assert!(resident_bytes < expected.len(), "fixture must require detached pages: {resident_bytes}");
                cx.update(|app| window.update(app, |_, window, app| {
                    composer.read(app).gpui_input().update(app, |input, cx| {
                        input.focus(window);
                        input.replace_text_in_range(None, "forbidden edit", window, cx);
                        assert!(matches!(input.begin_clipboard(gpui_text_input::ClipboardKind::Cut, cx), Err(gpui_text_input::RangeTextInputError::ReadOnly)));
                    });
                    window.dispatch_action(Box::new(gpui_text_input::SelectAll), app);
                })).unwrap().unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let ready = cx.update(|app| {
                        let input = composer.read(app).gpui_input();
                        input.read(app).surface().is_some_and(|surface| surface.selection().range().is_ok_and(|range| range.start().byte_offset.get() == 0 && range.end().byte_offset.get() == expected.len() as u64))
                    }).unwrap();
                    if ready { break; }
                    assert!(Instant::now() < deadline, "detached select all did not settle");
                    cx.background_executor().timer(Duration::from_millis(10)).await;
                }
                cx.update(|app| window.update(app, |_, window, app| window.dispatch_action(Box::new(gpui_text_input::Copy), app))).unwrap().unwrap();
                while copied.borrow().is_empty() {
                    assert!(Instant::now() < deadline, "detached copy did not settle: {:?}", cx.update(|app| composer.read(app).last_error().map(str::to_owned)).unwrap());
                    cx.background_executor().timer(Duration::from_millis(10)).await;
                }
                assert_eq!(copied.borrow().as_slice(), &[expected]);
                cx.update(|app| window.update(app, |_, window, app| window.dispatch_action(Box::new(gpui_text_input::MoveToEnd), app))).unwrap().unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let ready = cx.update(|app| {
                        let input = composer.read(app).gpui_input().read(app);
                        input.is_quiescent() && input.surface().is_some_and(|surface| surface.selection().range().is_ok_and(|range| range.start().byte_offset.get() == binding.binding().logical_extent().logical_utf8_bytes() && range.end().byte_offset.get() == range.start().byte_offset.get()))
                    }).unwrap();
                    if ready { break; }
                    assert!(Instant::now() < deadline, "detached end navigation did not settle");
                    cx.background_executor().timer(Duration::from_millis(10)).await;
                }
                assert_eq!(cx.update(|app| composer.read(app).selection_identity()).unwrap(), binding);
                assert!(owner.borrow().test_services().graph().is_none());
                assert!(owner.borrow().test_final_teardown_detail().is_some());
                assert!(owner.borrow().exit_requested());
                assert_eq!(owner.borrow().test_process().windows.shells().len(), usize::from(selected_count));
                cx.update(|app| {
                    let root = window.read(app).unwrap();
                    assert_eq!(root.test_exit_presentation().0, "Quit Anyway");
                    let notice = root.notice_projection().unwrap();
                    assert_eq!(notice.content.title().as_str(), "Beryl couldn't finish shutting down");
                    assert_eq!(notice.content.commands().count(), 0);
                    let mount = root.controller().unwrap().composer_mount().unwrap();
                    let composer = mount.read(app).contribution().unwrap();
                    assert!(composer.read(app).gpui_input().read(app).is_enabled());
                    assert!(Rc::ptr_eq(&identity, &owner.borrow().test_final_exit_identity().unwrap()));
                }).unwrap();
                let cancelled = ProjectionCancellationToken::new();
                cancelled.cancel();
                assert!(cx.update(|app| RunningProcessOwner::advance_shutdown(&owner, cancelled, app,
                    |_, _| panic!("irreversible reopening"))).unwrap().is_err());
                assert!(cx.update(|app| RunningProcessOwner::wait_for_exit_attempt(&owner,
                    ProjectionCancellationToken::new(), app, |_, _, _| panic!("duplicate teardown"))).unwrap().is_err());
                let terminations = Rc::new(Cell::new(0));
                let terminated = terminations.clone();
                owner.borrow_mut().test_blocked_termination(move || terminated.set(terminated.get() + 1));
                cx.update(|app| RunningProcessOwner::begin_blocked_quit(&owner, invoking, app)).unwrap().unwrap();
                let dialog_handle = dialog(cx).await;
                let reveal = cx.update(|app| RunningProcessOwner::begin_blocked_quit(&owner, invoking, app)).unwrap();
                if let Err(error) = reveal { assert_eq!(error, "native confirmation activation was refused"); }
                assert_eq!(dialog(cx).await, dialog_handle);
                choose(dialog_handle, IDCANCEL.0);
                wait_quit(&owner, cx).await;
                assert_eq!(terminations.get(), 0);
                assert!(owner.borrow().test_final_teardown_detail().is_some());
                cx.update(|app| window.update(app, |root, _, cx| root.request_blocked_quit(cx))).unwrap().unwrap().unwrap();
                choose(dialog(cx).await, IDOK.0);
                let deadline = Instant::now() + Duration::from_secs(5);
                while terminations.get() == 0 {
                    assert!(Instant::now() < deadline);
                    cx.background_executor().timer(Duration::from_millis(10)).await;
                }
                assert_eq!(terminations.get(), 1);
                assert!(cx.update(|app| RunningProcessOwner::begin_blocked_quit(&owner, invoking, app)).unwrap().is_err());
                assert!(owner.borrow().test_final_teardown_detail().is_some());
                observed.set(true);
                cx.update(|app| app.quit()).unwrap();
            }).detach();
        }, app);
        support::watchdog_with_timeout(app, Duration::from_secs(30));
    });
    assert!(finished.get());
    assert_reopens(&directory);
}

#[path = "running_detached_source_preparation.rs"]
mod source_preparation;

async fn dialog(cx: &AsyncApp) -> windows::Win32::Foundation::HWND {
    let title: Vec<u16> = "Quit Beryl anyway?".encode_utf16().chain(Some(0)).collect();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(dialog) = unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) } {
            return dialog;
        }
        assert!(
            Instant::now() < deadline,
            "blocked confirmation did not appear"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

fn choose(dialog: windows::Win32::Foundation::HWND, button: i32) {
    unsafe {
        PostMessageW(
            Some(dialog),
            TDM_CLICK_BUTTON.0 as u32,
            WPARAM(button as usize),
            LPARAM(0),
        )
        .unwrap();
    }
}

async fn wait_quit(owner: &Rc<RefCell<RunningProcessOwner>>, cx: &AsyncApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while owner.borrow().test_blocked_quit_pending() {
        assert!(
            Instant::now() < deadline,
            "blocked confirmation did not settle"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}
