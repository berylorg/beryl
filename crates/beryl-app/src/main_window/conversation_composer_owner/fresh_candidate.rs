use super::*;
use crate::main_window::MainWindowConversationComposerCloseTicket;

pub(super) struct FreshRecoveryGuiPreparation {
    fenced_frame: u64,
    protection_confirmed: bool,
}

impl MainWindowConversationComposer {
    #[cfg(feature = "test-faults")]
    pub fn test_fresh_recovery_release_requests(&self) -> Option<usize> {
        self.fresh_recovery_release_requests.as_ref().map(Vec::len)
    }

    pub(in crate::main_window) fn detach_fresh_recovery(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if !close.matches_editor(self.selection)
            || self.window_close.is_some_and(|ticket| ticket != close)
        {
            return Err("fresh recovery widget close identity changed".into());
        }
        if self.service.is_none() {
            return Ok(true);
        }
        if self.active_flight.is_some()
            || self.pending_dispatch.is_some()
            || self.pending_realizer.is_some()
            || self.paste.is_some()
            || self.propagated_clipboard.is_some()
            || self.propagated_cut.is_some()
            || self.pending_marker_metadata.is_some()
            || self.mutation_evidence.is_some()
        {
            return Ok(false);
        }
        self.phase = MainWindowConversationComposerPhase::RecoveryFenced;
        self.window_close = Some(close);
        self.shutdown_interaction_gated = true;
        self.scheduled = false;
        if self.fresh_recovery_release_requests.is_none() {
            self.fresh_recovery_release_requests =
                Some(self.input.update(cx, |input, cx| input.dispose(window, cx)));
        }
        let requests = self.fresh_recovery_release_requests.as_ref().unwrap();
        self.bound_service()?
            .release_fresh_candidate_widget(self.selection, requests)?;
        self.fresh_recovery_release_requests.take();
        self.fresh_recovery_gui = None;
        self.unpublished_recovery_protection = None;
        self.activation_seeds.clear();
        self._input_subscription.take();
        self._input_event_subscription.take();
        self.service.take();
        self.clipboard_writer.take();
        self.checked_clipboard_writer.take();
        self.checked_clipboard_reader.take();
        self.admitted_positions = None;
        cx.notify();
        Ok(true)
    }

    pub(in crate::main_window) fn fence_fresh_recovery(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !close.matches_editor(self.selection)
            || self.window_close.is_some()
            || self.active_flight.is_some()
            || self.pending_dispatch.is_some()
            || !self.is_live()
            || self.is_pending_target()
        {
            return Err("fresh recovery resident is not an idle selected editor".into());
        }
        self.phase = MainWindowConversationComposerPhase::RecoveryFenced;
        self.window_close = Some(close);
        self.shutdown_interaction_gated = true;
        self.startup_interaction_gated = false;
        self.scheduled = false;
        self.fresh_recovery_gui = Some(FreshRecoveryGuiPreparation {
            fenced_frame: self
                .input
                .read(cx)
                .realization_diagnostics()
                .frame_generation,
            protection_confirmed: false,
        });
        self.input.update(cx, |input, cx| {
            input.set_read_only(true, cx);
            input.set_enabled(false, cx);
        });
        self.admitted_positions = None;
        cx.notify();
        Ok(())
    }

