use std::sync::Arc;

use beryl_home_store::CommandCancellation;
use gpui::{Context, Window};
use gpui_text_input::{MutationPositions, SourceRange};

use super::super::{MainWindowComposerSelectionIdentity, MainWindowConversationComposer};

mod preparation;
mod producer;
mod replay;

pub(super) use producer::ActiveComposerPasteProducer;

use super::resources::PasteQueuePermit;

pub(in super::super) struct CapturedComposerPaste {
    pub(in super::super) selection: MainWindowComposerSelectionIdentity,
    positions: MutationPositions,
    replacement: SourceRange,
    pub(in super::super) cancellation: CommandCancellation,
    pub(in super::super) producer: Option<ActiveComposerPasteProducer>,
    pub(in super::super) key: Option<gpui_text_input::MutationKey>,
    pub(in super::super) admitted: bool,
    snapshot_sequence: Option<u32>,
    pub(in super::super) origin: Option<crate::composer_host::ComposerHostPrivatePasteOrigin>,
    _permit: Arc<PasteQueuePermit>,
}

pub(super) enum PasteSource {
    Text(String),
    Image(gpui::Image),
    Private {
        text: String,
        token: String,
        descriptor: super::source::MainWindowPrivateClipboardDescriptor,
        storage: syndic_storage::SyndicStorage,
    },
}

