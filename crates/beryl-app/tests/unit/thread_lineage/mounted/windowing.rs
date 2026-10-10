use super::*;

#[gpui::test]
fn mounted_refused_page_releases_slot_and_waits_for_original_owner_resume(cx: &mut TestAppContext) {
    let mounted = Mounted::new(cx, 100);
    let requests = mounted
        .widget
        .read_with(cx, |widget, _| widget.model.requests.clone());
    for request in requests {
        mounted.update(cx, |widget, window, cx| {
            widget.settle_page(request, None, false, window, cx)
        });
    }
    mounted.events.borrow_mut().clear();
    for _ in 0..8 {
        mounted.draw(cx);
    }
    assert!(mounted.events.borrow().is_empty());
    assert!(
        mounted
            .widget
            .read_with(cx, |widget, _| widget.model.requests.is_empty())
    );
    mounted.update(cx, |widget, _, cx| widget.resume(cx));
    assert!(
        !mounted
            .widget
            .read_with(cx, |widget, _| widget.model.requests.is_empty())
    );
}

#[gpui::test]
fn mounted_partial_page_resumes_exact_ordinal_and_only_current_far_target_gains_focus(
    cx: &mut TestAppContext,
) {
    let mounted = Mounted::new(cx, 32);
    mounted.update(cx, |widget, window, _| widget.proxy.focus(window));
    mounted.key(cx, "end");
    let request = mounted
        .widget
        .read_with(cx, |widget, _| widget.model.requests[0]);
    mounted.update(cx, |widget, window, cx| {
        widget.settle_page(request, Some(page(request, 5)), false, window, cx)
    });
    let continuation = mounted.widget.read_with(cx, |widget, _| {
        assert!(widget.model.target.is_some());
        assert!(widget.model.focus.is_none());
        assert_eq!(widget.model.requests.len(), 1);
        widget.model.requests[0]
    });
    assert_eq!(continuation.start, 5);
    assert_eq!(continuation.end, 32);
    mounted.update(cx, |widget, window, cx| {
        widget.replace(query(2, 32), "Current".into(), window, cx)
    });
    mounted.update(cx, |widget, window, cx| {
        widget.settle_page(
            continuation,
            Some(page(continuation, 27)),
            false,
            window,
            cx,
        )
    });
    mounted
        .widget
        .read_with(cx, |widget, _| assert!(widget.model.focus.is_none()));
    mounted.key(cx, "end");
    mounted.fill(cx);
    mounted.widget.read_with(cx, |widget, _| {
        assert_eq!(widget.model.focus.unwrap().thread, id(32))
    });
    assert!(mounted.activations().is_empty());
}

#[gpui::test]
fn mounted_windowing_retires_tooltip_and_keeps_only_logical_focus_until_matching_proxy_reentry(
    cx: &mut TestAppContext,
) {
    let mounted = Mounted::new(cx, 10_000);
    mounted.fill(cx);
    mounted.update(cx, |widget, window, _| widget.proxy.focus(window));
    mounted.key(cx, "end");
    mounted.fill(cx);
    mounted
        .window
        .update(cx, |_, window, app| {
            assert!(
                mounted
                    .widget
                    .read(app)
                    .diagnostics(window, 1)
                    .tooltip_anchor_present
            );
        })
        .unwrap();
    for _ in 0..30 {
        mounted.wheel(cx, 6400., 0., false);
        let manual = mounted
            .widget
            .read_with(cx, |widget, _| widget.first_ordinal);
        mounted.fill(cx);
        mounted
            .window
            .update(cx, |_, window, app| {
                let widget = mounted.widget.read(app);
                assert_eq!(widget.first_ordinal, manual);
                assert_eq!(widget.model.focus.unwrap().thread, id(10_000));
                assert!(widget.proxy.is_focused(window));
                assert!(widget.handles.iter().all(|(key, _)| *key != id(10_000)));
                assert!(!widget.diagnostics(window, 1).tooltip_anchor_present);
                assert!(widget.model.resident_count() <= 24);
                assert!(widget.model.requests.len() <= 2);
            })
            .unwrap();
    }
    mounted.key(cx, "right");
    mounted.fill(cx);
    mounted
        .window
        .update(cx, |_, window, app| {
            let widget = mounted.widget.read(app);
            assert_eq!(widget.model.focus.unwrap().thread, id(10_000));
            assert!(!widget.proxy.is_focused(window));
            assert!(
                widget
                    .handles
                    .iter()
                    .any(|(key, focus)| *key == id(10_000) && focus.is_focused(window))
            );
        })
        .unwrap();
    assert!(mounted.activations().is_empty());
}

