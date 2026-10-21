use super::*;
use crate::syndic_transcript::{
    ImageMarkerInspectionRequested, ResidentContextMenuContentKind, SyndicTranscriptPanel,
};
use gpui::{
    AnyElement, FocusHandle, IntoElement as _, KeyDownEvent, MouseButton,
    StatefulInteractiveElement as _,
};

pub(super) struct TranscriptImagePreview {
    source: Entity<SyndicTranscriptPanel>,
    origin: ImageMarkerInspectionRequested,
    close: FocusHandle,
    _focus_guard: gpui::Subscription,
    inspection: crate::main_window::image_inspection::ImageInspection,
    commands: crate::main_window::image_inspection::original_commands::PreviewImageCommands,
}

pub(super) fn subscribe(
    transcript: &Entity<SyndicTranscriptPanel>,
    window: &mut Window,
    cx: &mut Context<MainWindowShellRoot>,
) -> gpui::Subscription {
    cx.subscribe_in(
        transcript,
        window,
        |root, source, event: &ImageMarkerInspectionRequested, window, cx| {
            if root.transcript_image_preview.is_some()
                || root.running_threads.views_retired
                || root.running_threads.reads_suspended
                || root.running_selection_interaction_gated(cx)
                || root.ordinary_close_interaction_gated
                || root.shutdown_interaction_gated
                || root.startup_interaction_gated()
                || source != &root.running_threads.transcript
                || !source
                    .read(cx)
                    .image_marker_inspection_current(event, window)
            {
                source.update(cx, |panel, _| {
                    panel.finish_image_marker_inspection(event.request)
                });
                return;
            }
            let close = cx.focus_handle().tab_stop(true);
            let guard = cx.on_blur(&close, window, |root, window, _| {
                if let Some(preview) = &root.transcript_image_preview {
                    if !preview.commands.focus_delegated(window) {
                        window.focus(&preview.close);
                    }
                }
            });
            window.focus(&close);
            let demand = transcript_image_demand(event);
            let inspection = crate::main_window::image_inspection::ImageInspection::new(
                root.image_media_window.as_deref(),
                demand,
            );
            let commands =
                crate::main_window::image_inspection::original_commands::PreviewImageCommands::new(
                    root.image_media_window
                        .as_ref()
                        .map(Arc::downgrade)
                        .unwrap_or_default(),
                    demand,
                    close.clone(),
                    window,
                    cx,
                );
            root.transcript_image_preview = Some(TranscriptImagePreview {
                source: source.clone(),
                origin: event.clone(),
                close,
                _focus_guard: guard,
                inspection,
                commands,
            });
            cx.notify();
        },
    )
}