impl MainWindowConversationComposer {
    pub fn paste_pending(&self) -> bool {
        self.paste.is_some()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_paste_durable_begin(&self) -> bool {
        self.paste.as_ref().is_some_and(|paste| paste.admitted)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_set_checked_clipboard_reader(
        &mut self,
        reader: super::super::ComposerCheckedClipboardReader,
    ) {
        self.checked_clipboard_reader = Some(reader);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_begin_captured_paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_captured_paste(window, cx);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_cancel_captured_paste(&mut self, cx: &mut Context<Self>) {
        self.cancel_captured_paste(cx);
    }

    pub(in super::super) fn cancel_captured_paste(&mut self, cx: &mut Context<Self>) {
        let Some(paste) = self.paste.as_ref() else {
            return;
        };
        if paste.admitted || self.mutation_admission_unavailable() {
            return;
        }
        paste.cancellation.cancel();
        if let Some(key) = paste.key {
            let _ = self
                .input
                .update(cx, |input, cx| input.cancel_mutation(key, cx));
        } else {
            paste.cancellation.cancel();
        }
        cx.notify();
    }

    pub(in super::super) fn begin_captured_paste(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_live()
            || self.mutation_gated()
            || self.active_flight.is_some()
            || self.propagated_clipboard.is_some()
            || self.propagated_cut.is_some()
        {
            return;
        }
        self.clipboard_operation = super::collection::next_operation();
        self.clipboard_feedback = None;
        self.mutation_feedback = None;
        let Some(permit) = self.paste_queue.reserve() else {
            self.report_paste_feedback(
                super::super::MainWindowComposerClipboardFeedbackKind::CapacityUnavailable,
                cx,
            );
            return;
        };
        let captured = self.input.update(cx, |input, _| {
            let surface = input.surface()?;
            if surface.binding() != self.selection.binding().range_binding()
                || !input.is_surface_current_and_interactive()
            {
                return None;
            }
            let range = surface.selection().range().ok()?;
            Some((
                MutationPositions::new(
                    surface.caret(),
                    surface.selection().anchor,
                    surface.selection().head,
                ),
                range,
            ))
        });
        let Some((positions, replacement)) = captured else {
            return;
        };
        let cancellation = CommandCancellation::new();
        self.paste = Some(CapturedComposerPaste {
            selection: self.selection,
            positions,
            replacement,
            cancellation: cancellation.clone(),
            producer: None,
            key: None,
            admitted: false,
            snapshot_sequence: None,
            origin: None,
            _permit: Arc::new(permit),
        });
        self.sync_mutation_gate(cx);
        cx.notify();
        let allowance = self.clipboard_limits.max_bytes();
        let limits = gpui::ClipboardLimits {
            total_bytes: allowance,
            text_bytes: allowance,
            metadata_bytes: allowance,
            image_bytes: allowance,
        };
        let observation = self.private_clipboard_owner.observation_token();
        let snapshot = match self.checked_clipboard_reader.as_mut() {
            Some(reader) => reader(limits, cx),
            None => cx.read_from_clipboard_checked(limits),
        };
        let source = snapshot
            .map_err(|error| match error {
                gpui::ClipboardError::OverLimit => {
                    super::super::MainWindowComposerClipboardFeedbackKind::TooLarge
                }
                _ => super::super::MainWindowComposerClipboardFeedbackKind::Unavailable,
            })
            .and_then(|snapshot| self.capture_paste_source(snapshot, observation.as_deref()));
        let source = match source {
            Ok(source) => source,
            Err(kind) => {
                self.finish_paste_preparation(Some(kind), cx);
                return;
            }
        };
        let (flight, service) = match self.begin_flight() {
            Ok(value) => value,
            Err(_) => {
                self.finish_paste_preparation(
                    Some(super::super::MainWindowComposerClipboardFeedbackKind::Unavailable),
                    cx,
                );
                return;
            }
        };
        let selection = self.selection;
        let proof_limits = self.proof_limits;
        let mutation_limits = self.mutation_limits;
        let sidecar_page_bytes = self.paste_queue.resources().sidecar_page_bytes();
        let private_owner = self.private_clipboard_owner.clone();
        let permit = self.paste.as_ref().unwrap()._permit.clone();
        #[cfg(feature = "test-faults")]
        let image = matches!(&source, PasteSource::Image(_));
        let task = cx.background_executor().spawn(async move {
            #[cfg(feature = "test-faults")]
            if let Some(gate) = service.take_test_cut_preparation_gate() {
                gate.await;
            }
            let result = preparation::prepare(
                &service,
                selection,
                positions,
                replacement,
                source,
                proof_limits,
                mutation_limits,
                allowance,
                sidecar_page_bytes,
                cancellation,
            );
            if let Err(preparation::PastePreparationError::PrivateCorrelation(token)) = &result {
                private_owner.observe_clipboard_metadata(token, None);
            }
            #[cfg(feature = "test-faults")]
            if image
                && result.is_ok()
                && let Some(gate) = service.take_test_paste_asset_completion_gate()
            {
                gate.await;
            }
            (result, permit)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (result, _permit) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.settle_flight(flight) { return; }
                let valid = this.is_live() && this.selection == selection
                    && this.service.as_ref().and_then(|service| service.selected_identity()) == Some(selection)
                    && this.paste.as_ref().is_some_and(|paste| !paste.cancellation.is_cancelled()
                        && paste.positions == positions && paste.replacement == replacement && paste.snapshot_sequence.is_some())
                    && this.input.read(cx).surface().is_some_and(|surface| {
                        surface.binding() == selection.binding().range_binding()
                            && surface.selection().anchor == positions.selection_anchor()
                            && surface.selection().head == positions.selection_head()
                            && surface.caret() == positions.caret()
                    });
                if !valid {
                    this.finish_paste_preparation(None, cx);
                    this.schedule_pump(window, cx);
                    return;
                }
                match result {
                    Ok(prepared) => {
                        this.input.update(cx, |input, cx| input.set_read_only(false, cx));
                        let started = this.input.update(cx, |input, cx| prepared.begin(input, cx));
                        match started {
                            Ok((mut producer, private)) => {
                                producer.retain_permit(this.paste.as_ref().unwrap()._permit.clone());
                                let key = producer.key();
                                let cancellation = this.paste.as_ref().unwrap().cancellation.clone();
                                let origin = private.map(|(token, descriptor)| this.private_clipboard_owner.paste_origin(token, descriptor, key, cancellation));
                                let paste = this.paste.as_mut().unwrap();
                                paste.producer = Some(producer);
                                paste.key = Some(key);
                                paste.origin = origin;
                                this.sync_mutation_gate(cx);
                            }
                            Err(_) => this.finish_paste_preparation(Some(super::super::MainWindowComposerClipboardFeedbackKind::Unavailable), cx),
                        }
                    }
                    Err(preparation::PastePreparationError::Cancelled | preparation::PastePreparationError::Image(crate::composer_host::paste_image::ClipboardImageAdmissionError::Cancelled)) => this.finish_paste_preparation(None, cx),
                    Err(preparation::PastePreparationError::TooLarge) => this.finish_paste_preparation(Some(super::super::MainWindowComposerClipboardFeedbackKind::TooLarge), cx),
                    Err(preparation::PastePreparationError::PrivateCorrelation(token)) => {
                        this.private_clipboard_owner.observe_clipboard_metadata(&token, None);
                        this.finish_paste_preparation(Some(super::super::MainWindowComposerClipboardFeedbackKind::Unavailable), cx);
                    }
                    Err(preparation::PastePreparationError::Image(error)) => {
                        use crate::composer_host::paste_image::ClipboardImageAdmissionError as Error;
                        let feedback = match error {
                            Error::TooLarge => super::super::MainWindowComposerClipboardFeedbackKind::TooLarge,
                            Error::Unsupported => super::super::MainWindowComposerClipboardFeedbackKind::Unavailable,
                            Error::Sidecar(beryl_home_store::SidecarError::BoundExceeded { .. }) => super::super::MainWindowComposerClipboardFeedbackKind::TooLarge,
                            Error::Sidecar(beryl_home_store::SidecarError::Source { .. } | beryl_home_store::SidecarError::LengthMismatch { .. }) => super::super::MainWindowComposerClipboardFeedbackKind::Unavailable,
                            Error::Storage | Error::Sidecar(_) | Error::Metadata(_) => super::super::MainWindowComposerClipboardFeedbackKind::StorageUnavailable,
                            Error::Cancelled => unreachable!(),
                        };
                        this.finish_paste_preparation(Some(feedback), cx);
                    }
                    Err(error) => {
                        let _ = error;
                        this.finish_paste_preparation(Some(super::super::MainWindowComposerClipboardFeedbackKind::Unavailable), cx);
                    }
                }
                this.schedule_pump(window, cx);
            });
        }).detach();
    }

    fn capture_paste_source(
        &mut self,
        snapshot: gpui::CheckedClipboardSnapshot,
        observation: Option<&str>,
    ) -> Result<PasteSource, super::super::MainWindowComposerClipboardFeedbackKind> {
        if let Some(paste) = self.paste.as_mut() {
            paste.snapshot_sequence = Some(snapshot.sequence);
        }
        let metadata = snapshot.item.metadata().cloned();
        if let Some(expected) = observation {
            self.private_clipboard_owner
                .observe_clipboard_metadata(expected, metadata.as_deref());
        }
        let private = metadata
            .as_deref()
            .filter(|value| value.starts_with("beryl.private-composer"));
        let descriptor = private
            .map(|token| {
                self.private_clipboard_owner
                    .capture_paste(token)
                    .ok_or(super::super::MainWindowComposerClipboardFeedbackKind::Unavailable)
            })
            .transpose()?;
        let mut entries = snapshot.item.into_entries();
        let entry = entries
            .next()
            .ok_or(super::super::MainWindowComposerClipboardFeedbackKind::Unavailable)?;
        if entries.next().is_some() {
            return Err(super::super::MainWindowComposerClipboardFeedbackKind::Unavailable);
        }
        match entry {
            gpui::ClipboardEntry::String(text) => match descriptor {
                Some((descriptor, storage)) => {
                    if descriptor.origin.binding().home_id() != self.selection.binding().home_id()
                        || descriptor.origin.binding().home_generation()
                            != self.selection.binding().home_generation()
                    {
                        return Err(
                            super::super::MainWindowComposerClipboardFeedbackKind::Unavailable,
                        );
                    }
                    Ok(PasteSource::Private {
                        text: text.into_text(),
                        token: metadata.unwrap(),
                        descriptor,
                        storage,
                    })
                }
                None => Ok(PasteSource::Text(text.into_text())),
            },
            gpui::ClipboardEntry::Image(image) if descriptor.is_none() => {
                Ok(PasteSource::Image(image))
            }
            _ => Err(super::super::MainWindowComposerClipboardFeedbackKind::Unavailable),
        }
    }

    fn finish_paste_preparation(
        &mut self,
        feedback: Option<super::super::MainWindowComposerClipboardFeedbackKind>,
        cx: &mut Context<Self>,
    ) {
        self.paste = None;
        self.sync_mutation_gate(cx);
        if let Some(kind) = feedback {
            self.report_paste_feedback(kind, cx);
        }
        cx.notify();
    }

    fn report_paste_feedback(
        &mut self,
        kind: super::super::MainWindowComposerClipboardFeedbackKind,
        cx: &mut Context<Self>,
    ) {
        self.report_clipboard_feedback(kind, cx);
        if let Some(feedback) = self.clipboard_feedback.as_mut() {
            feedback.paste = true;
        }
    }

    pub(in super::super) fn clear_captured_paste(
        &mut self,
        key: gpui_text_input::MutationKey,
        cx: &mut Context<Self>,
    ) {
        if self
            .paste
            .as_ref()
            .is_some_and(|paste| paste.key == Some(key))
        {
            self.paste = None;
            self.sync_mutation_gate(cx);
            cx.notify();
        }
    }

    pub(in super::super) fn dispatch_paste_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (flight, service) = self.begin_flight()?;
        let selection = self.selection;
        let paste = self
            .paste
            .as_mut()
            .ok_or("captured paste owner disappeared")?;
        let mut producer = paste
            .producer
            .take()
            .ok_or("captured paste producer disappeared")?;
        let key = producer.key();
        #[cfg(feature = "test-faults")]
        let staging = producer.is_staging();
        let task = cx.background_executor().spawn(async move {
            #[cfg(feature = "test-faults")]
            if staging && let Some(gate) = service.take_test_paste_staging_gate() {
                gate.await;
            }
            let result = producer.load_next(&service, selection);
            (producer, result)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (producer, result) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.settle_flight(flight) { return; }
                if let Some(paste) = this.paste.as_mut().filter(|paste| paste.key == Some(key) && paste.selection == selection) {
                    paste.producer = Some(producer);
                    if let Err(error) = result {
                        if paste.admitted {
                            this.mutation_feedback = Some(super::super::MainWindowComposerMutationFeedback {
                                selection, key, kind: super::super::MainWindowComposerMutationFeedbackKind::AdmittedWorkUnavailable,
                            });
                            this.last_error = Some(error);
                            cx.notify();
                        } else {
                            this.cancel_captured_paste(cx);
                            this.report_paste_feedback(super::super::MainWindowComposerClipboardFeedbackKind::Unavailable, cx);
                        }
                    }
                }
                this.schedule_pump(window, cx);
            });
        }).detach();
        Ok(())
    }

    pub(in super::super) fn submit_captured_paste_page(
        &mut self,
        key: gpui_text_input::MutationKey,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(producer) = self
            .paste
            .as_mut()
            .and_then(|paste| paste.producer.as_mut())
            .filter(|producer| producer.key() == key)
        else {
            return Ok(());
        };
        if producer.needs_page() {
            return Ok(());
        }
        self.pending_marker_metadata = Some((key, producer.metadata()));
        self.input
            .update(cx, |input, cx| producer.submit_next(input, cx))
    }
}