fn assert_trailing_endpoint(widget: &ThreadLineage) {
    let endpoint_right = (widget.query().parent_count - widget.first_ordinal) as f64
        * widget.stride() as f64
        - widget.fraction as f64
        + widget.current_width() as f64;
    assert!(
        (endpoint_right - widget.viewport_width() as f64).abs() < 0.1,
        "readonly endpoint must end at measured viewport edge: endpoint={endpoint_right}, viewport={}",
        widget.viewport_width()
    );
}

#[gpui::test]
fn mounted_selected_endpoint_reveals_against_measured_narrow_container(cx: &mut TestAppContext) {
    let mounted = Mounted::new(cx, 40);
    mounted.draw(cx);
    mounted
        .widget
        .read_with(cx, |widget, _| assert_trailing_endpoint(widget));
    mounted
        .window
        .update(cx, |root, _, cx| {
            root.maximum_width = 450.;
            cx.notify();
        })
        .unwrap();
    mounted.update(cx, |widget, window, cx| {
        widget.replace(
            LineageQuery {
                revision: 2,
                selected: id(900),
                parent_count: 80,
            },
            "Current changed".into(),
            window,
            cx,
        )
    });
    mounted.draw(cx);
    mounted.draw(cx);
    mounted.widget.read_with(cx, |widget, _| {
        assert!(widget.viewport_width() < 450.);
        assert_trailing_endpoint(widget);
    });
    mounted.update(cx, |widget, window, _| widget.proxy.focus(window));
    mounted.key(cx, "home");
    mounted.fill(cx);
    assert_eq!(
        mounted
            .widget
            .read_with(cx, |widget, _| widget.first_ordinal),
        0
    );
}

