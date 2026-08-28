use gpui::{
    App, AppContext, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Subscription,
    WeakEntity, Window,
};
use gpui_text_input::{
    ClipboardCompletion, ClipboardKind, ClipboardLimits, ClipboardWriteOutcome,
    InlineObjectActivation, InlineObjectSurfaceAttachment, InlineObjectSurfaceDismissal,
    MutationLimits, ObjectPurpose, PagePurpose, RangeSourceSelection, RangeTextInput,
    RangeTextInputEvent, RangeTextInputRequest, RealizedInlineObjectAnchor, TextInputCommand,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Weak},
};

use crate::composer_host::ComposerHostImageMarkerMetadata;

use super::{
    ComposerImagePresentationState, ComposerImagePreviewShell, ComposerMarkerFocusTarget,
    ComposerMarkerMenu, MainWindowComposerActivationAdvance, MainWindowComposerActivationReceipt,
    MainWindowComposerAutosaveCaptureRequirement, MainWindowComposerDispatchOutcome,
    MainWindowComposerDisposalAdvance, MainWindowComposerImageSurfaces,
    MainWindowComposerPublishAdvance, MainWindowComposerResidencyBound,
    MainWindowComposerResidencyUsage, MainWindowComposerRetirementAdvance,
    MainWindowComposerSelectionIdentity, MainWindowComposerWidgetRelease,
    MainWindowConversationComposerConfig,
};

mod clipboard;
mod construction;
mod dispatch;
mod lifecycle;
mod realization;
mod render;
mod service;

pub use realization::*;
pub use service::MainWindowConversationComposerService;

pub type ComposerClipboardWriter =
    Box<dyn FnMut(&str, &mut App) -> ClipboardWriteOutcome + 'static>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowConversationComposerEvent {
    SelectionAdvanced {
        previous: MainWindowComposerSelectionIdentity,
        current: MainWindowComposerSelectionIdentity,
    },
    RichPastePropagated {
        selection: MainWindowComposerSelectionIdentity,
    },
    ClipboardLimitExceeded {
        selection: MainWindowComposerSelectionIdentity,
    },
    SubmitPropagated {
        selection: MainWindowComposerSelectionIdentity,
    },
}

#[derive(Clone, Copy)]
enum MainWindowConversationComposerPhase {
    Live,
    Fencing,
    Releasing,
    Released(MainWindowComposerWidgetRelease),
    ReleaseFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MainWindowConversationComposerRoute {
    Selected,
    Pending(MainWindowComposerActivationReceipt),
}

pub(in crate::main_window) enum MainWindowConversationComposerActivationSeed {
    Page(crate::composer_host::ComposerHostResponse),
    ObjectPage(crate::composer_host::ComposerHostResponse),
}

struct MainWindowConversationComposerPendingRealizer {
    receipt: MainWindowComposerActivationReceipt,
    composer: WeakEntity<MainWindowConversationComposer>,
    lifetime: Weak<()>,
}

pub(in crate::main_window) struct MainWindowConversationComposerPendingRealizerToken {
    _lifetime: Arc<()>,
}

pub struct MainWindowConversationComposer {
    input: Entity<RangeTextInput>,
    service: Arc<MainWindowConversationComposerService>,
    selection: MainWindowComposerSelectionIdentity,
    route: MainWindowConversationComposerRoute,
    pending_realizer: Option<MainWindowConversationComposerPendingRealizer>,
    residency_bound: MainWindowComposerResidencyBound,
    activation_seeds: VecDeque<MainWindowConversationComposerActivationSeed>,
    clipboard_writer: ComposerClipboardWriter,
    proof_limits: super::MainWindowComposerSuccessorProofLimits,
    clipboard_limits: ClipboardLimits,
    mutation_limits: MutationLimits,
    image_surfaces: MainWindowComposerImageSurfaces,
    image_surface_focus: FocusHandle,
    image_surface_attachment: Option<InlineObjectSurfaceAttachment>,
    pending_marker_removal: Option<gpui_text_input::MutationKey>,
    propagated_clipboard: Option<clipboard::ActivePropagatedClipboard>,
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

    pub fn realization_diagnostics(
        &self,
        cx: &App,
    ) -> gpui_text_input::RangeRealizationDiagnostics {
        self.input
            .read_with(cx, |input, _| input.realization_diagnostics())
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
            .map_err(|_| "composer marker activation was rejected".to_owned())?;
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
                    return Err("composer marker surface was rejected".into());
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
            .map_err(|_| "composer marker view was rejected".to_owned())?;
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
            .map_err(|_| "composer marker removal was rejected".to_owned())?;
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
            .map_err(|_| "composer marker mutation was rejected".to_owned())?;
        self.pending_marker_removal = Some(key);
        let removed = self
            .image_surfaces
            .invoke_remove(self.selection)
            .map_err(|_| "composer marker removal was rejected".to_owned())?;
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
            .map_err(|error| format!("composer marker metadata mutation was rejected: {error}"))?;
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
                return Err("composer marker surface dismissal was rejected".into());
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
        match clipboard::ActivePropagatedClipboard::new(
            self.selection,
            selected_range,
            kind,
            self.clipboard_limits,
        ) {
            Ok(clipboard) => {
                self.propagated_clipboard = Some(clipboard);
                self.drive_propagated_clipboard(selected_range, window, cx);
            }
            Err(error) => {
                self.input
                    .update(cx, |input, cx| input.set_enabled(true, cx));
                self.last_error = Some(error);
            }
        }
    }

