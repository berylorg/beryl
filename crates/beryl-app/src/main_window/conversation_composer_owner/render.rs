use gpui::prelude::*;
use gpui::{
    Context, Corner, IntoElement, KeyDownEvent, Pixels, Render, Window, anchored, deferred, div,
    point, px, rgb,
};

use super::MainWindowConversationComposer;
use crate::main_window::ComposerImagePresentationState;

const MARKER_MENU_WIDTH: Pixels = px(144.0);
const MARKER_MENU_HEIGHT: Pixels = px(68.0);
const PREVIEW_WIDTH: Pixels = px(360.0);
const PREVIEW_HEIGHT: Pixels = px(224.0);

impl MainWindowConversationComposer {
    pub(in crate::main_window) fn protected_resident_layout(
        &self,
        cx: &gpui::App,
    ) -> Option<gpui_text_input::RangeResidentLayoutSnapshot> {
        (matches!(
            self.phase,
            super::MainWindowConversationComposerPhase::RecoveryFenced
        ) && (self
            .failed_resident
            .as_ref()
            .is_some_and(|fence| fence.capture_taken)
            || self.failed_claim_protection.is_some_and(|protection| {
                let input = self.input.read(cx);
                input.resident_protection_is_current(protection)
                    && input.surface().is_some_and(|surface| {
                        surface.binding() == self.selection.binding().range_binding()
                    })
            })))
        .then(|| self.input.read(cx).resident_layout_snapshot())
    }

    fn record_surface_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.last_error = Some(error);
        cx.notify();
    }

    fn render_marker_menu(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let anchor = self.image_surfaces.menu().unwrap().anchor();
        let viewport = window.viewport_size();
        let position = point(
            clamp_axis(anchor.bounds.origin.x, viewport.width - MARKER_MENU_WIDTH),
            clamp_axis(anchor.bounds.bottom(), viewport.height - MARKER_MENU_HEIGHT),
        );

        deferred(
            anchored().position(position).anchor(Corner::TopLeft).child(
                div()
                    .id("conversation-composer-marker-menu")
                    .debug_selector(|| "conversation-composer-marker-menu".to_owned())
                    .track_focus(&self.image_surface_focus)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key == "escape" {
                            cx.stop_propagation();
                            if let Err(error) = this.dismiss_marker_menu(window, cx) {
                                this.record_surface_error(error, cx);
                            }
                        }
                    }))
                    .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                        if let Err(error) = this.dismiss_marker_menu(window, cx) {
                            this.record_surface_error(error, cx);
                        }
                    }))
                    .w(MARKER_MENU_WIDTH)
                    .h(MARKER_MENU_HEIGHT)
                    .p_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .occlude()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x5f6875))
                    .bg(rgb(0x20242b))
                    .shadow_lg()
                    .text_sm()
                    .text_color(rgb(0xf2f4f7))
                    .child(
                        div()
                            .id("conversation-composer-marker-view")
                            .debug_selector(|| "conversation-composer-marker-view".to_owned())
                            .h(px(28.0))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x333a45)))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                if let Err(error) = this.invoke_marker_view(
                                    ComposerImagePresentationState::Pending,
                                    window,
                                    cx,
                                ) {
                                    this.record_surface_error(error, cx);
                                }
                            }))
                            .child("View"),
                    )
                    .child(
                        div()
                            .id("conversation-composer-marker-remove")
                            .debug_selector(|| "conversation-composer-marker-remove".to_owned())
                            .h(px(28.0))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_color(rgb(0xff8f8f))
                            .hover(|style| style.bg(rgb(0x3c2b30)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                if let Err(error) = this.invoke_marker_remove(cx) {
                                    this.record_surface_error(error, cx);
                                }
                            }))
                            .child("Remove"),
                    ),
            ),
        )
        .with_priority(2)
    }

    fn render_image_preview(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let preview = self.image_surfaces.preview().unwrap();
        let (heading, detail) = match preview.state() {
            ComposerImagePresentationState::Pending => (
                "Image preview pending",
                "The local image presentation is still being prepared.",
            ),
            ComposerImagePresentationState::LocalUnavailable => (
                "Image preview unavailable",
                "This image is not available from local storage.",
            ),
        };
        let viewport = window.viewport_size();
        let position = point(
            clamp_axis(
                (viewport.width - PREVIEW_WIDTH) * 0.5,
                viewport.width - PREVIEW_WIDTH,
            ),
            clamp_axis(
                (viewport.height - PREVIEW_HEIGHT) * 0.5,
                viewport.height - PREVIEW_HEIGHT,
            ),
        );

        deferred(
            anchored().position(position).anchor(Corner::TopLeft).child(
                div()
                    .id("conversation-composer-image-preview")
                    .debug_selector(|| "conversation-composer-image-preview".to_owned())
                    .track_focus(&self.image_surface_focus)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key == "escape" {
                            cx.stop_propagation();
                            if let Err(error) = this.dismiss_image_preview(window, cx) {
                                this.record_surface_error(error, cx);
                            }
                        }
                    }))
                    .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                        if let Err(error) = this.dismiss_image_preview(window, cx) {
                            this.record_surface_error(error, cx);
                        }
                    }))
                    .w(PREVIEW_WIDTH)
                    .h(PREVIEW_HEIGHT)
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .occlude()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(0x5f6875))
                    .bg(rgb(0x20242b))
                    .shadow_lg()
                    .text_color(rgb(0xf2f4f7))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().child("Image preview"))
                            .child(
                                div()
                                    .id("conversation-composer-image-preview-close")
                                    .debug_selector(|| {
                                        "conversation-composer-image-preview-close".to_owned()
                                    })
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(0x333a45)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        cx.stop_propagation();
                                        if let Err(error) = this.dismiss_image_preview(window, cx) {
                                            this.record_surface_error(error, cx);
                                        }
                                    }))
                                    .child("Close"),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .rounded_md()
                            .bg(rgb(0x181b20))
                            .child(div().text_sm().child(heading))
                            .child(div().text_xs().opacity(0.72).child(detail)),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(disabled_preview_command(
                                "conversation-composer-image-preview-copy",
                                "Copy",
                            ))
                            .child(disabled_preview_command(
                                "conversation-composer-image-preview-save",
                                "Save",
                            )),
                    ),
            ),
        )
        .with_priority(3)
    }
}

