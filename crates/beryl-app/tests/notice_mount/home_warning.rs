use super::*;
use std::time::Duration;

fn classify(cx: &mut gpui::TestAppContext, best_effort: bool) {
    cx.update(|app| test_publish_home_open_notice_classification(best_effort, app));
}

fn repeat(window: gpui::WindowHandle<MainWindowShellRoot>, cx: &mut gpui::TestAppContext) {
    window
        .update(cx, |root, window, cx| {
            root.test_repeat_home_warning_trigger(window, cx)
        })
        .unwrap();
}

fn timer(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &gpui::TestAppContext,
) -> BestEffortHomeWarningTimer {
    window
        .read_with(cx, |root, _| {
            root.test_home_warning_timer()
                .expect("visible warning timer")
        })
        .unwrap()
}

fn expire(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    timer: &BestEffortHomeWarningTimer,
    cx: &mut gpui::TestAppContext,
) {
    window
        .update(cx, |root, window, cx| {
            root.test_expire_home_warning(timer, window, cx)
        })
        .unwrap();
}

fn advance(cx: &mut gpui::TestAppContext, duration: Duration) {
    cx.executor().advance_clock(duration);
    support::draw(cx);
}

#[gpui::test]
fn home_warning_manual_dismissal_is_once_and_later_windows_receive_their_own(
    cx: &mut gpui::TestAppContext,
) {
    classify(cx, true);
    let mounted = support::mount(cx, 101);
    let ingress = support::ingress(mounted.window, cx);
    let visible = support::visible_token(mounted.window, cx);
    let stale = timer(mounted.window, cx);
    let geometry = support::shell_geometry(mounted.window, cx);
    mounted
        .window
        .read_with(cx, |root, _| {
            let projection = root.notice_projection().unwrap();
            assert_eq!(projection.kind, NoticeKind::Warning);
            assert_eq!(projection.content.dismissal, NoticeDismissal::Dismissible);
            assert!(
                projection
                    .content
                    .detail()
                    .as_str()
                    .contains("exclusive ownership")
            );
            assert!(
                projection
                    .content
                    .detail()
                    .as_str()
                    .contains("native local NTFS")
            );
            assert_eq!(root.notice_diagnostics().admitted, 1);
        })
        .unwrap();
    repeat(mounted.window, cx);
    support::draw(cx);
    assert_eq!(support::visible_token(mounted.window, cx), visible);
    cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(visible), app))
        .unwrap();
    repeat(mounted.window, cx);
    expire(mounted.window, &stale, cx);
    advance(cx, Duration::from_secs(5));
    assert!(projection(mounted.window, cx).is_none());
    assert_eq!(geometry, support::shell_geometry(mounted.window, cx));
    let later = support::mount_second(&mounted, cx);
    assert_eq!(projection(later, cx).unwrap().1, NoticeKind::Warning);
    assert!(projection(mounted.window, cx).is_none());
    support::finish(mounted, cx);
}

#[gpui::test]
fn home_warning_preemption_waits_and_restarts_a_full_visible_interval(
    cx: &mut gpui::TestAppContext,
) {
    classify(cx, true);
    let mounted = support::mount(cx, 103);
    let ingress = support::ingress(mounted.window, cx);
    let stale = timer(mounted.window, cx);
    advance(cx, Duration::from_secs(4));
    let error = admitted(
        &ingress,
        record(
            support::window_id(mounted.window, cx),
            NoticeKind::Error,
            "preempting error",
        ),
        cx,
    );
    assert!(
        mounted
            .window
            .read_with(cx, |root, _| root.test_home_warning_timer().is_none())
            .unwrap()
    );
    advance(cx, Duration::from_secs(10));
    expire(mounted.window, &stale, cx);
    assert_eq!(projection(mounted.window, cx).unwrap().1, NoticeKind::Error);
    cx.update(|app| ingress.remove(&error, app)).unwrap();
    assert_eq!(
        projection(mounted.window, cx).unwrap().1,
        NoticeKind::Warning
    );
    expire(mounted.window, &stale, cx);
    advance(cx, Duration::from_millis(4_999));
    assert_eq!(
        projection(mounted.window, cx).unwrap().1,
        NoticeKind::Warning
    );
    advance(cx, Duration::from_millis(1));
    assert!(projection(mounted.window, cx).is_none());
    repeat(mounted.window, cx);
    assert!(projection(mounted.window, cx).is_none());
    support::finish(mounted, cx);
}

