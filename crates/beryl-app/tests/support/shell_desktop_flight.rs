use beryl_app::main_window::{
    MainWindowDesktopPlacementCompletion as Completion,
    MainWindowDesktopPlacementRejection as Rejection, MainWindowShell,
    WindowsDesktopPlacementOutcome,
};
use beryl_home_store::CommandCancellation;
use gpui::{AppContext, AsyncApp};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::{FindWindowW, SendMessageW, WM_CLOSE},
    },
    core::PCWSTR,
};

#[path = "desktop_placement_native.rs"]
pub mod native;

#[path = "native_shell_appearance.rs"]
mod appearance;
pub use appearance::system_font_appearance;

#[derive(Clone, Copy, Debug)]
pub enum Case {
    Ready,
    MissingDesktop,
    CancelBefore,
    LeaseRefused,
    CancelDuring,
    CancelAfter,
    CloseDuring,
    CloseAfter,
    RemoveDuring,
}

pub async fn flight(
    mut shell: MainWindowShell,
    case: Case,
    cx: &mut AsyncApp,
) -> (MainWindowShell, usize) {
    let window = shell.window();
    let title = format!("Beryl shell flight {} {case:?}", std::process::id());
    window
        .update(cx, |_, window, _| window.set_window_title(&title))
        .unwrap();
    let wide = title.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let raw = unsafe { FindWindowW(None, PCWSTR(wide.as_ptr())) }
        .unwrap()
        .0 as usize;
    assert!(native::alive(raw));
    assert!(!native::visible(raw));
    assert!(!shell.desktop_placement_ready());
    let cancellation = CommandCancellation::new();
    if matches!(case, Case::CancelBefore) {
        cancellation.cancel();
    }
    if matches!(case, Case::LeaseRefused) {
        let (lease, released) = window
            .update(cx, |_, window, _| window.lease_hidden_windows_window())
            .unwrap()
            .unwrap();
        drop(lease);
        released.await.unwrap();
    }
    let (release, wait) = mpsc::channel();
    shell.test_hold_desktop_worker(wait);
    let completion = Rc::new(RefCell::new(None));
    let retained = completion.clone();
    let (observation, dropped_observation) = mpsc::channel::<()>();
    drop(dropped_observation);
    let admitted = cx
        .update(|app| {
            shell.start_desktop_placement(
                cancellation.clone(),
                move |result, _| {
                    assert!(retained.borrow().is_none(), "exactly one terminal callback");
                    *retained.borrow_mut() = Some(result);
                    assert!(observation.send(()).is_err());
                },
                app,
            )
        })
        .unwrap();
    if matches!(case, Case::CancelBefore | Case::LeaseRefused) {
        let failure = admitted
            .err()
            .expect("admission refused with original shell");
        assert!(matches!(
            (&case, &failure.reason),
            (Case::CancelBefore, Rejection::Cancelled)
                | (Case::LeaseRefused, Rejection::LeaseAdmission(_))
        ));
        assert!(completion.borrow().is_none());
        assert!(!failure.shell.desktop_placement_ready());
        return (failure.shell, raw);
    }
    admitted.unwrap_or_else(|_| panic!("native flight admission"));
    native::pump(cx).await;
    assert!(
        completion.borrow().is_none(),
        "worker gate retains completion"
    );
    assert!(native::alive(raw));
    assert!(!native::visible(raw));
    match case {
        Case::CancelDuring => cancellation.cancel(),
        Case::CloseDuring => {
            unsafe {
                SendMessageW(
                    native::hwnd(raw),
                    WM_CLOSE,
                    Some(WPARAM(0)),
                    Some(LPARAM(0)),
                )
            };
        }
        Case::RemoveDuring => {
            window
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        }
        _ => {}
    }
    native::pump(cx).await;
    assert!(native::alive(raw), "native lease retains exact HWND");
    assert!(completion.borrow().is_none());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while completion.borrow().is_none() {
        assert!(
            Instant::now() < deadline,
            "shell desktop completion timeout"
        );
        native::pump(cx).await;
    }
    let completed = completion.borrow_mut().take().unwrap();
    let mut shell = match completed {
        Completion::Ready { shell, outcome } => {
            assert!(matches!(
                case,
                Case::Ready | Case::MissingDesktop | Case::CancelAfter | Case::CloseAfter
            ));
            assert!(shell.desktop_placement_ready());
            match case {
                Case::MissingDesktop => assert!(matches!(
                    outcome,
                    WindowsDesktopPlacementOutcome::CurrentDesktopDefault { failure: Some(_) }
                )),
                _ => assert_eq!(
                    outcome,
                    WindowsDesktopPlacementOutcome::CurrentDesktopDefault { failure: None }
                ),
            }
            shell
        }
        Completion::Rejected { shell, reason } => {
            assert!(matches!(
                (case, reason),
                (Case::CancelDuring, Rejection::Cancelled)
                    | (Case::CloseDuring, Rejection::NativeClose)
                    | (Case::RemoveDuring, Rejection::NativeWindowLost)
            ));
            assert!(!shell.desktop_placement_ready());
            shell
        }
    };
    if matches!(case, Case::CancelAfter) {
        cancellation.cancel();
    }
    if matches!(case, Case::CloseAfter) {
        unsafe {
            SendMessageW(
                native::hwnd(raw),
                WM_CLOSE,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            )
        };
    }
    if !matches!(case, Case::Ready | Case::MissingDesktop) {
        cx.update(|app| {
            assert!(!shell.ready_to_publish(app));
            assert!(shell.publish(app).is_err());
        })
        .unwrap();
    }
    assert!(!native::visible(raw));
    (shell, raw)
}

pub async fn wait_destroyed(raw: usize, cx: &AsyncApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while native::alive(raw) {
        assert!(
            Instant::now() < deadline,
            "shell native destruction timeout"
        );
        native::pump(cx).await;
    }
}