impl Render for MainWindowConversationComposer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "test-faults")]
        self.submission_outcome_rendered.set(None);
        self.schedule_pump(window, cx);
        let protected_layout = self.protected_resident_layout(cx);
        let selected = !self.is_pending_target();
        let pending_realizer = self.pending_realizer.as_ref().and_then(|pending| {
            pending.lifetime.upgrade()?;
            pending
                .composer
                .upgrade()
                .filter(|composer| composer.read(cx).matches_pending_target(pending.receipt))
        });
        div()
            .id(("conversation-composer-root", cx.entity_id()))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.submission_outcome.is_some() && event.keystroke.key != "enter" {
                    this.submission_outcome = None;
                    cx.notify();
                }
                if event.keystroke.key == "escape" && this.paste_pending() {
                    this.cancel_captured_paste(cx);
                    cx.stop_propagation();
                }
            }))
            .relative()
            .when_some(
                self.submission_unavailable().or(self.submission_outcome),
                |root, reason| {
                    root.tooltip(move |_, cx| cx.new(|_| SubmissionTooltip(reason)).into())
                },
            )
            .when(self.paste_pending(), |root| root.opacity(0.55))
            .size_full()
            .when_some(protected_layout, |root, layout| {
                root.w(layout.layout.wrap_width)
                    .h(layout.viewport_extent)
                    .flex_none()
            })
            .when(selected, |root| {
                root.debug_selector(|| "conversation-composer-root".to_owned())
            })
            .children(pending_realizer.map(|pending| {
                div().absolute().size_full().overflow_hidden().child(
                    div()
                        .debug_selector(|| "conversation-composer-pending-realization".to_owned())
                        .absolute()
                        .size_full()
                        .opacity(0.)
                        .child(pending),
                )
            }))
            .child(self.input.clone())
            .children(self.submission_outcome.map(|reason| {
                let outcome = div();
                #[cfg(feature = "test-faults")]
                let outcome = {
                    let rendered = self.submission_outcome_rendered.clone();
                    outcome.on_children_prepainted(move |bounds, _, _| {
                        rendered.set(bounds.first().copied().map(|bounds| (reason, bounds)))
                    })
                };
                outcome
                    .id("conversation-composer-submission-outcome")
                    .debug_selector(|| "conversation-composer-submission-outcome".to_owned())
                    .absolute()
                    .right(px(4.))
                    .bottom(px(4.))
                    .max_w_full()
                    .child(submission_tooltip(reason))
            }))
            .children(
                self.image_surfaces
                    .menu()
                    .map(|_| self.render_marker_menu(window, cx)),
            )
            .children(
                self.image_surfaces
                    .preview()
                    .map(|_| self.render_image_preview(window, cx)),
            )
    }
}

struct SubmissionTooltip(crate::cas_projection::SubmissionUnavailable);

impl Render for SubmissionTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        submission_tooltip(self.0)
    }
}

fn submission_tooltip(reason: crate::cas_projection::SubmissionUnavailable) -> impl IntoElement {
    div()
        .max_w(px(320.))
        .max_h(px(160.))
        .px(px(8.))
        .py(px(6.))
        .rounded(px(5.))
        .bg(rgb(0x111827))
        .text_color(rgb(0xffffff))
        .text_size(px(12.))
        .child(reason.reason())
}

fn disabled_preview_command(selector: &'static str, label: &'static str) -> impl IntoElement {
    div()
        .id(selector)
        .debug_selector(move || selector.to_owned())
        .px_3()
        .py_1()
        .rounded_sm()
        .border_1()
        .border_color(rgb(0x5f6875))
        .opacity(0.48)
        .child(label)
}

fn clamp_axis(value: Pixels, maximum: Pixels) -> Pixels {
    value.max(Pixels::ZERO).min(maximum.max(Pixels::ZERO))
}