    pub(in crate::main_window) fn advance_fresh_recovery(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if !self.recovery_binding_current(close)
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated
            || self.bound_service()?.selected_identity() != Some(self.selection)
        {
            return Err("fresh recovery resident preparation binding changed".into());
        }
        if let Some((protected_close, protection)) = self.unpublished_recovery_protection {
            if protected_close != close {
                return Err(self.fresh_recovery_geometry_error(
                    "fresh recovery resident protection changed",
                    cx,
                ));
            }
            if !self
                .input
                .read(cx)
                .resident_protection_is_current(protection)
            {
                self.input
                    .update(cx, |input, cx| {
                        input.release_resident_protection(protection, cx)
                    })
                    .map_err(|error| {
                        format!("fresh recovery protection release was refused: {error}")
                    })?;
                self.unpublished_recovery_protection = None;
                let gui = self
                    .fresh_recovery_gui
                    .as_mut()
                    .ok_or("fresh recovery GUI preparation is missing")?;
                gui.fenced_frame = self
                    .input
                    .read(cx)
                    .realization_diagnostics()
                    .frame_generation;
                gui.protection_confirmed = false;
                cx.notify();
                return Ok(false);
            }
            self.fresh_recovery_gui
                .as_mut()
                .ok_or("fresh recovery GUI preparation is missing")?
                .protection_confirmed = true;
            return Ok(true);
        }
        for _ in 0..16 {
            let Some(request) = self.input.update(cx, |input, _| input.take_request()) else {
                break;
            };
            match &request {
                RangeTextInputRequest::CancelPage(_)
                | RangeTextInputRequest::ReleasePage(_)
                | RangeTextInputRequest::CancelObjectPage(_)
                | RangeTextInputRequest::ReleaseObjectPage(_) => continue,
                _ => {}
            }
            let seed = self
                .activation_seeds
                .iter()
                .find(|seed| match (seed, &request) {
                    (
                        MainWindowConversationComposerActivationSeed::Page(_),
                        RangeTextInputRequest::Page(_),
                    ) => true,
                    (
                        MainWindowConversationComposerActivationSeed::ObjectPage(_),
                        RangeTextInputRequest::ObjectPage(_),
                    ) => true,
                    _ => false,
                })
                .ok_or("fresh recovery widget requested content beyond its bounded preparation")?;
            let response = match seed {
                MainWindowConversationComposerActivationSeed::Page(response)
                | MainWindowConversationComposerActivationSeed::ObjectPage(response) => response,
            };
            if self
                .selection
                .binding()
                .logical_extent()
                .logical_utf8_bytes()
                != 0
                || self
                    .selection
                    .binding()
                    .root()
                    .marker_commitment()
                    .marker_count()
                    != 0
            {
                return Err(
                    "fresh first conversation widget requires its empty admitted draft".into(),
                );
            }
            let outcome = super::super::translate_initial_composer_response(
                self.selection,
                request,
                response,
            )
            .map_err(|_| "fresh recovery initial response was refused")?;
            self.input
                .update(cx, |input, cx| match outcome {
                    MainWindowComposerDispatchOutcome::Page(page) => {
                        input.deliver_page(page, window, cx)
                    }
                    MainWindowComposerDispatchOutcome::ObjectPage(page) => {
                        input.deliver_object_page_in_window(page, window, cx)
                    }
                    _ => unreachable!(),
                })
                .map_err(|error| {
                    format!("fresh recovery bounded initial response was refused: {error}")
                })?;
        }
        let gui = self
            .fresh_recovery_gui
            .as_ref()
            .ok_or("fresh recovery GUI preparation is missing")?;
        if self
            .input
            .read(cx)
            .realization_diagnostics()
            .frame_generation
            == gui.fenced_frame
            || !self.input.read(cx).is_quiescent()
        {
            cx.notify();
            return Ok(false);
        }
        self.admit_fresh_origin_positions(cx)?;
        let protection = self
            .input
            .update(cx, |input, cx| input.protect_resident(cx))
            .map_err(|error| {
                self.fresh_recovery_geometry_error(
                    &format!("fresh recovery widget protection was refused: {error:?}"),
                    cx,
                )
            })?;
        self.unpublished_recovery_protection = Some((close, protection));
        cx.notify();
        Ok(false)
    }

