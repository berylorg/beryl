use gpui::{
    App, AppContext, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Subscription,
    WeakEntity, Window,
};
use gpui_text_input::{
    ClipboardCompletion, ClipboardKind, ClipboardLimits, ClipboardWriteOutcome,
    InlineObjectActivation, InlineObjectSurfaceAttachment, InlineObjectSurfaceDismissal,
    MutationLimits, ObjectPurpose, PagePurpose, RangeRestorationSeed, RangeSourceSelection,
    RangeTextInput, RangeTextInputEvent, RangeTextInputRequest, RealizedInlineObjectAnchor,
    TextInputCommand,
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
mod close;
mod construction;
mod detached;
mod dispatch;
mod failed_resident;
mod fresh_candidate;
mod lifecycle;
mod prepublication;
mod realization;
mod recovery;
mod render;
mod selected_preparation;
mod service;
pub(in crate::main_window) use service::{
    MainWindowComposerClaimAdvance, MainWindowComposerClaimAutosave,
    MainWindowComposerClaimCompletion, MainWindowComposerClaimPreparedPresentation,
    MainWindowComposerClaimPublication, MainWindowComposerClaimWidgetWork,
};
mod shutdown;
mod startup;
mod surviving_native_close;

pub use failed_resident::preparation::{
    MainWindowFailedResidentAdoption, MainWindowFailedResidentPreparation,
};
pub use failed_resident::{MainWindowFailedResidentCapture, MainWindowFailedResidentTicket};
#[cfg(feature = "test-faults")]
pub use prepublication::MainWindowNativeLineagePrepublicationDiagnostics;
pub(in crate::main_window) use prepublication::{
    MainWindowNativeLineagePrepublicationResult, MainWindowNativeLineagePrepublicationSource,
    MainWindowNativeLineagePrepublicationWork,
};
pub use realization::*;
pub use recovery::{
    MainWindowComposerRecoveryPreparation, MainWindowComposerRecoveryProgress,
    MainWindowComposerRecoveryResources, MainWindowComposerRecoverySnapshot,
};
pub use selected_preparation::MainWindowConversationComposerPreparedSelection;
pub use service::MainWindowComposerCandidateSource;
pub use service::MainWindowConversationComposerService;
pub(crate) use service::MainWindowThreadPredecessorSave;
pub use service::MainWindowFailedResidentCandidateSource;
pub(in crate::main_window) use service::MainWindowNativeLineageSourceRetentionError;
pub use service::{
    MainWindowComposerCandidateCompletion, MainWindowComposerCandidateCustody,
    MainWindowComposerCandidateRead, MainWindowComposerCandidateWorker,
};
#[cfg(feature = "test-faults")]
pub use service::{
    MainWindowNativeLineageCleanupTestWitness, MainWindowNativeLineageCleanupTestWitnessSnapshot,
    MainWindowSelectedComposerPreparationTestFault,
};

pub type ComposerClipboardWriter =
    Box<dyn FnMut(&str, &mut App) -> ClipboardWriteOutcome + 'static>;

pub type ComposerCheckedClipboardWriter =
    Box<dyn FnMut(&str, Option<&str>, &mut App) -> ClipboardWriteOutcome + 'static>;

pub type ComposerCheckedClipboardReader = Box<
    dyn FnMut(
            gpui::ClipboardLimits,
            &mut App,
        ) -> Result<gpui::CheckedClipboardSnapshot, gpui::ClipboardError>
        + 'static,
>;

pub use clipboard::resources::{
    MainWindowComposerPasteResourceError, MainWindowComposerPasteResources,
};
pub use clipboard::source::{
    MainWindowPrivateClipboardDescriptor, MainWindowPrivateClipboardOwner,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowComposerMutationFeedbackKind {
    OperationTooLarge,
    CapacityUnavailable,
    Storage,
    Refused,
    Unavailable,
    AdmittedWorkUnavailable,
    CommittedUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerMutationFeedback {
    pub selection: MainWindowComposerSelectionIdentity,
    pub key: gpui_text_input::MutationKey,
    pub kind: MainWindowComposerMutationFeedbackKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowComposerClipboardFeedbackKind {
    Failed,
    TooLarge,
    CapacityUnavailable,
    Unavailable,
    StorageUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerClipboardFeedback {
    pub selection: MainWindowComposerSelectionIdentity,
    pub operation: gpui_text_input::ClipboardId,
    pub kind: MainWindowComposerClipboardFeedbackKind,
    pub paste: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowConversationComposerEvent {
    SelectionAdvanced {
        previous: MainWindowComposerSelectionIdentity,
        current: MainWindowComposerSelectionIdentity,
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
    RecoveryFenced,
    Detached,
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
    recovery_config: MainWindowConversationComposerConfig,
    input: Entity<RangeTextInput>,
    service: Option<Arc<MainWindowConversationComposerService>>,
    detached: Option<detached::DetachedComposer>,
    selection: MainWindowComposerSelectionIdentity,
    route: MainWindowConversationComposerRoute,
    pending_realizer: Option<MainWindowConversationComposerPendingRealizer>,
    residency_bound: MainWindowComposerResidencyBound,
    activation_seeds: VecDeque<MainWindowConversationComposerActivationSeed>,
    clipboard_writer: Option<ComposerClipboardWriter>,
    checked_clipboard_writer: Option<ComposerCheckedClipboardWriter>,
    checked_clipboard_reader: Option<ComposerCheckedClipboardReader>,
    paste_queue: clipboard::resources::ComposerPasteQueue,
    paste: Option<clipboard::paste::CapturedComposerPaste>,
    private_clipboard_owner: MainWindowPrivateClipboardOwner,
    private_clipboard_preparation: Option<clipboard::source::PrivateClipboardPreparation>,
    clipboard_operation: gpui_text_input::ClipboardId,
    clipboard_feedback: Option<MainWindowComposerClipboardFeedback>,
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
    mutation_evidence: Option<dispatch::ActiveComposerMutationEvidence>,
    last_mutation_admission_failure:
        Option<std::sync::Arc<crate::composer_host::ComposerHostMutationAdmissionFailure>>,
    mutation_feedback: Option<MainWindowComposerMutationFeedback>,
    admitted_positions: Option<gpui_text_input::MutationPositions>,
    next_flight: u64,
    active_flight: Option<u64>,
    pending_dispatch: Option<dispatch::MainWindowConversationComposerPendingDispatch>,
    phase: MainWindowConversationComposerPhase,
    release_fence_requires_restoration: bool,
    window_close: Option<super::MainWindowConversationComposerCloseTicket>,
    recovery_snapshot: Option<MainWindowComposerRecoverySnapshot>,
    failed_resident: Option<failed_resident::FailedResidentFence>,
    failed_resident_generation: u64,
    unpublished_recovery_protection: Option<(
        super::MainWindowConversationComposerCloseTicket,
        gpui_text_input::RangeResidentProtection,
    )>,
    fresh_recovery_release_requests: Option<Vec<RangeTextInputRequest>>,
    fresh_recovery_gui: Option<fresh_candidate::FreshRecoveryGuiPreparation>,
    startup_interaction_gated: bool,
    shutdown_interaction_gated: bool,
    startup_release_started: bool,
    startup_release_completion:
        Option<futures_channel::oneshot::Sender<Result<MainWindowComposerWidgetRelease, String>>>,
    scheduled: bool,
    last_error: Option<String>,
    _input_subscription: Option<Subscription>,
    _input_event_subscription: Option<Subscription>,
}

impl EventEmitter<MainWindowConversationComposerEvent> for MainWindowConversationComposer {}

impl Drop for MainWindowConversationComposer {
    fn drop(&mut self) {
        if let Some(paste) = self.paste.as_ref() {
            paste.cancellation.cancel();
        }
        self.private_clipboard_owner.expire_origin(self.selection);
        if let Some(clipboard) = self.propagated_clipboard.as_ref() {
            clipboard.cancel();
        }
    }
}

impl MainWindowConversationComposer {
    pub(crate) fn interrupted_exit_configurator(
        &mut self,
        cx: &Context<Self>,
    ) -> Result<
        Box<
            dyn FnMut(
                MainWindowComposerSelectionIdentity,
            ) -> Result<
                (
                    MainWindowConversationComposerConfig,
                    gpui_text_input::RangeSurfaceCharge,
                ),
                String,
            >,
        >,
        String,
    > {
        self.recovery_config
            .retain_resident_layout(self.input.read(cx).resident_layout_snapshot());
        self.recovery_config.interrupted_exit_configurator()
    }
    pub fn mutation_feedback(&self) -> Option<MainWindowComposerMutationFeedback> {
        self.mutation_feedback.filter(|feedback| {
            self.route == MainWindowConversationComposerRoute::Selected
                && feedback.selection == self.selection
                && matches!(
                    self.phase,
                    MainWindowConversationComposerPhase::Live
                        | MainWindowConversationComposerPhase::Fencing
                )
        })
    }

    pub fn clipboard_feedback(&self) -> Option<MainWindowComposerClipboardFeedback> {
        self.clipboard_feedback
    }

    fn report_clipboard_feedback(
        &mut self,
        kind: MainWindowComposerClipboardFeedbackKind,
        cx: &mut Context<Self>,
    ) {
        if !self.mutation_feedback.is_some_and(|feedback| {
            matches!(
                feedback.kind,
                MainWindowComposerMutationFeedbackKind::Unavailable
                    | MainWindowComposerMutationFeedbackKind::AdmittedWorkUnavailable
                    | MainWindowComposerMutationFeedbackKind::CommittedUnavailable
            )
        }) {
            self.mutation_feedback = None;
        }
        self.clipboard_feedback = Some(MainWindowComposerClipboardFeedback {
            selection: self.selection,
            operation: self.clipboard_operation,
            kind,
            paste: false,
        });
        cx.notify();
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn last_mutation_admission_failure(
        &self,
    ) -> Option<&crate::composer_host::ComposerHostMutationAdmissionFailure> {
        self.last_mutation_admission_failure.as_deref()
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
        if self.startup_interaction_gated {
            return Err("conversation composer is waiting for startup".to_owned());
        }
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
        if self.startup_interaction_gated {
            return Err("conversation composer is waiting for startup".to_owned());
        }
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
        if self.mutation_gated() {
            return Err("conversation composer mutation is gated".to_owned());
        }
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
        if !self.is_live() || self.mutation_gated() || self.pending_marker_metadata.is_some() {
            return Err("composer marker insertion lane is busy".to_owned());
        }
        let retained_bytes = metadata.retained_bytes();
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
        Box::new(|text, cx| Self::checked_write(text, None, 64 * 1024, cx))
    }

    pub fn production_checked_clipboard_writer(
        limits: ClipboardLimits,
    ) -> ComposerCheckedClipboardWriter {
        Box::new(move |text, metadata, cx| {
            Self::checked_write(text, metadata, limits.max_bytes(), cx)
        })
    }

    fn checked_write(
        text: &str,
        metadata: Option<&str>,
        max_bytes: usize,
        cx: &mut App,
    ) -> ClipboardWriteOutcome {
        let Some(text_bytes) = max_bytes
            .checked_add(1)
            .and_then(|bytes| bytes.checked_mul(2))
        else {
            return ClipboardWriteOutcome::Failed;
        };
        let metadata_bytes = 1024;
        let Some(total_bytes) = text_bytes.checked_add(metadata_bytes + 8) else {
            return ClipboardWriteOutcome::Failed;
        };
        let item = match metadata {
            Some(metadata) => {
                ClipboardItem::new_string_with_metadata(text.to_owned(), metadata.to_owned())
            }
            None => ClipboardItem::new_string(text.to_owned()),
        };
        match cx.write_to_clipboard_checked(
            &item,
            gpui::ClipboardLimits {
                total_bytes,
                text_bytes,
                metadata_bytes,
                image_bytes: 1,
            },
        ) {
            Ok(()) => ClipboardWriteOutcome::Written,
            Err(_) => ClipboardWriteOutcome::Failed,
        }
    }

    fn begin_propagated_clipboard(
        &mut self,
        kind: ClipboardKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if (!self.is_live() && self.detached.is_none())
            || self.startup_interaction_gated
            || self.active_flight.is_some()
            || (self.last_error.is_some() && self.detached.is_none())
        {
            return;
        }
        if kind == ClipboardKind::Cut && self.mutation_gated() {
            return;
        }
        let Some(selected_range) = self.input.update(cx, |input, _| {
            input.surface().map(|surface| surface.selection())
        }) else {
            self.last_error = Some("propagated clipboard has no coherent selection".to_owned());
            return;
        };
        let mut limits = self
            .clipboard_limits
            .with_provenance(gpui_text_input::ClipboardProvenancePolicy::Omit);
        self.clipboard_operation = clipboard::collection::next_operation();
        self.clipboard_feedback = None;
        if self.detached.is_none() {
            match self.private_clipboard_owner.prepare() {
                Ok(preparation) => self.private_clipboard_preparation = Some(preparation),
                Err(_) => {
                    self.report_clipboard_feedback(
                        MainWindowComposerClipboardFeedbackKind::CapacityUnavailable,
                        cx,
                    );
                    return;
                }
            }
        }
        if self.detached.is_none() && self.checked_clipboard_writer.is_some() {
            limits = limits.with_provenance(gpui_text_input::ClipboardProvenancePolicy::Stream(
                gpui_text_input::ClipboardProvenanceLimits::new(32, 64 * 1024)
                    .expect("bounded composer clipboard provenance capacity"),
            ));
        }
        self.input
            .update(cx, |input, cx| input.set_enabled(false, cx));
        match clipboard::ActivePropagatedClipboard::new(
            self.selection,
            selected_range,
            kind,
            limits,
            self.clipboard_operation,
        ) {
            Ok(clipboard) => {
                self.propagated_clipboard = Some(clipboard);
                self.drive_propagated_clipboard(selected_range, window, cx);
            }
            Err(error) => {
                self.input.update(cx, |input, cx| {
                    input.set_enabled(!self.startup_interaction_gated, cx)
                });
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
                if self.detached.is_some() {
                    self.drive_detached_clipboard_request(request, selected_range, window, cx);
                    return;
                }
                let (flight, service) = match self.begin_flight() {
                    Ok(flight) => flight,
                    Err(error) => {
                        self.finish_propagated_clipboard_without_cut(cx);
                        self.last_error = Some(error);
                        return;
                    }
                };
                let selection = self.selection;
                let clipboard_operation = self.clipboard_operation;
                let cancellation = self
                    .propagated_clipboard
                    .as_ref()
                    .expect("clipboard scan remains active")
                    .cancellation();
                let task = cx.background_executor().spawn(async move {
                    #[cfg(feature = "test-faults")]
                    if let Some(gate) = service.take_test_selected_page_dispatch_gate() {
                        gate.await;
                    }
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
                            if this.clipboard_operation == clipboard_operation {
                                this.finish_propagated_clipboard_without_cut(cx);
                                this.private_clipboard_owner
                                    .expire_operation(clipboard_operation);
                            }
                            this.schedule_pump(window, cx);
                            return;
                        }
                        if this.propagated_clipboard.is_none()
                            || this.selection != selection
                            || this
                                .service
                                .as_ref()
                                .and_then(|service| service.selected_identity())
                                != Some(selection)
                        {
                            if this.clipboard_operation == clipboard_operation {
                                this.finish_propagated_clipboard_without_cut(cx);
                                this.private_clipboard_owner
                                    .expire_operation(clipboard_operation);
                            }
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
                let private = self.private_clipboard_preparation.is_some()
                    && write
                        .provenance()
                        .is_some_and(|closure| closure.item_count() != 0);
                let metadata = match self.private_clipboard_preparation.as_ref() {
                    Some(preparation) => match preparation.begin_write() {
                        Ok(metadata) => Some(metadata),
                        Err(error) => {
                            self.finish_propagated_clipboard_without_cut(cx);
                            self.last_error = Some(error);
                            return;
                        }
                    },
                    None => None,
                };
                let outcome = if let Some(writer) = self.checked_clipboard_writer.as_mut() {
                    writer(write.text(), metadata.as_deref().filter(|_| private), cx)
                } else {
                    self.write_clipboard(write.text(), cx)
                };
                if outcome == ClipboardWriteOutcome::Written && private {
                    let result = (|| {
                        let service = self
                            .service
                            .as_ref()
                            .ok_or("private clipboard origin retired")?;
                        if service.selected_identity() != Some(self.selection) {
                            return Err("private clipboard origin changed".to_owned());
                        }
                        let candidate = self.selection.binding().candidate();
                        let descriptor = MainWindowPrivateClipboardDescriptor {
                            clipboard_key: key,
                            origin: self.selection,
                            content_origin: self.selection,
                            selection: selected_range,
                            closure: write
                                .provenance()
                                .expect("private write carries provenance"),
                            provenance_limits: gpui_text_input::ClipboardProvenanceLimits::new(
                                32,
                                64 * 1024,
                            )
                            .expect("bounded composer clipboard provenance capacity"),
                            source: syndic_storage::DraftPrivateClipboardSourceV1::from_candidate(
                                candidate.draft_id(),
                                candidate.session_id(),
                                candidate.candidate_generation(),
                                candidate.root(),
                            )
                            .ok_or("private clipboard candidate source is malformed")?,
                        };
                        self.private_clipboard_preparation
                            .as_ref()
                            .expect("private preparation remains")
                            .publish(
                                service,
                                descriptor,
                                self.propagated_clipboard
                                    .as_ref()
                                    .expect("active clipboard")
                                    .kind()
                                    == ClipboardKind::Cut,
                            )
                    })();
                    if let Err(error) = result {
                        self.finish_propagated_clipboard_without_cut(cx);
                        self.last_error = Some(error);
                        return;
                    }
                }
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
                        self.private_clipboard_preparation = None;
                        if deletion.selection() != expected {
                            self.input.update(cx, |input, cx| {
                                input.set_enabled(!self.startup_interaction_gated, cx)
                            });
                            self.last_error =
                                Some("composer cut selection changed before deletion".into());
                            return;
                        }
                        self.begin_cut_after_write(deletion, window, cx);
                    }
                    ClipboardCompletion::Copied | ClipboardCompletion::Cancelled => {
                        self.finish_propagated_clipboard_without_cut(cx)
                    }
                    ClipboardCompletion::WriteFailed => {
                        self.finish_propagated_clipboard_without_cut(cx);
                        self.report_clipboard_feedback(
                            MainWindowComposerClipboardFeedbackKind::Failed,
                            cx,
                        );
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
                self.report_clipboard_feedback(
                    MainWindowComposerClipboardFeedbackKind::TooLarge,
                    cx,
                );
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
        self.private_clipboard_owner.noncommit(self.selection, None);
        self.private_clipboard_preparation = None;
        if let Some(clipboard) = self.propagated_clipboard.take() {
            clipboard.cancel();
        }
        if self.is_live() || self.detached.is_some() {
            self.input.update(cx, |input, cx| {
                input.set_enabled(!self.startup_interaction_gated, cx)
            });
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
        let (flight, service) = match self.begin_flight() {
            Ok(flight) => flight,
            Err(error) => {
                self.private_clipboard_owner.noncommit(self.selection, None);
                self.last_error = Some(error);
                return;
            }
        };
        let selection = self.selection;
        let clipboard_operation = self.clipboard_operation;
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
                    this.private_clipboard_owner
                        .expire_operation(clipboard_operation);
                    this.schedule_pump(window, cx);
                    return;
                }
                if this.selection != selection
                    || this
                        .service
                        .as_ref()
                        .and_then(|service| service.selected_identity())
                        != Some(selection)
                {
                    this.private_clipboard_owner
                        .expire_operation(clipboard_operation);
                    return;
                }
                this.input.update(cx, |input, cx| {
                    input.set_enabled(!this.startup_interaction_gated, cx)
                });
                match result.and_then(|prepared| {
                    this.input
                        .update(cx, |input, input_cx| prepared.begin(input, input_cx))
                }) {
                    Ok(active) => {
                        this.private_clipboard_owner
                            .bind_cut(this.selection, active.key());
                        this.propagated_cut = Some(active);
                    }
                    Err(error) => {
                        this.private_clipboard_owner.noncommit(this.selection, None);
                        this.last_error = Some(error);
                    }
                }
                this.schedule_pump(window, cx);
            });
        })
        .detach();
    }

    fn schedule_pump(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.fail_startup_release_if_needed();
        if !self.can_pump()
            || self.scheduled
            || self.active_flight.is_some()
            || (self.last_error.is_some() && self.detached.is_none())
        {
            return;
        }
        self.scheduled = true;
        cx.defer_in(window, |this, window, cx| {
            this.scheduled = false;
            this.pump_one(window, cx);
            this.advance_startup_release(window, cx);
        });
    }

    fn begin_flight(
        &mut self,
    ) -> Result<(u64, Arc<MainWindowConversationComposerService>), String> {
        if !self.can_pump() {
            return Err("composer lifecycle is unavailable for dispatch".to_owned());
        }
        let service = self.bound_service()?;
        if self.active_flight.is_some() {
            return Err("another composer operation already owns the lifecycle lane".to_owned());
        }
        let flight = self.next_flight;
        self.next_flight = self
            .next_flight
            .checked_add(1)
            .ok_or_else(|| "composer lifecycle generation exhausted".to_owned())?;
        self.active_flight = Some(flight);
        Ok((flight, service))
    }

    fn settle_flight(&mut self, flight: u64) -> bool {
        if self.active_flight != Some(flight) {
            return false;
        }
        self.active_flight = None;
        true
    }
}