#[gpui::test]
fn mounted_horizontal_scrollbar_lane_preserves_registered_noop_behavior(cx: &mut TestAppContext) {
    let mounted = Mounted::new(cx, 1000);
    mounted.update(cx, |widget, window, _| widget.proxy.focus(window));
    mounted.key(cx, "home");
    mounted.fill(cx);
    let (lane, owner) = mounted.widget.read_with(cx, |widget, _| {
        let y = widget.bounds.bottom_right().y - px(4.);
        (
            point(
                widget.bounds.origin.x + widget.bounds.size.width - px(20.),
                y,
            ),
            widget.owner,
        )
    });
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    visual.simulate_mouse_down(lane, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    visual.simulate_mouse_up(lane, gpui::MouseButton::Left, Default::default());
    mounted.draw(cx);
    assert_eq!(
        mounted.widget.read_with(cx, |widget, _| widget.offset()),
        0.
    );
    assert!(mounted.activations().is_empty());
    assert_eq!(
        mounted.widget.read_with(cx, |widget, _| widget.owner),
        owner
    );
}

#[gpui::test]
fn mounted_scrollbar_thumb_drag_changes_original_viewport_and_keeps_exact_owner(
    cx: &mut TestAppContext,
) {
    let mounted = Mounted::new(cx, 1000);
    mounted.update(cx, |widget, window, _| widget.proxy.focus(window));
    mounted.key(cx, "home");
    mounted.fill(cx);
    let (thumb, owner) = mounted.widget.read_with(cx, |widget, _| {
        let snapshot = widget
            .interaction
            .current_snapshot(Axis::Horizontal, widget.scrollbar_style())
            .unwrap();
        (
            point(
                widget.bounds.origin.x
                    + (snapshot.thumb_bounds.start + snapshot.thumb_bounds.end) / 2.,
                widget.bounds.bottom_right().y - px(4.),
            ),
            widget.owner,
        )
    });
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    visual.simulate_mouse_down(thumb, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    visual.simulate_mouse_move(
        thumb + point(px(120.), px(0.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    visual.simulate_mouse_move(
        thumb + point(px(160.), px(0.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    visual.simulate_mouse_up(
        thumb + point(px(160.), px(0.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    mounted.draw(cx);
    mounted.widget.read_with(cx, |widget, _| {
        assert_eq!(widget.owner, owner);
        assert!(widget.offset() > 1000.);
        assert!(widget.model.resident_count() <= 24);
        assert!(widget.model.requests.len() <= 2);
    });
    assert!(mounted.activations().is_empty());
}

#[gpui::test]
fn mounted_hover_and_focused_disabled_tooltips_explain_closest_gate_and_source_failure(
    cx: &mut TestAppContext,
) {
    let mounted = Mounted::new(cx, 100);
    mounted.update(cx, |widget, window, _| widget.proxy.focus(window));
    mounted.key(cx, "home");
    mounted.fill(cx);
    let observed = Rc::new(RefCell::new(Vec::<String>::new()));
    let sink = observed.clone();
    let _observer = cx.update(|app| {
        app.observe_new::<super::super::super::render::LineageTooltip>(move |tooltip, _, _| {
            sink.borrow_mut().push(tooltip.text.clone());
        })
    });
    mounted
        .window
        .update(cx, |root, window, _| root.other.focus(window))
        .unwrap();
    mounted.update(cx, |widget, _, cx| {
        widget.set_inert_reason(
            Some("This window is settling its original thread selection.".into()),
            cx,
        )
    });
    mounted.draw(cx);
    let position = mounted
        .window
        .update(cx, |_, window, app| {
            mounted
                .widget
                .read(app)
                .test_parent_input(id(1), window)
                .unwrap()
                .0
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    visual.simulate_mouse_move(position, None, Default::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(501));
    mounted.draw(cx);
    assert!(
        observed
            .borrow()
            .last()
            .unwrap()
            .contains("settling its original thread selection")
    );
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    assert!(visual.debug_bounds("thread-lineage-tooltip").is_some());
    mounted.click_ordinal(cx, 0);
    mounted.key(cx, "enter");
    assert!(mounted.activations().is_empty());
    mounted.update(cx, |widget, window, cx| {
        widget.test_focus_parent(id(2), window, cx)
    });
    mounted.draw(cx);
    assert!(
        observed
            .borrow()
            .last()
            .unwrap()
            .contains("settling its original thread selection")
    );
    assert!(!observed.borrow().last().unwrap().contains("Open elsewhere"));
    mounted.update(cx, |widget, _, cx| widget.set_inert(false, cx));
    mounted.draw(cx);
    assert!(observed.borrow().last().unwrap().contains("Open elsewhere"));
    mounted.update(cx, |widget, window, cx| {
        widget.test_focus_parent(id(1), window, cx);
        let request = widget.model.request(32).unwrap();
        let mut response = page(request, 32);
        response.next_ordinal += 1;
        widget.settle_page(request, Some(response), false, window, cx);
    });
    mounted.draw(cx);
    assert!(
        observed
            .borrow()
            .last()
            .unwrap()
            .contains("ancestry could not be read")
    );
    mounted.key(cx, "space");
    assert!(mounted.activations().is_empty());
}

#[gpui::test]
fn mounted_unvalidated_selected_endpoint_is_immediate_and_parent_commands_remain_inert(
    cx: &mut TestAppContext,
) {
    let mounted = Mounted::new(cx, 40);
    mounted.update(cx, |widget, window, cx| {
        widget.set_inert(true, cx);
        widget.replace(
            LineageQuery {
                selected: id(700),
                revision: 2,
                parent_count: 65,
            },
            "Selected child".into(),
            window,
            cx,
        );
    });
    mounted.draw(cx);
    mounted
        .window
        .update(cx, |_, window, app| {
            let widget = mounted.widget.read(app);
            assert_eq!(widget.query().selected, id(700));
            assert_eq!(widget.current_title, "Selected child");
            assert!(widget.diagnostics(window, 1).current_endpoint_present);
            assert!(widget.model.requests.is_empty());
            assert!(widget.handles.is_empty());
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    assert!(visual.debug_bounds("thread-lineage-current").is_some());
}

#[gpui::test]
fn mounted_deep_keyboard_navigation_preserves_integer_identity_and_proxy_custody(
    cx: &mut TestAppContext,
) {
    let count = (1u64 << 48) + 35;
    let mounted = Mounted::new(cx, count);
    mounted.fill(cx);
    mounted.update(cx, |widget, window, _| widget.proxy.focus(window));
    mounted.key(cx, "home");
    mounted.key(cx, "enter");
    assert!(mounted.activations().is_empty());
    mounted.fill(cx);
    mounted.widget.read_with(cx, |widget, _| {
        assert_eq!(widget.first_ordinal, 0);
        assert_eq!(widget.model.focus.unwrap().ordinal, 0);
        assert!(widget.realized <= (widget.ranges().1.end - widget.ranges().1.start) as usize);
    });
    mounted.key(cx, "enter");
    assert_eq!(mounted.activations(), [id(1)]);
    mounted.key(cx, "right");
    mounted.key(cx, "space");
    assert_eq!(mounted.activations(), [id(1)]);
    mounted.key(cx, "end");
    mounted.fill(cx);
    mounted.widget.read_with(cx, |widget, _| {
        assert_eq!(widget.model.focus.unwrap().ordinal, count - 1);
        assert!(widget.first_ordinal > (1u64 << 48) - 10);
        assert!(widget.model.resident_count() <= 24);
        assert!(widget.model.requests.len() <= 2);
        assert!(widget.handles.len() <= (widget.ranges().1.end - widget.ranges().1.start) as usize);
    });
    mounted.key(cx, "home");
    mounted.fill(cx);
    assert_eq!(
        mounted
            .widget
            .read_with(cx, |widget, _| widget.first_ordinal),
        0
    );
}

#[gpui::test]
fn mounted_pointer_keyboard_unavailable_and_refresh_use_exact_resident_keys(
    cx: &mut TestAppContext,
) {
    let mounted = Mounted::new(cx, 100);
    mounted.update(cx, |widget, window, _| widget.proxy.focus(window));
    mounted.key(cx, "home");
    mounted.fill(cx);
    mounted.click_ordinal(cx, 0);
    assert_eq!(
        mounted.activations(),
        [id(1)],
        "pointer must activate exact parent"
    );
    mounted.key(cx, "enter");
    assert_eq!(
        mounted.activations(),
        [id(1), id(1)],
        "focused Enter must activate exact parent"
    );
    mounted.key(cx, "space");
    assert_eq!(mounted.activations(), [id(1), id(1), id(1)]);
    mounted.click_ordinal(cx, 1);
    mounted.key(cx, "enter");
    mounted.key(cx, "space");
    assert_eq!(mounted.activations(), [id(1), id(1), id(1)]);
    mounted.update(cx, |widget, window, cx| {
        widget.replace(query(2, 100), "Current refreshed".into(), window, cx)
    });
    mounted.key(cx, "enter");
    assert_eq!(mounted.activations().len(), 3);
    mounted.update(cx, |widget, window, cx| {
        widget.replace(query(3, 100), "Current validated".into(), window, cx)
    });
    mounted.key(cx, "space");
    assert_eq!(mounted.activations().len(), 3);
    mounted.fill(cx);
    mounted.update(cx, |widget, window, _| {
        assert!(widget.test_logical_focus(id(2), window));
        assert_eq!(widget.model.focus.unwrap().thread, id(2));
        assert_eq!(widget.model.focus.unwrap().query.revision, 3);
    });
}

#[gpui::test]
fn mounted_wide_viewport_resize_and_manual_wheel_use_production_window_bounds(
    cx: &mut TestAppContext,
) {
    let mounted = Mounted::new(cx, 10_000);
    cx.simulate_window_resize(mounted.window.into(), size(px(10_092.), px(400.)));
    mounted.draw(cx);
    mounted.update(cx, |widget, window, cx| {
        widget.auto_reveal = false;
        widget.first_ordinal = 10;
        widget.fraction = 0.;
        widget.proxy.focus(window);
        cx.notify();
    });
    mounted.draw(cx);
    mounted.fill(cx);
    mounted.widget.read_with(cx, |widget, _| {
        let (visible, realized) = widget.ranges();
        assert!(visible.end - visible.start >= 49);
        assert_eq!(widget.realized as u64, realized.end - realized.start);
        assert!(widget.realized > 24);
    });
    let before = mounted
        .widget
        .read_with(cx, |widget, _| (widget.first_ordinal, widget.fraction));
    mounted.wheel(cx, 0., -200., false);
    assert_eq!(
        mounted
            .widget
            .read_with(cx, |widget, _| (widget.first_ordinal, widget.fraction)),
        before
    );
    mounted.wheel(cx, -200., 0., false);
    let manual = mounted
        .widget
        .read_with(cx, |widget, _| widget.first_ordinal);
    assert_eq!(manual, before.0 + 1);
    mounted.draw(cx);
    assert_eq!(
        mounted
            .widget
            .read_with(cx, |widget, _| widget.first_ordinal),
        manual
    );
    mounted.wheel(cx, 0., -200., true);
    assert_eq!(
        mounted
            .widget
            .read_with(cx, |widget, _| widget.first_ordinal),
        manual + 1
    );
    cx.simulate_window_resize(mounted.window.into(), size(px(600.), px(400.)));
    mounted.draw(cx);
    mounted.widget.read_with(cx, |widget, _| {
        assert_eq!(widget.first_ordinal, manual + 1);
        assert!(widget.realized <= 8);
    });
}