impl MainWindowShellRoot {
    fn refresh_transcript_generated_image(&mut self, window: &Window, cx: &Context<Self>) {
        let Some(preview) = self.transcript_image_preview.as_mut() else {
            return;
        };
        if preview.source != self.running_threads.transcript {
            return;
        }
        if let Some(origin) = preview
            .source
            .read(cx)
            .refreshed_image_marker_inspection(&preview.origin, window)
        {
            preview.origin = origin;
        }
        if let ResidentContextMenuContentKind::GeneratedImage(image) =
            preview.origin.target.record.content_kind
        {
            let demand = transcript_image_demand(&preview.origin);
            preview
                .inspection
                .set_generated(image, demand, self.image_media_window.as_deref());
            preview.commands.original.set_generated(image, demand);
        }
    }
    pub(super) fn sync_image_media_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "windows")]
        let attachment = crate::running_owner::RunningProcessOwner::mounted_owner(cx)
            .and_then(|owner| owner.upgrade())
            .and_then(|owner| owner.borrow().media_attachment());
        #[cfg(not(target_os = "windows"))]
        let attachment: Option<Arc<crate::image_processing::ImageMediaAttachment>> = None;
        let identity = attachment
            .as_ref()
            .map(|attachment| attachment.id())
            .or(self.image_media_attachment);
        if self
            .image_media_window
            .as_ref()
            .is_some_and(|media| media.is_closed())
        {
            self.image_media_window = None;
            self.dismiss_transcript_image_preview(window, cx);
        }
        if self.image_media_attachment != identity {
            self.image_media_retry = None;
            self.dismiss_transcript_image_preview(window, cx);
            if let Some(media) = self.image_media_window.take() {
                media.close();
            }
            self.image_media_attachment = identity;
            if let Some(attachment) = attachment.filter(|attachment| !attachment.is_retired()) {
                match attachment.new_window(window) {
                    Ok(media) => self.image_media_window = Some(Arc::new(media)),
                    Err(crate::image_processing::ImageMediaError(
                        crate::image_processing::ImageMediaFailure::Native(
                            gpui::ImageResourceError::Pending,
                        ),
                    )) => {
                        self.image_media_retry = Some(cx.spawn(async move |root, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(16))
                                .await;
                            let _ = root.update(cx, |root, cx| {
                                root.image_media_attachment = None;
                                root.image_media_retry = None;
                                cx.notify();
                            });
                        }));
                    }
                    Err(_) => {}
                }
            }
        }
        self.running_threads.transcript.update(cx, |panel, _| {
            panel.set_image_original_media_window(self.image_media_window.as_ref())
        });
        let composer = self
            .controller
            .as_ref()
            .and_then(|controller| controller.composer_mount())
            .or_else(|| self.runtime_setup.mount.clone())
            .and_then(|mount| mount.read(cx).contribution());
        if let Some(composer) = composer {
            composer.update(cx, |composer, _| {
                composer.set_image_media_window(self.image_media_window.as_ref())
            });
        }
    }

    pub(crate) fn close_image_media_window(&mut self) {
        self.image_media_retry = None;
        self.transcript_image_preview = None;
        if let Some(media) = self.image_media_window.take() {
            media.close();
        }
    }

    #[cfg(test)]
    pub(crate) fn test_image_original_command(
        &self,
        save: bool,
    ) -> Option<(Option<&'static str>, &'static str)> {
        let commands = &self.transcript_image_preview.as_ref()?.commands.original;
        Some((
            commands.reason(if save {
                crate::image_processing::OriginalImageRequest::Save
            } else {
                crate::image_processing::OriginalImageRequest::Copy
            }),
            commands.feedback,
        ))
    }

    #[cfg(test)]
    pub(crate) fn test_image_original_anchor_focused(&self, window: &Window) -> bool {
        self.transcript_image_preview
            .as_ref()
            .is_some_and(|preview| preview.commands.anchor_focus.is_focused(window))
    }

    #[cfg(test)]
    pub(crate) fn test_image_marker_pixel(
        &self,
    ) -> Option<Result<[u8; 4], crate::image_processing::ImageMediaError>> {
        self.transcript_image_preview
            .as_ref()?
            .inspection
            .read_pixel_for_test()
    }

    #[cfg(test)]
    pub(crate) fn test_image_marker_rendition(&self) -> Option<(bool, bool, bool, bool)> {
        self.transcript_image_preview
            .as_ref()
            .map(|preview| preview.inspection.diagnostics())
    }

    #[cfg(test)]
    pub(crate) fn test_image_marker_inspection(&self) -> Option<ImageMarkerInspectionRequested> {
        self.transcript_image_preview
            .as_ref()
            .map(|preview| preview.origin.clone())
    }

    fn dismiss_transcript_image_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preview) = self.transcript_image_preview.take() else {
            return;
        };
        let current = preview.source == self.running_threads.transcript
            && !self.running_threads.views_retired
            && !self.running_threads.reads_suspended
            && preview
                .source
                .read(cx)
                .image_marker_inspection_current(&preview.origin, window);
        preview.source.update(cx, |panel, _| {
            panel.finish_image_marker_inspection(preview.origin.request)
        });
        if current {
            window.focus(&preview.origin.focus);
        } else {
            let fallback = (preview.source == self.running_threads.transcript
                && !self.running_threads.views_retired
                && !self.running_threads.reads_suspended)
                .then(|| preview.source.read(cx).image_marker_return_focus(window))
                .flatten()
                .unwrap_or_else(|| self.thread_switcher.focus.clone());
            window.focus(&fallback);
        }
        cx.notify();
    }

    pub(super) fn render_transcript_image_preview(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let fallback = self.thread_switcher.focus.clone();
        self.running_threads.transcript.update(cx, |panel, _| {
            panel.set_image_marker_focus_fallback(fallback)
        });
        self.refresh_transcript_generated_image(window, cx);
        if self
            .transcript_image_preview
            .as_ref()
            .is_some_and(|preview| {
                preview.source != self.running_threads.transcript
                    || self.running_threads.views_retired
                    || self.running_threads.reads_suspended
                    || !preview
                        .source
                        .read(cx)
                        .image_marker_inspection_current(&preview.origin, window)
            })
        {
            self.dismiss_transcript_image_preview(window, cx);
        }
        let colors = super::model_controls::menu_colors(self);
        let preview = self.transcript_image_preview.as_mut()?;
        preview
            .inspection
            .prepare(self.image_media_window.as_deref(), window, cx);
        let presentation = preview.inspection.element();
        preview.commands.poll(window, cx);
        let menu = preview.commands.render(
            colors,
            "transcript-image-original-menu",
            transcript_original_menu_event,
            window,
            cx,
        );
        let anchor_bounds = preview.commands.anchor_bounds.clone();
        let anchor_focus = preview.commands.anchor_focus.clone();
        let feedback = preview.commands.original.feedback;
        let label = preview.origin.target.record.content_kind.image_label()?;
        let close = preview.close.clone();
        let viewport = window.viewport_size();
        Some(
            div()
                .id("transcript-image-preview")
                .debug_selector(|| "transcript-image-preview".to_owned())
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .p(px(16.0))
                .bg(gpui::rgba(0x00000088))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|root, _, window, cx| {
                        cx.stop_propagation();
                        root.dismiss_transcript_image_preview(window, cx);
                    }),
                )
                .on_key_down(cx.listener(move |root, event: &KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "escape" => {
                            cx.stop_propagation();
                            root.dismiss_transcript_image_preview(window, cx);
                        }
                        "tab" => {
                            cx.stop_propagation();
                            if let Some(preview) = root.transcript_image_preview.as_mut() {
                                preview.commands.key("tab", window);
                            }
                        }
                        _ => {}
                    }
                }))
                .child(
                    div()
                        .id("transcript-image-preview-frame")
                        .occlude()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .w(px(420.0).min((viewport.width - px(32.0)).max(px(1.0))))
                        .h(px(240.0).min((viewport.height - px(32.0)).max(px(1.0))))
                        .p(px(16.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.border)
                        .bg(colors.background)
                        .text_color(colors.foreground)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _, _, cx| cx.stop_propagation()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(label)
                                .child(
                                    div()
                                        .on_children_prepainted(move |children, _, _| {
                                            anchor_bounds.set(children.first().copied())
                                        })
                                        .child(
                                            div()
                                                .id("transcript-image-preview-actions")
                                                .debug_selector(|| {
                                                    "transcript-image-preview-actions".to_owned()
                                                })
                                                .track_focus(&anchor_focus)
                                                .focusable()
                                                .tab_stop(true)
                                                .px(px(8.0))
                                                .py(px(4.0))
                                                .cursor_pointer()
                                                .on_click(cx.listener(|root, _, window, cx| {
                                                    if let Some(preview) =
                                                        root.transcript_image_preview.as_mut()
                                                    {
                                                        preview.commands.open(window, cx);
                                                    }
                                                }))
                                                .on_key_down(cx.listener(
                                                    |root, event: &KeyDownEvent, window, cx| {
                                                        if matches!(
                                                            event.keystroke.key.as_str(),
                                                            "enter" | "space" | " "
                                                        ) {
                                                            cx.stop_propagation();
                                                            if let Some(preview) = root
                                                                .transcript_image_preview
                                                                .as_mut()
                                                            {
                                                                preview.commands.open(window, cx);
                                                            }
                                                        }
                                                    },
                                                ))
                                                .child("Image actions"),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("transcript-image-preview-close")
                                        .debug_selector(|| {
                                            "transcript-image-preview-close".to_owned()
                                        })
                                        .track_focus(&close)
                                        .focusable()
                                        .px(px(8.0))
                                        .py(px(4.0))
                                        .rounded(px(4.0))
                                        .cursor_pointer()
                                        .hover(|style| style.bg(colors.hover))
                                        .focus(|style| style.bg(colors.focused))
                                        .on_click(cx.listener(|root, _, window, cx| {
                                            cx.stop_propagation();
                                            root.dismiss_transcript_image_preview(window, cx);
                                        }))
                                        .on_key_down(cx.listener(
                                            |root, event: &KeyDownEvent, window, cx| {
                                                if matches!(
                                                    event.keystroke.key.as_str(),
                                                    "enter" | "space" | " "
                                                ) {
                                                    cx.stop_propagation();
                                                    root.dismiss_transcript_image_preview(
                                                        window, cx,
                                                    );
                                                }
                                            },
                                        ))
                                        .child("Close"),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(colors.disabled)
                                .child(presentation),
                        )
                        .children((!feedback.is_empty()).then(|| div().text_xs().child(feedback))),
                )
                .children(menu)
                .into_any_element(),
        )
    }
}

fn transcript_original_menu_event(
    root: &mut MainWindowShellRoot,
    event: crate::widgets::anchored_context_menu::MenuEvent,
    window: &mut Window,
    cx: &mut Context<MainWindowShellRoot>,
) {
    root.refresh_transcript_generated_image(window, cx);
    let current = root
        .transcript_image_preview
        .as_ref()
        .is_some_and(|preview| {
            preview.source == root.running_threads.transcript
                && !root.running_threads.views_retired
                && !root.running_threads.reads_suspended
                && preview
                    .source
                    .read(cx)
                    .image_marker_inspection_current(&preview.origin, window)
        });
    if !current {
        root.dismiss_transcript_image_preview(window, cx);
        return;
    }
    if let Some(preview) = root.transcript_image_preview.as_mut() {
        preview.commands.event(event, window, cx);
    }
}

fn transcript_image_demand(
    event: &ImageMarkerInspectionRequested,
) -> Option<crate::image_processing::ImageSourceDemand> {
    event.target.record.image_demand()
}
