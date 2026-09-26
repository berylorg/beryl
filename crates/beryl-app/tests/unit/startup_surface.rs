use super::*;
use gpui::{TestAppContext, VisualTestContext};
use std::cell::RefCell;

fn mount(
    cx: &mut TestAppContext,
    detail: Option<&str>,
) -> (
    WindowHandle<StartupSurface>,
    Rc<RefCell<Vec<StartupSurfaceEvent>>>,
) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let output = events.clone();
    let window = cx.update(|cx| {
        let handler = move |event, _: &mut App| output.borrow_mut().push(event);
        match detail {
            Some(detail) => StartupSurface::open_failure(detail, handler, cx),
            None => StartupSurface::open_busy(handler, cx),
        }
        .unwrap()
    });
    cx.run_until_parked();
    (window, events)
}

fn key(visual: &mut VisualTestContext, value: &str) {
    let keystroke = gpui::Keystroke::parse(value).unwrap();
    visual.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
    });
    visual.simulate_event(gpui::KeyUpEvent { keystroke });
}

#[gpui::test]
fn mounted_retry_is_exact_and_pending_exit_is_idempotent(cx: &mut TestAppContext) {
    let (window, events) = mount(cx, Some("failed"));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let retry = visual.debug_bounds("startup-retry").unwrap();
    visual.simulate_click(retry.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let StartupSurfaceEvent::Retry(first) = events.borrow()[0] else {
        panic!("missing retry")
    };
    visual.simulate_click(retry.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(events.borrow().len(), 1);
    window
        .update(cx, |surface, _, cx| {
            assert!(surface.complete_failure(first, "second failure", cx));
            assert!(!surface.complete_failure(first, "stale", cx));
            let second = surface.request_retry(cx).unwrap();
            assert_ne!(first, second);
            assert!(!surface.complete_failure(first, "stale", cx));
            surface.request_exit(cx);
            assert!(!surface.complete_failure(second, "after exit", cx));
            surface.request_exit(cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        &*events.borrow(),
        &[StartupSurfaceEvent::Retry(first), StartupSurfaceEvent::Exit]
    );
    assert!(window.update(cx, |_, _, _| ()).is_ok());
}

#[gpui::test]
fn keyboard_and_pointer_share_admission_and_detail_is_read_only(cx: &mut TestAppContext) {
    let (window, events) = mount(cx, Some("selectable failure"));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    key(&mut visual, "enter");
    cx.run_until_parked();
    assert!(matches!(
        events.borrow().as_slice(),
        [StartupSurfaceEvent::Retry(_)]
    ));
    key(&mut visual, "tab");
    window
        .update(cx, |surface, window, cx| {
            assert!(
                surface
                    .detail
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .tab_focus_handle()
                    .is_focused(window)
            )
        })
        .unwrap();
    key(&mut visual, "ctrl-a");
    key(&mut visual, "ctrl-c");
    assert_eq!(
        visual
            .read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("selectable failure")
    );
    key(&mut visual, "backspace");
    key(&mut visual, "delete");
    window
        .update(cx, |surface, _, cx| {
            let input = surface.detail.as_ref().unwrap().read(cx);
            assert_eq!(input.text(), "selectable failure");
            assert_eq!(input.retained_counts().undo_snapshot_count, 0);
        })
        .unwrap();
    key(&mut visual, "tab");
    key(&mut visual, "space");
    cx.run_until_parked();
    assert_eq!(events.borrow().last(), Some(&StartupSurfaceEvent::Exit));
}

#[gpui::test]
fn reentrant_handler_can_complete_exact_attempt_without_borrow_conflict(cx: &mut TestAppContext) {
    let target = Rc::new(std::cell::Cell::new(None::<WindowHandle<StartupSurface>>));
    let owner = target.clone();
    let window = cx.update(|cx| {
        StartupSurface::open_failure(
            "initial",
            move |event, cx| {
                if let StartupSurfaceEvent::Retry(attempt) = event {
                    owner
                        .get()
                        .unwrap()
                        .update(cx, |surface, _, cx| {
                            assert!(surface.request_retry(cx).is_none());
                            assert!(surface.complete_failure(attempt, &"漢".repeat(5000), cx));
                        })
                        .unwrap();
                }
            },
            cx,
        )
        .unwrap()
    });
    target.set(Some(window));
    for _ in 0..3 {
        window
            .update(cx, |surface, _, cx| {
                surface.request_retry(cx).unwrap();
            })
            .unwrap();
        cx.run_until_parked();
        window
            .update(cx, |surface, _, cx| {
                assert!(surface.pending.is_none());
                let input = surface.detail.as_ref().unwrap().read(cx);
                assert!(input.text().len() <= MAX_DETAIL_BYTES);
                assert!(input.text().ends_with(TRUNCATED));
                assert_eq!(input.retained_counts().undo_snapshot_count, 0);
                assert_eq!(input.retained_counts().redo_snapshot_count, 0);
            })
            .unwrap();
    }
}

#[gpui::test]
fn disposal_cancels_busy_deadline_and_failure_never_auto_exits(cx: &mut TestAppContext) {
    let (busy, busy_events) = mount(cx, None);
    let weak = busy.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
    busy.update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
    let (_, failure_events) = mount(cx, Some("failure"));
    cx.executor().advance_clock(Duration::from_secs(60));
    cx.run_until_parked();
    assert!(busy_events.borrow().is_empty());
    assert!(failure_events.borrow().is_empty());
}

#[test]
fn unicode_detail_is_capped_including_truncation_suffix() {
    for value in ["é", "🙂", "a"] {
        let detail = bounded_detail(&value.repeat(5000));
        assert!(detail.len() <= MAX_DETAIL_BYTES);
        assert!(detail.ends_with(TRUNCATED));
    }
    assert_eq!(
        bounded_detail(&"a".repeat(MAX_DETAIL_BYTES)).len(),
        MAX_DETAIL_BYTES
    );
}

#[gpui::test]
fn pending_retry_explains_unavailability_and_rejects_foreign_attempt(cx: &mut TestAppContext) {
    let (window, _) = mount(cx, Some("first"));
    let (other, _) = mount(cx, Some("other"));
    let foreign = other
        .update(cx, |surface, _, cx| surface.request_retry(cx).unwrap())
        .unwrap();
    window
        .update(cx, |surface, _, cx| {
            surface.request_retry(cx).unwrap();
            assert!(!surface.complete_failure(foreign, "wrong surface", cx));
        })
        .unwrap();
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let retry = visual.debug_bounds("startup-retry").unwrap();
    visual.simulate_mouse_move(retry.center(), None, gpui::Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(501));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
    });
    assert!(visual.debug_bounds("startup-command-explanation").is_some());
}

#[gpui::test]
fn maximum_detail_keeps_controls_inside_fixed_window(cx: &mut TestAppContext) {
    let (window, _) = mount(cx, Some(&"🙂\n".repeat(5000)));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let root = visual.debug_bounds("startup-surface").unwrap();
    for selector in ["startup-detail", "startup-retry", "startup-exit"] {
        let bounds = visual.debug_bounds(selector).unwrap();
        assert!(root.contains(&bounds.origin));
        assert!(bounds.bottom() <= root.bottom());
        assert!(bounds.right() <= root.right());
    }
}

#[gpui::test]
fn busy_timer_exits_once_and_exit_cancels_it(cx: &mut TestAppContext) {
    let (window, events) = mount(cx, None);
    cx.executor().advance_clock(Duration::from_secs(4));
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(&*events.borrow(), &[StartupSurfaceEvent::Exit]);
    window
        .update(cx, |surface, _, cx| {
            assert!(surface.timer.is_none());
            surface.request_exit(cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(events.borrow().len(), 1);
    let (early, events) = mount(cx, None);
    early
        .update(cx, |surface, _, cx| surface.request_exit(cx))
        .unwrap();
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert_eq!(&*events.borrow(), &[StartupSurfaceEvent::Exit]);
}
