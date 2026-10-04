use beryl_app::thread_root_picker::*;
use gpui::{
    AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, Subscription, Window, WindowHandle, div, px,
};
use std::{cell::RefCell, rc::Rc};

struct Root {
    picker: Entity<ThreadRootPicker>,
    trigger: FocusHandle,
    _events: Subscription,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("picker-trigger")
                    .track_focus(&self.trigger)
                    .tab_stop(true)
                    .h(px(32.))
                    .child("Running threads"),
            )
            .child(self.picker.clone())
            .child(gpui::canvas(
                |_, window, _| {
                    for (size, weight) in [
                        (19., 650.),
                        (12., 400.),
                        (11., 700.),
                        (14., 520.),
                        (10., 650.),
                    ] {
                        let text =
                            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789- .⌕▤…";
                        let mut font = gpui::font("Inter");
                        font.weight = gpui::FontWeight(weight);
                        let line = window.text_system().shape_line(
                            text.into(),
                            px(size),
                            &[gpui::TextRun {
                                len: text.len(),
                                font,
                                color: gpui::black(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        );
                        window.text_system().set_glyph_raster_bounds_for_test(
                            &line,
                            window.scale_factor(),
                            gpui::Bounds::new(
                                gpui::point(
                                    gpui::DevicePixels(0),
                                    gpui::DevicePixels(-(size as i32)),
                                ),
                                gpui::size(
                                    gpui::DevicePixels(size as i32),
                                    gpui::DevicePixels(size as i32),
                                ),
                            ),
                        );
                    }
                },
                |_, _, _, _| {},
            ))
    }
}

struct Mounted {
    window: WindowHandle<Root>,
    picker: Entity<ThreadRootPicker>,
    trigger: FocusHandle,
    events: Rc<RefCell<Vec<PickerEvent>>>,
}

impl Mounted {
    fn new(cx: &mut gpui::TestAppContext, total: usize) -> Self {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        let (window, picker, trigger) = cx.update(|app| {
            let mut retained = None;
            let window = app
                .open_window(gpui::WindowOptions::default(), |window, app| {
                    let trigger = app.focus_handle();
                    let picker = app.new(|cx| {
                        ThreadRootPicker::new(
                            ThreadRootPickerConfig {
                                title: "Running threads".into(),
                                helper: "Choose a thread".into(),
                                heading: "RUNNING THREADS".into(),
                                empty_text: "No running threads".into(),
                                search_placeholder: "Search".into(),
                                owner_focus: trigger.clone(),
                                appearance: None,
                                style: ThreadRootPickerStyle::default(),
                                scrollbar_style: gpui_scrollbar::ScrollbarStyle::default(),
                            },
                            PickerCollectionKey("opaque-collection".into()),
                            1,
                            total,
                            window,
                            cx,
                        )
                    });
                    retained = Some((picker.clone(), trigger.clone()));
                    app.new(|cx| {
                        let subscription = cx.subscribe(&picker, move |_, _, event, _| {
                            sink.borrow_mut().push(event.clone())
                        });
                        Root {
                            picker,
                            trigger,
                            _events: subscription,
                        }
                    })
                })
                .expect("picker fixture window");
            let (picker, trigger) = retained.unwrap();
            (window, picker, trigger)
        });
        let mounted = Self {
            window,
            picker,
            trigger,
            events,
        };
        mounted.update(cx, |picker, _, cx| picker.request_initial_page(cx));
        mounted
    }

    fn update(
        &self,
        cx: &mut gpui::TestAppContext,
        f: impl FnOnce(&mut ThreadRootPicker, &mut Window, &mut Context<ThreadRootPicker>),
    ) {
        self.window
            .update(cx, |_, window, app| {
                self.picker.update(app, |picker, cx| f(picker, window, cx))
            })
            .expect("picker update");
        cx.run_until_parked();
    }

    fn request(&self) -> PickerPageRequest {
        self.events
            .borrow()
            .iter()
            .rev()
            .find_map(|event| match event {
                PickerEvent::RequestPage(request) => Some(request.clone()),
                _ => None,
            })
            .expect("page request")
    }

    fn settle(&self, request: PickerPageRequest, total: usize, cx: &mut gpui::TestAppContext) {
        let rows = (request.range.start..request.range.end.min(total))
            .map(|position| PickerRow {
                key: PickerRowKey(format!("row-{position}")),
                primary: format!("Thread {position}"),
                secondary: "Host runtime".into(),
                status: "RUNNING".into(),
                tooltip: None,
                unavailable_reason: None,
                current: position == 0,
                activation_pending: false,
            })
            .collect();
        self.update(cx, |picker, window, cx| {
            picker.settle_page(
                PickerPageOutcome::Success(PickerPage {
                    request,
                    total_count: total,
                    rows,
                }),
                window,
                cx,
            )
        });
    }

    fn diagnostics(&self, cx: &mut gpui::TestAppContext) -> PickerDiagnostics {
        self.picker.read_with(cx, |picker, _| picker.diagnostics())
    }

    fn draw(&self, cx: &mut gpui::TestAppContext) {
        cx.run_until_parked();
        cx.update(|app| {
            app.update_window(self.window.into(), |_, window, app| {
                window.draw(app).clear()
            })
            .expect("draw picker fixture");
        });
    }
}

#[gpui::test]
fn mounted_empty_results_stop_requesting_on_repeated_frames_and_resume_for_a_new_query(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx, 0);
    mounted.settle(mounted.request(), 0, cx);
    mounted.events.borrow_mut().clear();
    for _ in 0..32 {
        mounted.draw(cx);
    }
    cx.run_until_parked();
    assert!(
        mounted
            .events
            .borrow()
            .iter()
            .all(|event| !matches!(event, PickerEvent::RequestPage(_)))
    );
    assert_eq!(mounted.diagnostics(cx).pending_page_count, 0);
    assert_eq!(mounted.diagnostics(cx).realized_row_count, 0);
    mounted.update(cx, |picker, window, cx| {
        picker.replace_collection(
            PickerCollectionKey("opaque-collection".into()),
            2,
            0,
            None,
            window,
            cx,
        )
    });
    let request = mounted.request();
    assert_eq!(request.query_revision, 2);
    mounted.settle(request, 0, cx);
    mounted.events.borrow_mut().clear();
    for _ in 0..32 {
        mounted.draw(cx);
    }
    cx.run_until_parked();
    assert!(
        mounted
            .events
            .borrow()
            .iter()
            .all(|event| !matches!(event, PickerEvent::RequestPage(_)))
    );
    assert_eq!(mounted.diagnostics(cx).pending_page_count, 0);
}

