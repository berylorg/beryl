use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Anchor {
    Handle,
    Row(beryl_model::SyndicItemId),
}

#[derive(Default)]
pub(super) struct State {
    pub(super) anchor: Option<Anchor>,
    pub(super) ready: bool,
    pending: Option<gpui::Task<()>>,
}

impl State {
    pub(super) fn clear(&mut self) {
        self.anchor = None;
        self.ready = false;
        self.pending = None;
    }
}

impl ActivityPanel {
    pub(super) fn tooltip_hover(
        &mut self,
        anchor: Anchor,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !hovered {
            if self.tooltip.anchor == Some(anchor) {
                self.tooltip.clear();
                cx.notify();
            }
            return;
        }
        if !self.current(window, cx) || self.tooltip.anchor == Some(anchor) {
            return;
        }
        self.tooltip.clear();
        self.tooltip.anchor = Some(anchor);
        self.tooltip.pending = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
            let _ = this.update_in(cx, |panel, _, cx| {
                if panel.tooltip.anchor == Some(anchor) && !panel.model.inert {
                    panel.tooltip.ready = true;
                    panel.tooltip.pending = None;
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    pub(super) fn tooltip_current(
        &self,
        anchor: Anchor,
        bounds: gpui::Bounds<gpui::Pixels>,
        window: &Window,
    ) -> bool {
        if self.model.inert
            || !self.tooltip.ready
            || self.tooltip.anchor != Some(anchor)
            || !bounds.contains(&window.mouse_position())
            || !self
                .gate
                .as_ref()
                .is_some_and(|gate| probe(gate) == Gate::Current)
        {
            return false;
        }
        match anchor {
            Anchor::Handle => true,
            Anchor::Row(id) => {
                self.bounds.contains(&window.mouse_position())
                    && self.model.pages.iter().any(|page| {
                        page.rows.iter().enumerate().any(|(index, row)| {
                            row.id == id
                                && self
                                    .model
                                    .realized_range()
                                    .contains(&(page.start + index as u64))
                        })
                    })
            }
        }
    }
}
