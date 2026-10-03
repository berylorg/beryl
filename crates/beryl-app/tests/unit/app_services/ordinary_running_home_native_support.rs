use super::*;
use crate::main_window::{NoticeDismissal, NoticeKind, NoticeVariant};
use windows::Win32::UI::WindowsAndMessaging::{
    GW_ENABLEDPOPUP, GW_OWNER, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

pub(super) async fn wait_for_notice(
    windows: &[WindowHandle<MainWindowShellRoot>],
    recovered: bool,
    cx: &mut AsyncApp,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = cx
            .update(|app| {
                let mut condition = None;
                for window in windows {
                    let root = window.read(app).unwrap();
                    let Some(projection) = root.notice_projection() else {
                        return false;
                    };
                    let (kind, variant, dismissal) = if recovered {
                        (
                            NoticeKind::Recovery,
                            NoticeVariant::Info,
                            NoticeDismissal::Dismissible,
                        )
                    } else {
                        (
                            NoticeKind::HomeFailure,
                            NoticeVariant::Error,
                            NoticeDismissal::Persistent,
                        )
                    };
                    if projection.kind != kind {
                        return false;
                    }
                    if !recovered {
                        let detail = projection.content.detail().as_str();
                        if !detail.contains("recovering this home")
                            && !detail.contains("retrying recovery")
                        {
                            return false;
                        }
                    }
                    assert_eq!(projection.content.variant, variant);
                    assert_eq!(projection.content.dismissal, dismissal);
                    assert_eq!(projection.content.commands().count(), 0);
                    let current = projection.token.record().condition().clone();
                    if let Some(prior) = &condition {
                        assert_eq!(prior, &current);
                    }
                    condition = Some(current);
                    assert_eq!(root.test_exit_command_enabled(), recovered);
                }
                true
            })
            .unwrap();
        if ready {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "all preserved windows did not project the ordinary recovery notice"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

pub(super) async fn confirm_fixture_exit(owner: HWND, cx: &mut AsyncApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(dialog) = unsafe { GetWindow(owner, GW_ENABLEDPOPUP) } {
            if dialog != owner && fixture_owns_dialog(owner, dialog) {
                unsafe {
                    PostMessageW(
                        Some(dialog),
                        TDM_CLICK_BUTTON.0 as u32,
                        WPARAM(IDOK.0 as usize),
                        LPARAM(0),
                    )
                }
                .unwrap();
                wait_until(
                    cx,
                    || !unsafe { IsWindow(Some(dialog)).as_bool() },
                    "owned Exit confirmation settled",
                )
                .await;
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "fixture-owned Exit confirmation did not appear"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

fn fixture_owns_dialog(owner: HWND, dialog: HWND) -> bool {
    let mut owner_pid = 0;
    let mut dialog_pid = 0;
    let mut title = [0u16; 128];
    unsafe {
        GetWindowThreadProcessId(owner, Some(&mut owner_pid));
        GetWindowThreadProcessId(dialog, Some(&mut dialog_pid));
    }
    let count = unsafe { GetWindowTextW(dialog, &mut title) };
    owner_pid == std::process::id()
        && dialog_pid == owner_pid
        && unsafe { GetWindow(dialog, GW_OWNER) }.ok() == Some(owner)
        && unsafe { IsWindowVisible(dialog).as_bool() }
        && String::from_utf16_lossy(&title[..count as usize]) == "Exit Beryl?"
}
