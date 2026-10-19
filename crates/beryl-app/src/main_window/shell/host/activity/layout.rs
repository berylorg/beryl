use super::*;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn activity_allocation(
        &self,
        metrics: &sizing::ShellMetrics,
        composer: gpui::Pixels,
        viewport: gpui::Pixels,
    ) -> gpui::Pixels {
        if !metrics.activity || !self.activity.visible {
            return px(0.);
        }
        let maximum = f32::from((viewport - metrics.pinned_height() - composer).max(px(0.)));
        if maximum < model::MIN_HEIGHT {
            return px(0.);
        }
        px(self
            .activity
            .preferred_height
            .clamp(model::MIN_HEIGHT, maximum))
    }

    pub(in crate::main_window::shell::host) fn render_activity(
        &mut self,
        composer: gpui::Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<ActivityPanel>> {
        let metrics = self.controller.as_ref()?.metrics.clone();
        if !metrics.activity || !self.activity.visible {
            return None;
        }
        let maximum = f32::from(
            (window.viewport_size().height - metrics.pinned_height() - composer).max(px(0.)),
        );
        if maximum < model::MIN_HEIGHT {
            return None;
        }
        let height = self.activity_allocation(&metrics, composer, window.viewport_size().height);
        let widget = self.activity.widget.clone()?;
        widget.update(cx, |widget, cx| {
            widget.allocation(f32::from(height), maximum, cx)
        });
        Some(widget)
    }
}
