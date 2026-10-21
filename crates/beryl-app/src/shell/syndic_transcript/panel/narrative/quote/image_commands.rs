use super::*;
use crate::{
    image_processing::{ImageMediaWindow, ImageSourceDemand, OriginalImageRequest},
    main_window::image_inspection::original_commands::OriginalImageCommands,
};

pub(super) struct DirectImageCommands {
    target: crate::syndic_transcript::ResidentTranscriptContextMenuTarget,
    activation: u64,
    serial: u64,
    origin: gpui::FocusHandle,
    fallback: gpui::FocusHandle,
    pub(super) effect_started: bool,
    pub(super) original: OriginalImageCommands,
}
pub(super) struct DirectImageFeedback {
    target: crate::syndic_transcript::ResidentTranscriptContextMenuTarget,
    message: &'static str,
}
impl SyndicTranscriptPanel {
    #[cfg(test)]
    pub(crate) fn test_image_direct_original_command(
        &self,
        save: bool,
    ) -> Option<(Option<&'static str>, &'static str)> {
        let commands = &self.quote_state.image_commands.as_ref()?.original;
        Some((
            commands.reason(if save {
                OriginalImageRequest::Save
            } else {
                OriginalImageRequest::Copy
            }),
            commands.feedback,
        ))
    }
    pub(crate) fn set_image_original_media_window(
        &mut self,
        media: Option<&std::sync::Arc<ImageMediaWindow>>,
    ) {
        let next = media.map(std::sync::Arc::downgrade).unwrap_or_default();
        if !std::sync::Weak::ptr_eq(&next, &self.quote_state.image_media) {
            self.quote_state.image_commands = None;
            self.quote_state.image_command_feedback = None;
            self.quote_state.image_media = next;
        }
    }
    pub(super) fn open_image_original_commands(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.quote_state.image_commands = None;
        self.quote_state.image_command_feedback = None;
        let Some(capture) = self.quote_state.turn.as_ref() else {
            return;
        };
        let Some((_, origin)) = self.quote_state.marker_origin.as_ref() else {
            return;
        };
        let Some(fallback) = self.image_marker_return_focus(window) else {
            return;
        };
        let record = &capture.target.record;
        let crate::syndic_transcript::ResidentContextMenuContentKind::ImageMarker(marker) =
            record.content_kind
        else {
            return;
        };
        let demand = (|| {
            Some(ImageSourceDemand::TranscriptMarker {
                thread_id: record.source.view_id.0.parse().ok()?,
                entry: record.source.transcript_entry?,
                projection_id: record.projection_id.0.parse().ok()?,
                projection_revision: beryl_model::ProjectionRevision::new(
                    record.projection_revision.0,
                )
                .ok()?,
                marker,
            })
        })();
        self.quote_state.image_commands = Some(DirectImageCommands {
            target: capture.target.clone(),
            activation: capture.activation,
            serial: self.quote_state.serial,
            origin: origin.clone(),
            fallback,
            effect_started: false,
            original: OriginalImageCommands::new(self.quote_state.image_media.clone(), demand),
        });
        if let Some(commands) = self.quote_state.image_commands.as_mut() {
            commands.original.poll(cx);
        }
    }
    fn image_original_current(&self, window: &Window) -> bool {
        let Some(commands) = self.quote_state.image_commands.as_ref() else {
            return false;
        };
        let snapshot = self.host.shared_snapshot();
        !self.initial_history_loading
            && self.character_geometry_current(window)
            && commands.serial == self.quote_state.serial
            && commands.activation == snapshot.activation_revision
            && commands.target.presentation_revision == snapshot.presentation_revision
            && self
                .image_marker_bounds(&commands.target.record.record_id, window)
                .is_some()
            && self.last_frame_window.as_ref().is_some_and(|frame| {
                super::super::super::super::resident_context_menu_frame_loss(
                    &snapshot,
                    frame,
                    &commands.target,
                )
                .is_none()
            })
    }
    pub(super) fn poll_image_original_commands(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.image_original_current(window) {
            if let Some(commands) = self.quote_state.image_commands.take()
                && commands.origin.is_focused(window)
            {
                window.focus(&commands.fallback);
            }
            return;
        }
        if let Some(commands) = self.quote_state.image_commands.as_mut() {
            let picker = commands.original.picker_pending();
            commands.original.poll(cx);
            if picker && !commands.original.picker_pending() && self.quote_state.popup.is_none() {
                window.focus(&commands.origin);
            }
        }
    }
    pub(super) fn activate_image_original_command(
        &mut self,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.transcript_menu_current(window) || !self.image_original_current(window) {
            return;
        }
        let request = if save {
            OriginalImageRequest::Save
        } else {
            OriginalImageRequest::Copy
        };
        if self
            .quote_state
            .image_commands
            .as_ref()
            .is_none_or(|commands| commands.original.reason(request).is_some())
        {
            return;
        }
        if let Some(commands) = self.quote_state.image_commands.as_mut() {
            commands.effect_started = commands.original.start(request, cx);
            if !commands.effect_started {
                self.quote_state.image_command_feedback = Some(DirectImageFeedback {
                    target: commands.target.clone(),
                    message: commands.original.feedback,
                });
            }
        }
        self.close_quote_menu(window);
        cx.notify();
    }
    pub(super) fn image_original_feedback(&self, window: &Window) -> Option<AnyElement> {
        let (target, message) = if let Some(commands) = self.quote_state.image_commands.as_ref() {
            (&commands.target, commands.original.feedback)
        } else {
            let feedback = self.quote_state.image_command_feedback.as_ref()?;
            (&feedback.target, feedback.message)
        };
        if message.is_empty()
            || !self.last_frame_window.as_ref().is_some_and(|frame| {
                super::super::super::super::resident_context_menu_frame_loss(
                    &self.host.shared_snapshot(),
                    frame,
                    target,
                )
                .is_none()
            })
        {
            return None;
        }
        let bounds = self.image_marker_bounds(&target.record.record_id, window)?;
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(bounds.origin + point(px(0.0), bounds.size.height))
                    .child(
                        div()
                            .id("transcript-image-command-feedback")
                            .text_xs()
                            .p_1()
                            .bg(gpui::rgb(0x20242b))
                            .text_color(gpui::rgb(0xf2f4f7))
                            .child(message),
                    ),
            )
            .into_any_element(),
        )
    }
}
