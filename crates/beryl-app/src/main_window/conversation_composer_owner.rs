use gpui::{
    App, AppContext, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Subscription,
    Window,
};
use gpui_text_input::{
    ClipboardCompletion, ClipboardKind, ClipboardLimits, ClipboardWriteOutcome,
    InlineObjectActivation, InlineObjectSurfaceAttachment, InlineObjectSurfaceDismissal,
    MutationLimits, OperationId, RangeSourceSelection, RangeTextInput, RangeTextInputEvent,
    RangeTextInputRequest, RealizedInlineObjectAnchor, TextInputCommand,
};
use std::sync::Arc;

use crate::composer_host::ComposerHostImageMarkerMetadata;

use super::{
    ComposerImagePresentationState, ComposerImagePreviewShell, ComposerMarkerFocusTarget,
    ComposerMarkerMenu, MainWindowComposerActivationAdvance, MainWindowComposerActivationReceipt,
    MainWindowComposerAutosaveCaptureRequirement, MainWindowComposerDispatchOutcome,
    MainWindowComposerDisposalAdvance, MainWindowComposerImageSurfaces,
    MainWindowComposerPendingStatus, MainWindowComposerPublishAdvance,
    MainWindowComposerRetirementAdvance, MainWindowComposerSelectionIdentity,
    MainWindowComposerWidgetRelease, MainWindowConversationComposerConfig,
};

mod clipboard;
mod construction;
mod dispatch;
mod lifecycle;
mod render;
mod service;

pub use service::MainWindowConversationComposerService;

pub type ComposerClipboardWriter =
    Box<dyn FnMut(&str, &mut App) -> ClipboardWriteOutcome + 'static>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowConversationComposerEvent {
    SelectionAdvanced {
        previous: MainWindowComposerSelectionIdentity,
        current: MainWindowComposerSelectionIdentity,
    },
    RichPastePropagated,
}

#[derive(Clone, Copy)]
enum MainWindowConversationComposerPhase {
    Live,
    Fencing,
    Releasing,
    Released(MainWindowComposerWidgetRelease),
    ReleaseFailed,
}

pub struct MainWindowConversationComposer {
    input: Entity<RangeTextInput>,
    service: Arc<MainWindowConversationComposerService>,
    selection: MainWindowComposerSelectionIdentity,
    clipboard_writer: ComposerClipboardWriter,
    proof_limits: super::MainWindowComposerSuccessorProofLimits,
    clipboard_limits: ClipboardLimits,
    mutation_limits: MutationLimits,
    next_operation: u64,
    image_surfaces: MainWindowComposerImageSurfaces,
    image_surface_focus: FocusHandle,
    image_surface_attachment: Option<InlineObjectSurfaceAttachment>,
    pending_marker_removal: Option<gpui_text_input::MutationKey>,
    propagated_cut: Option<clipboard::ActivePropagatedCut>,
    pending_marker_metadata: Option<(
        gpui_text_input::MutationKey,
        Box<[ComposerHostImageMarkerMetadata]>,
    )>,
    admitted_positions: Option<gpui_text_input::MutationPositions>,
    next_flight: u64,
    active_flight: Option<u64>,
    phase: MainWindowConversationComposerPhase,
    scheduled: bool,
    last_error: Option<String>,
    _input_subscription: Option<Subscription>,
    _input_event_subscription: Option<Subscription>,
}

impl EventEmitter<MainWindowConversationComposerEvent> for MainWindowConversationComposer {}

impl MainWindowConversationComposer {
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub const fn marker_menu(&self) -> Option<ComposerMarkerMenu> {
        self.image_surfaces.menu()
    }

    pub const fn image_preview(&self) -> Option<ComposerImagePreviewShell> {
        self.image_surfaces.preview()
    }

    fn activate_marker(
        &mut self,
        activation: InlineObjectActivation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let disposition = self
            .image_surfaces
            .activate_marker(self.selection, activation)
            .map_err(|error| error.to_string())?;
        if matches!(
            disposition,
            super::ComposerMarkerActivationDisposition::Opened
        ) {
            let attachment = match self.input.update(cx, |input, _| {
                input.attach_active_inline_object_surface(activation.anchor)
            }) {
                Ok(attachment) => attachment,
                Err(error) => {
                    self.image_surfaces.dismiss_menu(false, self.is_live());
                    return Err(error.to_string());
                }
            };
            self.image_surface_attachment = Some(attachment);
        }
        self.image_surface_focus.focus(window);
        cx.notify();
        Ok(())
    }

