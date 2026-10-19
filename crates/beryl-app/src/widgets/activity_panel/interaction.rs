use super::*;

impl ActivityPanel {
    pub(super) fn wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input(window, cx, |panel, window, cx| {
            panel.wheel_current(event, window, cx)
        });
    }
    pub(super) fn viewport_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input(window, cx, |panel, window, cx| {
            panel.viewport_key_current(event, window, cx)
        });
    }
    pub(super) fn handle_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input(window, cx, |panel, window, cx| {
            panel.handle_key_current(event, window, cx)
        });
    }
    pub(super) fn begin_resize(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input(window, cx, |panel, window, cx| {
            panel.begin_resize_current(event, window, cx)
        });
    }
    pub(super) fn resize_motion(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input(window, cx, |panel, window, cx| {
            panel.resize_motion_current(event, window, cx)
        });
    }
    pub(super) fn finish_resize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input(window, cx, |panel, window, cx| {
            panel.finish_resize_current(window, cx)
        }) == Gate::Busy
        {
            if let Some(resize) = self.resizing.as_mut() {
                resize.released = true;
            }
        }
    }
    pub(super) fn retry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input(window, cx, |panel, window, cx| {
            panel.retry_current(window, cx)
        });
    }
    pub(super) fn scroll_from_bar(
        &mut self,
        offset: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input(window, cx, |panel, window, cx| {
            panel.scroll_from_bar_current(offset, window, cx)
        });
    }
    pub(super) fn after_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.model.set_viewport(self.model.viewport_height());
        self.tooltip.clear();
        if let Some(state) = self.scroll_state.borrow_mut().as_mut() {
            state.scroll_offset.y = px(self.model.offset() as f32);
        }
        self.visibility
            .record_viewport_activity(self.owner, window, cx);
        cx.emit(Event::Viewport(Demand {
            query: self.model.query,
            range: self.model.realized_range(),
        }));
        self.request(cx);
        cx.notify();
    }

    fn scroll_from_bar_current(
        &mut self,
        offset: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !offset.is_finite() {
            return;
        }
        if offset
            >= (self.model.content_height() - f64::from(self.model.viewport_height())).max(0.)
                as f32
        {
            self.model.move_to_end(true);
        } else {
            let offset = f64::from(offset.max(0.));
            self.model.first = (offset / f64::from(model::ROW_HEIGHT)).floor() as u64;
            self.model.fraction = offset.rem_euclid(f64::from(model::ROW_HEIGHT)) as f32;
            self.model.top_attached = offset == 0.;
        }
        self.after_scroll(window, cx);
    }

    fn wheel_current(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(px(model::ROW_HEIGHT));
        if self.model.scroll_by(-f32::from(delta.y)) {
            self.after_scroll(window, cx);
            cx.stop_propagation();
        }
    }

    fn viewport_key_current(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.viewport_focus.is_focused(window) {
            return;
        }
        let moved = match event.keystroke.key.as_str() {
            "up" => self.model.scroll_by(-model::ROW_HEIGHT),
            "down" => self.model.scroll_by(model::ROW_HEIGHT),
            "pageup" => self.model.scroll_by(-self.model.viewport_height()),
            "pagedown" => self.model.scroll_by(self.model.viewport_height()),
            "home" => self.model.move_to_end(false),
            "end" => self.model.move_to_end(true),
            _ => false,
        };
        if moved {
            self.after_scroll(window, cx);
            cx.stop_propagation();
        }
    }

    fn handle_key_current(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.handle_focus.is_focused(window) {
            return;
        }
        self.handle_keyboard = true;
        let height = match event.keystroke.key.as_str() {
            "up" => self.height + self.resize_step,
            "down" => self.height - self.resize_step,
            "pageup" => self.height + self.resize_page,
            "pagedown" => self.height - self.resize_page,
            "home" => self.minimum,
            "end" => self.maximum,
            "escape" => {
                if let Some(resize) = self.resizing.take() {
                    self.height = resize.height.clamp(self.minimum, self.maximum);
                    self.set_viewport_height();
                    cx.emit(Event::ResizeCancelled {
                        query: self.model.query,
                        value: self.height,
                    });
                    cx.notify();
                }
                cx.stop_propagation();
                return;
            }
            _ => return,
        };
        self.resize_to(height, true, cx);
        cx.stop_propagation();
    }

    fn begin_resize_current(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_keyboard = false;
        self.handle_focus.focus(window);
        self.resizing = Some(Resize {
            y: f32::from(event.position.y),
            height: self.height,
            released: false,
        });
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }

    fn resize_motion_current(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.resizing.is_none() {
            return;
        }
        if self.resizing.as_ref().is_some_and(|resize| resize.released) {
            return;
        }
        if event.pressed_button != Some(gpui::MouseButton::Left) {
            let resize = self.resizing.take().unwrap();
            self.height = resize.height.clamp(self.minimum, self.maximum);
            self.set_viewport_height();
            cx.emit(Event::ResizeCancelled {
                query: self.model.query,
                value: self.height,
            });
            cx.notify();
            return;
        }
        let resize = self.resizing.as_ref().unwrap();
        let height = resize.height + resize.y - f32::from(event.position.y);
        self.resize_to(height, false, cx);
        cx.stop_propagation();
    }

    fn finish_resize_current(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.resizing.is_none() {
            return;
        }
        self.resizing = None;
        cx.emit(Event::Height {
            query: self.model.query,
            value: self.height,
            committed: true,
        });
        cx.notify();
    }

    fn resize_to(&mut self, height: f32, committed: bool, cx: &mut Context<Self>) {
        if !height.is_finite() {
            return;
        }
        self.height = height.clamp(self.minimum, self.maximum);
        self.set_viewport_height();
        self.request(cx);
        cx.emit(Event::Height {
            query: self.model.query,
            value: self.height,
            committed,
        });
        cx.notify();
    }

    fn retry_current(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(feedback) = self
            .model
            .feedback
            .as_mut()
            .filter(|feedback| feedback.retry_available && !feedback.pending)
        else {
            return;
        };
        feedback.pending = true;
        feedback.retry_available = false;
        cx.emit(Event::Retry {
            query: self.model.query,
            feedback: feedback.identity,
        });
        cx.notify();
    }
}