#[gpui::test]
fn mounted_navigation_settlement_preserves_actual_focus_and_dispatches_exact_key(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx, 1_000_000);
    mounted.settle(mounted.request(), 1_000_000, cx);
    mounted.update(cx, |picker, window, cx| picker.focus_row(3, window, cx));
    mounted.update(cx, |picker, window, cx| {
        picker.navigate(PickerNavigation::End, window, cx)
    });
    let request = mounted.request();
    assert_eq!(
        mounted.diagnostics(cx).focused_key,
        Some(PickerRowKey("row-3".into()))
    );
    assert_eq!(
        mounted.diagnostics(cx).pending_navigation.unwrap().position,
        999_999
    );
    mounted.settle(request, 1_000_000, cx);
    assert_eq!(
        mounted.diagnostics(cx).focused_key,
        Some(PickerRowKey("row-999999".into()))
    );
    mounted.update(cx, |picker, window, cx| {
        assert!(picker.collection_focus().is_focused(window));
        let key = picker.focused_key().unwrap().clone();
        picker.activate(&key, cx);
        picker.activate(&key, cx);
    });
    let activation: Vec<_> = mounted
        .events
        .borrow()
        .iter()
        .filter(|event| matches!(event, PickerEvent::Activate(_)))
        .cloned()
        .collect();
    assert_eq!(
        activation,
        vec![PickerEvent::Activate(PickerRowKey("row-999999".into()))]
    );
    let diagnostics = mounted.diagnostics(cx);
    assert!(diagnostics.resident_page_count <= PICKER_MAX_RESIDENT_PAGES);
    assert!(diagnostics.resident_row_count <= PICKER_MAX_RESIDENT_PAGES * PICKER_PAGE_ROWS);
    assert!(diagnostics.realized_range.len() <= 17);
}

