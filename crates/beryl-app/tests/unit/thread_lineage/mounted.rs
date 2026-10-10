use super::*;
use gpui::{
    Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled, Subscription,
    TestAppContext, WindowHandle, div,
};

struct Root {
    widget: Entity<ThreadLineage>,
    maximum_width: f32,
    other: FocusHandle,
    _events: Subscription,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .max_w(px(self.maximum_width))
            .flex()
            .flex_col()
            .child(self.widget.clone())
            .child(
                div()
                    .id("lineage-test-other")
                    .track_focus(&self.other)
                    .h(px(32.))
                    .child("Other"),
            )
            .child(gpui::canvas(
                |_, window, _| {
                    let text = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789- .›";
                    for (font_size, weight) in [(10., 700.), (12., 400.), (13., 500.), (13., 600.)]
                    {
                        let mut font = gpui::font("Inter");
                        font.weight = gpui::FontWeight(weight);
                        let line = window.text_system().shape_line(
                            text.into(),
                            px(font_size),
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
                                point(
                                    gpui::DevicePixels(0),
                                    gpui::DevicePixels(-(font_size as i32)),
                                ),
                                size(
                                    gpui::DevicePixels(font_size as i32),
                                    gpui::DevicePixels(font_size as i32),
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
    widget: Entity<ThreadLineage>,
    events: Rc<RefCell<Vec<LineageEvent>>>,
}

impl Mounted {
    fn new(cx: &mut TestAppContext, count: u64) -> Self {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        let (window, widget) = cx.update(|app| {
            let mut widget = None;
            let window = app
                .open_window(gpui::WindowOptions::default(), |window, app| {
                    let view = app.new(|cx| {
                        ThreadLineage::new(query(1, count), "Current".into(), window, cx)
                    });
                    widget = Some(view.clone());
                    app.new(|cx| Root {
                        maximum_width: 100_000.,
                        other: cx.focus_handle(),
                        _events: cx.subscribe(&view, move |_, _, event, _| {
                            sink.borrow_mut().push(event.clone())
                        }),
                        widget: view,
                    })
                })
                .unwrap();
            (window, widget.unwrap())
        });
        let mounted = Self {
            window,
            widget,
            events,
        };
        mounted.draw(cx);
        mounted
    }

    fn update(
        &self,
        cx: &mut TestAppContext,
        f: impl FnOnce(&mut ThreadLineage, &mut Window, &mut Context<ThreadLineage>),
    ) {
        self.window
            .update(cx, |_, window, app| {
                self.widget.update(app, |widget, cx| f(widget, window, cx))
            })
            .unwrap();
        cx.run_until_parked();
    }

    fn draw(&self, cx: &mut TestAppContext) {
        cx.run_until_parked();
        cx.update(|app| {
            app.update_window(self.window.into(), |_, window, app| {
                window.draw(app).clear()
            })
            .unwrap()
        });
        cx.run_until_parked();
    }

    fn fill(&self, cx: &mut TestAppContext) {
        for _ in 0..20 {
            let requests = self
                .widget
                .read_with(cx, |widget, _| widget.model.requests.clone());
            if requests.is_empty() {
                self.draw(cx);
                return;
            }
            for request in requests {
                let response = page(request, (request.end - request.start) as usize);
                self.update(cx, |widget, window, cx| {
                    widget.settle_page(request, Some(response), false, window, cx)
                });
            }
            self.draw(cx);
        }
        panic!("visible pages did not settle");
    }

    fn key(&self, cx: &mut TestAppContext, key: &str) {
        let mut visual = gpui::VisualTestContext::from_window(self.window.into(), cx);
        visual.simulate_event(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse(key).unwrap(),
            is_held: false,
        });
        cx.run_until_parked();
        self.draw(cx);
    }

    fn wheel(&self, cx: &mut TestAppContext, x: f32, y: f32, shift: bool) {
        let position = self
            .widget
            .read_with(cx, |widget, _| widget.bounds.center());
        let mut visual = gpui::VisualTestContext::from_window(self.window.into(), cx);
        visual.simulate_event(gpui::ScrollWheelEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(x), px(y))),
            modifiers: gpui::Modifiers {
                shift,
                ..Default::default()
            },
            touch_phase: gpui::TouchPhase::Moved,
        });
        cx.run_until_parked();
        self.draw(cx);
    }

    fn click_ordinal(&self, cx: &mut TestAppContext, ordinal: u64) {
        let position = self.widget.read_with(cx, |widget, _| {
            assert!(widget.ranges().0.contains(&ordinal));
            assert_eq!(
                widget.model.row(ordinal).unwrap().thread,
                id(ordinal as u128 + 1)
            );
            point(
                widget.bounds.origin.x
                    + px((ordinal - widget.first_ordinal) as f32 * widget.stride()
                        - widget.fraction
                        + widget.breadcrumb_width() / 2.),
                widget.bounds.center().y,
            )
        });
        let mut visual = gpui::VisualTestContext::from_window(self.window.into(), cx);
        visual.simulate_event(gpui::MouseDownEvent {
            position,
            button: gpui::MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        cx.run_until_parked();
        let mut visual = gpui::VisualTestContext::from_window(self.window.into(), cx);
        visual.simulate_event(gpui::MouseUpEvent {
            position,
            button: gpui::MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        cx.run_until_parked();
        self.draw(cx);
    }

    fn activations(&self) -> Vec<beryl_model::SyndicThreadId> {
        self.events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                LineageEvent::Activate { thread, .. } => Some(*thread),
                _ => None,
            })
            .collect()
    }
}

#[path = "mounted/windowing.rs"]
mod windowing;