    fn drive_propagated_clipboard(
        &mut self,
        selected_range: RangeSourceSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = match self
            .propagated_clipboard
            .as_mut()
            .ok_or_else(|| "composer clipboard scan was released".to_owned())
            .and_then(clipboard::ActivePropagatedClipboard::next_action)
        {
            Ok(action) => action,
            Err(error) => {
                self.finish_propagated_clipboard_without_cut(cx);
                self.last_error = Some(error);
                return;
            }
        };
        match action {
            clipboard::PropagatedClipboardAction::Request(request) => {
                let flight = match self.begin_flight() {
                    Ok(flight) => flight,
                    Err(error) => {
                        self.finish_propagated_clipboard_without_cut(cx);
                        self.last_error = Some(error);
                        return;
                    }
                };
                let service = self.service.clone();
                let selection = self.selection;
                let cancellation = self
                    .propagated_clipboard
                    .as_ref()
                    .expect("clipboard scan remains active")
                    .cancellation();
                let task = cx.background_executor().spawn(async move {
                    let mut slot = service
                        .slot
                        .lock()
                        .map_err(|_| "composer clipboard host lane is unavailable".to_owned())?;
                    slot.dispatch_selected_request(
                        &service.store,
                        selection,
                        request,
                        Box::new([]),
                        &cancellation,
                    )
                    .map_err(|_| "composer clipboard page request failed".to_owned())
                });
                cx.spawn_in(window, async move |this, cx| {
                    let result = task.await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        if !this.settle_flight(flight) {
                            return;
                        }
                        if !this.is_live() {
                            this.schedule_pump(window, cx);
                            return;
                        }
                        if this.propagated_clipboard.is_none()
                            || this.selection != selection
                            || this.service.selected_identity() != Some(selection)
                        {
                            return;
                        }
                        match result.and_then(|outcome| {
                            this.propagated_clipboard
                                .as_mut()
                                .ok_or_else(|| "composer clipboard scan is unavailable".to_owned())?
                                .admit(outcome)
                        }) {
                            Ok(()) => this.drive_propagated_clipboard(selected_range, window, cx),
                            Err(error) => {
                                this.finish_propagated_clipboard_without_cut(cx);
                                this.last_error = Some(error);
                            }
                        }
                    });
                })
                .detach();
            }
            clipboard::PropagatedClipboardAction::Write(write) => {
                let key = write.key();
                let outcome = (self.clipboard_writer)(write.text(), cx);
                let completion = self
                    .propagated_clipboard
                    .as_mut()
                    .expect("clipboard scan remains active")
                    .acknowledge_write(key, outcome)
                    .unwrap_or_else(|_| ClipboardCompletion::WriteFailed);
                match completion {
                    ClipboardCompletion::Delete(deletion) => {
                        let expected = match selected_range.range() {
                            Ok(expected) => expected,
                            Err(_) => {
                                self.finish_propagated_clipboard_without_cut(cx);
                                self.last_error =
                                    Some("composer cut selection was malformed".into());
                                return;
                            }
                        };
                        self.propagated_clipboard = None;
                        if deletion.selection() != expected {
                            self.input
                                .update(cx, |input, cx| input.set_enabled(true, cx));
                            self.last_error =
                                Some("composer cut selection changed before deletion".into());
                            return;
                        }
                        self.begin_cut_after_write(deletion, window, cx);
                    }
                    ClipboardCompletion::Copied
                    | ClipboardCompletion::WriteFailed
                    | ClipboardCompletion::Cancelled => {
                        self.finish_propagated_clipboard_without_cut(cx)
                    }
                    _ => {
                        self.finish_propagated_clipboard_without_cut(cx);
                        self.last_error =
                            Some("composer clipboard write terminated unexpectedly".into());
                    }
                }
            }
            clipboard::PropagatedClipboardAction::ContiguousLimitExceeded => {
                self.finish_propagated_clipboard_without_cut(cx);
                cx.emit(
                    MainWindowConversationComposerEvent::ClipboardLimitExceeded {
                        selection: self.selection,
                    },
                );
            }
            clipboard::PropagatedClipboardAction::Cancelled => {
                self.finish_propagated_clipboard_without_cut(cx);
            }
        }
    }

    fn finish_propagated_clipboard_without_cut(&mut self, cx: &mut Context<Self>) {
        if let Some(clipboard) = self.propagated_clipboard.take() {
            clipboard.cancel();
        }
        if self.is_live() {
            self.input
                .update(cx, |input, cx| input.set_enabled(true, cx));
        }
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
            #[cfg(feature = "test-faults")]
            if let Some(gate) = service.take_test_cut_preparation_gate() {
                gate.await;
            }
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
                if !this.is_live() {
                    this.schedule_pump(window, cx);
                    return;
                }
                if this.selection != selection
                    || this.service.selected_identity() != Some(selection)
                {
                    return;
                }
                this.input
                    .update(cx, |input, cx| input.set_enabled(true, cx));
                match result.and_then(|prepared| {
                    this.input
                        .update(cx, |input, input_cx| prepared.begin(input, input_cx))
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