    pub fn invoke_marker_view(
        &mut self,
        state: ComposerImagePresentationState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.image_surfaces
            .invoke_view(self.selection, state)
            .map_err(|error| error.to_string())?;
        self.image_surface_focus.focus(window);
        cx.notify();
        Ok(())
    }

    pub fn invoke_marker_remove(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<gpui_text_input::MutationKey, String> {
        let anchor = self
            .image_surfaces
            .prepare_remove(self.selection)
            .map_err(|error| error.to_string())?;
        let active = self
            .input
            .update(cx, |input, _| input.active_inline_object())
            .ok_or_else(|| "composer marker origin is no longer realized".to_owned())?;
        if active != anchor {
            return Err("composer marker remove origin became stale".into());
        }
        let key = self
            .input
            .update(cx, |input, input_cx| {
                input.remove_active_inline_object(anchor, input_cx)
            })
            .map_err(|error| error.to_string())?;
        self.pending_marker_removal = Some(key);
        let removed = self
            .image_surfaces
            .invoke_remove(self.selection)
            .map_err(|error| error.to_string())?;
        if removed != anchor {
            return Err("composer marker remove origin changed during admission".to_owned());
        }
        Ok(key)
    }

    pub fn insert_authenticated_image_marker(
        &mut self,
        metadata: ComposerHostImageMarkerMetadata,
        order: gpui_text_input::InlineObjectOrder,
        cx: &mut Context<Self>,
    ) -> Result<gpui_text_input::MutationKey, String> {
        if !self.is_live() || self.pending_marker_metadata.is_some() {
            return Err("composer marker insertion lane is busy".to_owned());
        }
        let retained_bytes = std::mem::size_of::<ComposerHostImageMarkerMetadata>();
        let key = self
            .input
            .update(cx, |input, input_cx| {
                input.insert_inline_object_at_selection(
                    metadata.object_id(),
                    order,
                    retained_bytes,
                    0,
                    input_cx,
                )
            })
            .map_err(|error| error.to_string())?;
        self.pending_marker_metadata = Some((key, Box::new([metadata])));
        Ok(key)
    }

    pub fn dismiss_marker_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Option<ComposerMarkerFocusTarget>, String> {
        let Some(anchor) = self.image_surfaces.menu().map(|menu| menu.anchor()) else {
            return Ok(None);
        };
        let origin_eligible = self.exact_origin_is_active(anchor, cx);
        let target = self
            .image_surfaces
            .dismiss_menu(origin_eligible, self.is_live());
        self.finish_surface_dismissal(target, window, cx)?;
        Ok(target)
    }

    pub fn dismiss_image_preview(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Option<ComposerMarkerFocusTarget>, String> {
        let Some(anchor) = self
            .image_surfaces
            .preview()
            .map(|preview| preview.origin())
        else {
            return Ok(None);
        };
        let origin_eligible = self.exact_origin_is_active(anchor, cx);
        let target = self
            .image_surfaces
            .dismiss_preview(origin_eligible, self.is_live());
        self.finish_surface_dismissal(target, window, cx)?;
        Ok(target)
    }

    fn exact_origin_is_active(
        &self,
        anchor: RealizedInlineObjectAnchor,
        cx: &mut Context<Self>,
    ) -> bool {
        self.image_surface_attachment
            .as_ref()
            .is_some_and(|attachment| attachment.anchor() == anchor)
            && self
                .input
                .update(cx, |input, _| input.active_inline_object())
                == Some(anchor)
    }

    fn finish_surface_dismissal(
        &mut self,
        target: Option<ComposerMarkerFocusTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(target) = target else {
            return Ok(());
        };
        let attachment = self.image_surface_attachment.take();
        if let Some(attachment) = attachment {
            let dismissal = match target {
                ComposerMarkerFocusTarget::OriginMarker(anchor)
                    if attachment.anchor() == anchor =>
                {
                    InlineObjectSurfaceDismissal::RefocusObject
                }
                _ => InlineObjectSurfaceDismissal::ClearObject,
            };
            if let Err(error) = self.input.update(cx, |input, input_cx| {
                input.dismiss_active_inline_object_surface(attachment, dismissal, window, input_cx)
            }) && !matches!(error, gpui_text_input::RangeTextInputError::Stale)
            {
                return Err(error.to_string());
            }
        }
        if matches!(target, ComposerMarkerFocusTarget::ComposerEditor) {
            self.input.update(cx, |input, _| input.focus(window));
        }
        cx.notify();
        Ok(())
    }

    pub fn production_clipboard_writer() -> ComposerClipboardWriter {
        Box::new(|text, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
            ClipboardWriteOutcome::Written
        })
    }

    fn begin_propagated_clipboard(
        &mut self,
        kind: ClipboardKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_live() || self.active_flight.is_some() || self.last_error.is_some() {
            return;
        }
        let Some(selected_range) = self.input.update(cx, |input, _| {
            input.surface().map(|surface| surface.selection())
        }) else {
            self.last_error = Some("propagated clipboard has no coherent selection".to_owned());
            return;
        };
        self.input
            .update(cx, |input, cx| input.set_enabled(false, cx));
        let flight = match self.begin_flight() {
            Ok(flight) => flight,
            Err(error) => {
                self.last_error = Some(error);
                return;
            }
        };
        let service = self.service.clone();
        let selection = self.selection;
        let limits = self.clipboard_limits;
        let task = cx.background_executor().spawn(async move {
            clipboard::collect(&service, selection, selected_range, kind, limits)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.settle_flight(flight) {
                    return;
                }
                if let Err(error) =
                    this.finish_propagated_clipboard(selected_range, result, window, cx)
                {
                    this.last_error = Some(error);
                }
                this.schedule_pump(window, cx);
            });
        })
        .detach();
    }

    fn finish_propagated_clipboard(
        &mut self,
        selected_range: RangeSourceSelection,
        result: Result<clipboard::PropagatedClipboardCollection, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.input
                    .update(cx, |input, cx| input.set_enabled(true, cx));
                return Err(error);
            }
        };
        let cut = match result {
            clipboard::PropagatedClipboardCollection::Rejected => None,
            clipboard::PropagatedClipboardCollection::Ready {
                mut coordinator,
                write,
            } => {
                let key = write.key();
                let outcome = (self.clipboard_writer)(write.text(), cx);
                match coordinator
                    .acknowledge_write(key, outcome)
                    .map_err(|error| error.to_string())?
                {
                    ClipboardCompletion::Delete(deletion) => Some(deletion),
                    ClipboardCompletion::Copied
                    | ClipboardCompletion::WriteFailed
                    | ClipboardCompletion::Cancelled => None,
                    completion => {
                        return Err(format!(
                            "composer clipboard write settled unexpectedly: {completion:?}"
                        ));
                    }
                }
            }
        };
        let Some(deletion) = cut else {
            self.input
                .update(cx, |input, cx| input.set_enabled(true, cx));
            return Ok(());
        };
        if deletion.selection()
            != selected_range
                .range()
                .map_err(|error| format!("composer cut selection became malformed: {error:?}"))?
        {
            return Err("composer cut selection changed before deletion".into());
        }
        self.begin_cut_after_write(deletion, window, cx);
        Ok(())
    }

    fn begin_cut_after_write(
        &mut self,
        deletion: gpui_text_input::CutDeletion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_live() {
            return;
        }
        let flight = match self.begin_flight() {
            Ok(flight) => flight,
            Err(error) => {
                self.last_error = Some(error);
                return;
            }
        };
        let service = self.service.clone();
        let selection = self.selection;
        let proof_limits = self.proof_limits;
        let mutation_limits = self.mutation_limits;
        let task = cx.background_executor().spawn(async move {
            clipboard::prepare_cut_after_write(
                &service,
                selection,
                deletion,
                proof_limits,
                mutation_limits,
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.settle_flight(flight) {
                    return;
                }
                this.input
                    .update(cx, |input, cx| input.set_enabled(true, cx));
                match result.and_then(|prepared| {
                    this.input.update(cx, |input, input_cx| {
                        prepared.begin(OperationId::new(this.next_operation), input, input_cx)
                    })
                }) {
                    Ok(active) => this.propagated_cut = Some(active),
                    Err(error) => this.last_error = Some(error),
                }
                this.schedule_pump(window, cx);
            });
        })
        .detach();
    }

    fn schedule_pump(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.can_pump()
            || self.scheduled
            || self.active_flight.is_some()
            || self.last_error.is_some()
        {
            return;
        }
        self.scheduled = true;
        cx.defer_in(window, |this, window, cx| {
            this.scheduled = false;
            this.pump_one(window, cx);
        });
    }

    fn begin_flight(&mut self) -> Result<u64, String> {
        if self.active_flight.is_some() {
            return Err("another composer operation already owns the lifecycle lane".to_owned());
        }
        let flight = self.next_flight;
        self.next_flight = self
            .next_flight
            .checked_add(1)
            .ok_or_else(|| "composer lifecycle generation exhausted".to_owned())?;
        self.active_flight = Some(flight);
        Ok(flight)
    }

    fn settle_flight(&mut self, flight: u64) -> bool {
        if self.active_flight != Some(flight) {
            return false;
        }
        self.active_flight = None;
        true
    }
}