#[gpui::test]
fn home_warning_revision_rejects_stale_expiry_and_advances_to_waiting_information(
    cx: &mut gpui::TestAppContext,
) {
    classify(cx, true);
    let mounted = support::mount(cx, 105);
    let ingress = support::ingress(mounted.window, cx);
    let stale = timer(mounted.window, cx);
    let token = support::visible_token(mounted.window, cx).record().clone();
    let _information = admitted(
        &ingress,
        record(
            support::window_id(mounted.window, cx),
            NoticeKind::Information,
            "waiting information",
        ),
        cx,
    );
    advance(cx, Duration::from_secs(4));
    cx.update(|app| {
        ingress.update(
            &token,
            2,
            NoticeContent::new(
                NoticeVariant::Warning,
                NoticeDismissal::Dismissible,
                "Revised durability warning",
                "Bounded revised detail",
            ),
            app,
        )
    })
    .unwrap();
    expire(mounted.window, &stale, cx);
    assert_eq!(
        support::visible_token(mounted.window, cx)
            .record()
            .revision(),
        2
    );
    advance(cx, Duration::from_millis(4_999));
    assert_eq!(
        projection(mounted.window, cx).unwrap().1,
        NoticeKind::Warning
    );
    advance(cx, Duration::from_millis(1));
    assert_eq!(
        projection(mounted.window, cx).unwrap().1,
        NoticeKind::Information
    );
    repeat(mounted.window, cx);
    assert_eq!(
        projection(mounted.window, cx).unwrap().1,
        NoticeKind::Information
    );
    support::finish(mounted, cx);
}

#[gpui::test]
fn home_warning_runs_while_visible_with_commands_inert_and_retirement_cancels_it(
    cx: &mut gpui::TestAppContext,
) {
    classify(cx, true);
    let mounted = support::mount(cx, 107);
    let stale = timer(mounted.window, cx);
    mounted
        .window
        .update(cx, |root, window, cx| {
            root.set_notices_inert(true, window, cx)
        })
        .unwrap();
    advance(cx, Duration::from_secs(5));
    assert!(projection(mounted.window, cx).is_none());
    repeat(mounted.window, cx);
    assert!(projection(mounted.window, cx).is_none());
    let later = support::mount_second(&mounted, cx);
    let later_timer = timer(later, cx);
    later
        .update(cx, |root, window, cx| root.retire_notices(window, cx))
        .unwrap();
    expire(later, &later_timer, cx);
    expire(mounted.window, &stale, cx);
    advance(cx, Duration::from_secs(5));
    assert!(projection(later, cx).is_none());
    support::finish(mounted, cx);
}

#[gpui::test]
fn native_home_classification_creates_no_warning_or_timer(cx: &mut gpui::TestAppContext) {
    classify(cx, false);
    let mounted = support::mount(cx, 109);
    repeat(mounted.window, cx);
    advance(cx, Duration::from_secs(5));
    assert!(projection(mounted.window, cx).is_none());
    mounted
        .window
        .read_with(cx, |root, _| {
            assert!(root.test_home_warning_timer().is_none());
            assert_eq!(root.notice_diagnostics().admitted, 0);
        })
        .unwrap();
    let later = support::mount_second(&mounted, cx);
    assert!(projection(later, cx).is_none());
    support::finish(mounted, cx);
}

#[gpui::test]
fn home_warning_queue_replacement_never_readmits_the_startup_trigger(
    cx: &mut gpui::TestAppContext,
) {
    classify(cx, true);
    let mounted = support::mount(cx, 111);
    let ingress = support::ingress(mounted.window, cx);
    let window_id = support::window_id(mounted.window, cx);
    let stale = timer(mounted.window, cx);
    let mut errors = Vec::with_capacity(NOTICE_GENERAL_CAPACITY);
    for _ in 0..NOTICE_GENERAL_CAPACITY {
        errors.push(admitted(
            &ingress,
            record(window_id, NoticeKind::Error, "bounded error"),
            cx,
        ));
    }
    mounted
        .window
        .read_with(cx, |root, _| {
            assert_eq!(root.notice_diagnostics().replaced, 1);
            assert_eq!(
                root.notice_diagnostics().retained_records,
                NOTICE_GENERAL_CAPACITY
            );
        })
        .unwrap();
    repeat(mounted.window, cx);
    expire(mounted.window, &stale, cx);
    for error in errors {
        cx.update(|app| ingress.remove(&error, app)).unwrap();
    }
    repeat(mounted.window, cx);
    advance(cx, Duration::from_secs(5));
    assert!(projection(mounted.window, cx).is_none());
    support::finish(mounted, cx);
}
