use super::*;

impl MainWindowShellRoot {
    pub(super) fn activity_event(
        &mut self,
        widget: &Entity<ActivityPanel>,
        event: Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activity_event_current(widget, event, window, cx);
    }

    fn activity_event_current(
        &mut self,
        widget: &Entity<ActivityPanel>,
        event: Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.activity.widget.as_ref() != Some(widget)
            || !self.activity.visible
            || self.shutdown_interaction_gated
            || self.ordinary_close_interaction_gated
            || self.startup_interaction_gated()
        {
            return;
        }
        match event {
            Event::Demand(request) if self.activity.retry.is_some() => {
                let _ = request;
            }
            Event::Demand(request) | Event::Viewport(request) => {
                if self.activity.query != Some(request.query) {
                    return;
                }
                if let Some(demand) = self
                    .activity
                    .demands
                    .as_ref()
                    .and_then(|owner| owner.request(request.range))
                {
                    self.activity.demand = Some(demand);
                    self.activity.retry_command = None;
                }
            }
            Event::Retry { query, feedback } => {
                if self.activity.query == Some(query) && feedback == self.activity.feedback_key {
                    self.activity.retry_command = self.activity.retry.clone();
                }
            }
            Event::Height {
                query,
                value,
                committed,
            } => {
                if self.activity.query != Some(query) {
                    return;
                }
                if !value.is_finite() || value <= 0. {
                    return;
                }
                if !committed && !self.activity.resizing {
                    self.activity.resize_origin = Some(self.activity.preferred_height);
                }
                if committed {
                    self.activity.resize_origin = None;
                }
                self.activity.preferred_height = value;
                self.activity.resizing = !committed;
                if committed
                    && !self
                        .activity
                        .height_result
                        .as_ref()
                        .is_some_and(|flight| flight.blocks_replacement())
                {
                    if let Some(authority) = self.activity.authority.clone() {
                        self.activity.height_command = Some(HeightCommand { value, authority });
                    }
                }
                cx.notify();
            }
            Event::ResizeCancelled { query, value } => {
                if self.activity.query != Some(query) {
                    return;
                }
                if value.is_finite() && value > 0. {
                    self.activity.preferred_height =
                        self.activity.resize_origin.take().unwrap_or(value);
                }
                self.activity.resizing = false;
                cx.notify();
            }
        }
        let _ = window;
    }
}