    fn admit_fresh_origin_positions(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        use gpui_text_input::{
            ByteOffset, ByteRange, InlineObjectGap, ObjectDemand, ObjectDemandEnvelope,
            ObjectDirection, ObjectRequestId, ObjectResidency, ObjectResidencyLimits,
            PresentationGeneration, RangeResidency, ResidencyLimits,
        };
        let binding = self.selection.binding();
        if binding.logical_extent().logical_utf8_bytes() != 0
            || binding.root().marker_commitment().marker_count() != 0
        {
            return Err("fresh origin proof requires its authenticated empty draft".into());
        }
        let response = self
            .activation_seeds
            .iter()
            .find_map(|seed| match seed {
                MainWindowConversationComposerActivationSeed::ObjectPage(response) => {
                    Some(response)
                }
                _ => None,
            })
            .ok_or("fresh origin proof requires its retained marker response")?;
        let crate::composer_host::ComposerHostResponseValue::CandidateMarkers(markers) =
            response.value()
        else {
            return Err("fresh origin proof marker response has the wrong type".into());
        };
        let result = markers.value();
        if markers.binding() != binding.candidate()
            || result.scope()
                != (syndic_storage::DraftPieceMarkerScopeV1::InclusiveRange { start: 0, end: 0 })
            || result.direction() != syndic_storage::DraftPieceMarkerDirectionV1::Forward
            || !result.markers().is_empty()
            || result.continuation().is_some()
            || !result.requested_side_complete()
        {
            return Err(
                "fresh origin proof marker response is not its exact closed empty origin".into(),
            );
        }
        let range_binding = binding.range_binding();
        let text = RangeResidency::new(
            range_binding,
            ResidencyLimits::new(1, 4096, 1, 4096)
                .map_err(|error| format!("fresh origin proof text limits: {error:?}"))?,
        );
        let mut objects = ObjectResidency::new(
            range_binding,
            PresentationGeneration::new(binding.presentation_generation().get()),
            ObjectResidencyLimits::new(1, 48, 65_536, 65_536, 1, 48, 65_536)
                .map_err(|error| format!("fresh origin proof object limits: {error:?}"))?,
        );
        let demand = ObjectDemandEnvelope::range(
            ByteRange::new(ByteOffset::new(0), ByteOffset::new(0))
                .map_err(|error| format!("fresh origin proof range: {error:?}"))?,
            None,
            ObjectDirection::Forward,
            48,
            65_536,
        )
        .map_err(|error| format!("fresh origin proof envelope: {error:?}"))?;
        let ObjectDemand::Requested(request) = objects
            .demand(
                ObjectRequestId::new(1),
                ObjectPurpose::GeometryIndex,
                demand,
            )
            .map_err(|error| format!("fresh origin proof demand: {error:?}"))?
        else {
            return Err("fresh origin proof local demand was not newly admitted".into());
        };
        let MainWindowComposerDispatchOutcome::ObjectPage(page) =
            super::super::translate_initial_composer_response(
                self.selection,
                RangeTextInputRequest::ObjectPage(request),
                response,
            )
            .map_err(|error| format!("fresh origin proof translation: {error:?}"))?
        else {
            return Err("fresh origin proof response was not an object page".into());
        };
        let anchors = text
            .prove_object_page_anchors(range_binding, &page)
            .map_err(|error| format!("fresh origin proof anchors: {error:?}"))?;
        objects
            .admit(page, anchors)
            .map_err(|error| format!("fresh origin proof admission: {error:?}"))?;
        let mut positions = Vec::with_capacity(4);
        {
            let input = self.input.read(cx);
            let surface = input
                .surface()
                .ok_or("fresh origin proof has no realized surface")?;
            if surface.binding() != range_binding {
                return Err("fresh origin proof surface binding changed".into());
            }
            for position in [
                surface.caret(),
                surface.selection().anchor,
                surface.selection().head,
                surface.scroll_position(),
            ] {
                if position.byte_offset.get() != 0 || position.gap != InlineObjectGap::NoObjects {
                    return Err(
                        "fresh origin proof surface position differs from empty origin".into(),
                    );
                }
                if !positions.contains(&position) {
                    positions.push(position);
                }
            }
        }
        self.input
            .update(cx, |input, _| {
                input.admit_edit_positions(&positions, &text, &objects)
            })
            .map_err(|error| {
                self.fresh_recovery_geometry_error(
                    &format!("fresh origin position proof was refused: {error}"),
                    cx,
                )
            })
    }

    pub(in crate::main_window) fn validate_fresh_recovery_protection(
        &self,
        close: MainWindowConversationComposerCloseTicket,
        cx: &App,
    ) -> Result<(), String> {
        let Some(gui) = self.fresh_recovery_gui.as_ref() else {
            return Ok(());
        };
        if !self.recovery_binding_current(close)
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated
            || !gui.protection_confirmed
            || match self.unpublished_recovery_protection {
                Some((protected_close, protection)) => {
                    protected_close != close
                        || !self
                            .input
                            .read(cx)
                            .resident_protection_is_current(protection)
                        || !self.input.read(cx).is_quiescent()
                }
                None => {
                    !self.input.read(cx).is_enabled()
                        || !self.input.read(cx).is_semantically_quiescent()
                }
            }
        {
            return Err(self.fresh_recovery_geometry_error(
                "fresh recovery resident lacks current rendered protection",
                cx,
            ));
        }
        Ok(())
    }

    fn fresh_recovery_geometry_error(&self, reason: &str, cx: &App) -> String {
        let input = self.input.read(cx);
        let layout = input.resident_layout_snapshot();
        let diagnostics = input.realization_diagnostics();
        format!(
            "{reason}; frame={}, wrap_width={:?}, viewport_extent={:?}, quiescent={}",
            diagnostics.frame_generation,
            layout.layout.wrap_width,
            layout.viewport_extent,
            input.is_quiescent()
        )
    }
}
