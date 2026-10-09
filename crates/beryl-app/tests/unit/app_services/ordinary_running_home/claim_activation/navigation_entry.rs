use super::*;
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{VK_RETURN, VK_SPACE},
    WindowsAndMessaging::{WM_KEYDOWN, WM_KEYUP},
};

pub(super) async fn ready(
    window: WindowHandle<MainWindowShellRoot>,
    forward: bool,
    cx: &mut gpui::AsyncApp,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if window
            .read_with(cx, |root, app| {
                root.test_thread_navigation_reason(forward, app).is_none()
            })
            .unwrap()
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "exact native navigation target did not become available"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

pub(super) fn post_space(handle: windows::Win32::Foundation::HWND) {
    post_key(handle, VK_SPACE.0);
}

fn post_key(handle: windows::Win32::Foundation::HWND, key: u16) {
    unsafe {
        PostMessageW(Some(handle), WM_KEYDOWN, WPARAM(key as usize), LPARAM(1)).unwrap();
        PostMessageW(
            Some(handle),
            WM_KEYUP,
            WPARAM(key as usize),
            LPARAM(0xc0000001u32 as isize),
        )
        .unwrap();
    }
}

pub(super) async fn prepare(window: WindowHandle<MainWindowShellRoot>, cx: &mut gpui::AsyncApp) {
    let prior = window
        .read_with(cx, |root, app| {
            root.coherent_selected_thread_title(app)
                .unwrap()
                .0
                .thread_id()
        })
        .unwrap();
    let target = SyndicThreadId::from_bytes([242; 16]);
    assert_ne!(prior, target);
    let (picker, key) = switcher_entry::open(window, target, cx).await;
    window
        .update(cx, |_, _, app| {
            picker.update(app, |picker, pcx| picker.activate(&key, pcx))
        })
        .unwrap();
    ready(window, false, cx).await;
    window
        .update(cx, |root, window, _| {
            root.test_thread_navigation_focus(false).focus(window)
        })
        .unwrap();
    post_key(native(window, cx).await, VK_RETURN.0);
    ready(window, true, cx).await;
    window
        .read_with(cx, |root, app| {
            assert_eq!(
                root.coherent_selected_thread_title(app)
                    .unwrap()
                    .0
                    .thread_id(),
                prior
            );
            assert_eq!(root.test_thread_navigation_history(), vec![prior, target]);
        })
        .unwrap();
}

#[test]
fn native_forward_dirty_claim_recovery_preserves_original_window_and_commits_cursor_once() {
    run_navigation(Cut::ClaimCommitted);
}

#[test]
fn native_forward_dirty_save_noncommit_recovers_original_view_and_preserves_forward_cursor() {
    run_navigation(Cut::SaveNoncommit);
}

fn run_navigation(cut: Cut) {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        fixture::prepare_home,
        final_selection,
        move |owner, faults, cx| {
            Box::pin(async move {
                let (window, id, claim) = scenario::recover_with_entry(
                    owner,
                    faults,
                    cut,
                    control::Control::Normal,
                    SaveExpectation::Dirty,
                    scenario::Entry::Navigation,
                    cx,
                )
                .await;
                ready(window, !cut.committed(), cx).await;
                window
                    .read_with(cx, |root, app| {
                        assert_eq!(root.coherent_selected_thread_title(app).unwrap().0, claim);
                        if cut.committed() {
                            assert!(root.test_thread_navigation_reason(true, app).is_some());
                        } else {
                            assert!(root.test_thread_navigation_reason(false, app).is_some());
                        }
                    })
                    .unwrap();
                *qualified.borrow_mut() = Some((id, claim.thread_id()));
                activate_exit(window, cx);
            })
        },
        true,
        2,
    );
}