#[gpui::test]
fn mounted_search_departure_and_dismissal_cancel_pending_focus_and_return_exact_trigger(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx, 1000);
    mounted.settle(mounted.request(), 1000, cx);
    mounted.update(cx, |picker, window, cx| picker.focus_row(2, window, cx));
    mounted.update(cx, |picker, window, cx| {
        picker.navigate(PickerNavigation::End, window, cx)
    });
    let end = mounted.request();
    mounted.update(cx, |picker, window, cx| picker.focus_search(window, cx));
    mounted.settle(end, 1000, cx);
    assert_eq!(
        mounted.diagnostics(cx).focused_key,
        Some(PickerRowKey("row-2".into()))
    );
    assert!(mounted.diagnostics(cx).pending_navigation.is_none());
    mounted.update(cx, |picker, window, cx| picker.dismiss(window, cx));
    mounted
        .window
        .update(cx, |_, window, _| {
            assert!(mounted.trigger.is_focused(window))
        })
        .unwrap();
    assert_eq!(
        mounted
            .events
            .borrow()
            .iter()
            .filter(|event| matches!(event, PickerEvent::Dismiss))
            .count(),
        1
    );
}

#[gpui::test]
fn mounted_query_revisions_obsolete_pages_and_failure_keeps_coherent_facts(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx, 1000);
    mounted.settle(mounted.request(), 1000, cx);
    mounted.update(cx, |picker, window, cx| picker.focus_row(4, window, cx));
    let before = mounted.diagnostics(cx);
    mounted.update(cx, |picker, window, cx| picker.focus_search(window, cx));
    mounted.draw(cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    visual.simulate_input("missing");
    let first = mounted.request();
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("other");
    let second = mounted.request();
    assert!(second.query_revision > first.query_revision);
    mounted.settle(first, 1, cx);
    assert_eq!(mounted.diagnostics(cx).total_count, before.total_count);
    mounted.update(cx, |picker, window, cx| {
        picker.settle_page(
            PickerPageOutcome::Failed {
                request: second,
                message: "Search failed".into(),
            },
            window,
            cx,
        )
    });
    let after = mounted.diagnostics(cx);
    assert!(after.collection_failed);
    assert_eq!(after.total_count, before.total_count);
    assert_eq!(after.focused_key, before.focused_key);
    assert_eq!(after.scroll_offset, before.scroll_offset);
    assert_eq!(after.resident_row_count, before.resident_row_count);
    assert_eq!(after.pending_page_count, 0);
}

#[gpui::test]
fn mounted_pointer_and_keyboard_callbacks_use_exact_row_and_tab_cancels_end_intent(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx, 1000);
    mounted.settle(mounted.request(), 1000, cx);
    mounted.draw(cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let bounds = visual
        .debug_bounds("thread-root-picker-row-row-3")
        .expect("realized exact row");
    visual.simulate_click(bounds.center(), gpui::Modifiers::none());
    assert_eq!(
        mounted.diagnostics(cx).focused_key,
        Some(PickerRowKey("row-3".into()))
    );
    assert!(
        mounted
            .events
            .borrow()
            .contains(&PickerEvent::Activate(PickerRowKey("row-3".into())))
    );
    mounted.update(cx, |picker, _, cx| picker.finish_activation(cx));
    mounted.events.borrow_mut().clear();
    visual.simulate_event(gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("down").unwrap(),
        is_held: false,
    });
    visual.simulate_event(gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
        is_held: false,
    });
    assert!(
        mounted
            .events
            .borrow()
            .contains(&PickerEvent::Activate(PickerRowKey("row-4".into())))
    );
    visual.simulate_event(gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("end").unwrap(),
        is_held: false,
    });
    let request = mounted.request();
    assert!(mounted.diagnostics(cx).pending_navigation.is_some());
    visual.simulate_event(gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("tab").unwrap(),
        is_held: false,
    });
    mounted.settle(request, 1000, cx);
    assert!(mounted.diagnostics(cx).pending_navigation.is_none());
    assert_eq!(
        mounted.diagnostics(cx).focused_key,
        Some(PickerRowKey("row-4".into()))
    );
}
