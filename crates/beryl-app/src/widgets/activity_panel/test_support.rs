pub use super::Diagnostics;
use super::*;
use gpui::{Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled};

fn input_gate(current: Rc<std::cell::Cell<u8>>) -> InputGate {
    Rc::new(move |action| match current.get() {
        0 => {
            action();
            Gate::Current
        }
        1 => Gate::Busy,
        _ => Gate::Retired,
    })
}

pub struct Host {
    panel: Entity<ActivityPanel>,
    gate: Rc<std::cell::Cell<u8>>,
    events: Rc<RefCell<Vec<(f32, bool)>>>,
    retries: Rc<std::cell::Cell<usize>>,
    safe: FocusHandle,
    _subscription: gpui::Subscription,
}

impl Host {
    pub fn new(
        query: Query,
        pages: Vec<Arc<Page>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let safe = cx.focus_handle();
        let panel = cx.new(|cx| ActivityPanel::new(safe.clone(), window, cx));
        let gate = Rc::new(std::cell::Cell::new(0));
        let current = gate.clone();
        panel.update(cx, |panel, cx| {
            panel
                .replace(query, pages, input_gate(current), window, cx)
                .unwrap()
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let recorded = events.clone();
        let retries = Rc::new(std::cell::Cell::new(0));
        let retry_count = retries.clone();
        let subscription = cx.subscribe(&panel, move |_, _, event, _| {
            if let Event::Height {
                value, committed, ..
            } = event
            {
                recorded.borrow_mut().push((*value, *committed));
            }
            if matches!(event, Event::Retry { .. }) {
                retry_count.set(retry_count.get() + 1);
            }
        });
        Self {
            panel,
            gate,
            events,
            retries,
            safe,
            _subscription: subscription,
        }
    }
    pub fn diagnostics(&self, app: &gpui::App) -> Diagnostics {
        self.panel.read(app).diagnostics()
    }

    pub fn feedback(&self, feedback: Option<Feedback>, window: &mut Window, app: &mut gpui::App) {
        self.panel
            .update(app, |panel, cx| panel.feedback(feedback, window, cx));
    }
    pub fn focus_retry(&self, window: &mut Window, app: &gpui::App) {
        self.panel.read(app).retry_focus.focus(window);
    }
    pub fn retry_count(&self) -> usize {
        self.retries.get()
    }
    pub fn focus(&self, handle: bool, window: &mut Window, app: &gpui::App) {
        let panel = self.panel.read(app);
        if handle {
            panel.handle_focus.focus(window);
        } else {
            panel.viewport_focus.focus(window);
        }
    }
    pub fn focused(&self, window: &Window, app: &gpui::App) -> (bool, bool, bool) {
        let panel = self.panel.read(app);
        (
            panel.handle_focus.is_focused(window),
            panel.viewport_focus.is_focused(window),
            self.safe.is_focused(window),
        )
    }
    pub fn gate(&self, busy: bool, retired: bool) {
        self.gate.set(if retired { 2 } else { u8::from(busy) });
    }
    pub fn replace(
        &self,
        query: Query,
        pages: Vec<Arc<Page>>,
        window: &mut Window,
        app: &mut gpui::App,
    ) {
        let current = self.gate.clone();
        self.panel.update(app, |panel, cx| {
            panel
                .replace(query, pages, input_gate(current), window, cx)
                .unwrap()
        });
    }
    pub fn inert(&self, window: &mut Window, app: &mut gpui::App) {
        self.panel
            .update(app, |panel, cx| panel.set_inert(window, cx));
    }
    pub fn heights(&self) -> Vec<(f32, bool)> {
        self.events.borrow().clone()
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .w_full()
            .h_full()
            .font_family(".SystemUIFont")
            .track_focus(&self.safe)
            .child(self.panel.clone())
    }
}
