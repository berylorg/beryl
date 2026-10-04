use super::*;

impl MainWindowConversationComposer {
    pub(in crate::main_window) fn recovery_binding_current(
        &self,
        close: crate::main_window::MainWindowConversationComposerCloseTicket,
    ) -> bool {
        matches!(
            self.phase,
            MainWindowConversationComposerPhase::RecoveryFenced
        ) && self.recovery_snapshot.is_none()
            && self.service.is_some()
            && self.window_close == Some(close)
            && close.matches_editor(self.selection)
    }

    pub(in crate::main_window) fn appearance_applicable(&self) -> bool {
        matches!(
            self.phase,
            MainWindowConversationComposerPhase::Live
                | MainWindowConversationComposerPhase::Detached
                | MainWindowConversationComposerPhase::Fencing
        ) || (matches!(
            self.phase,
            MainWindowConversationComposerPhase::RecoveryFenced
        ) && self.recovery_snapshot.is_none()
            && self.service.is_some()
            && self.window_close.is_some())
    }

    pub(in crate::main_window) fn apply_appearance(
        &mut self,
        theme: gpui_text_input::TextInputTheme,
        scrollbar_style: gpui_scrollbar::ScrollbarStyle,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.input
            .update(cx, |input, input_cx| {
                let protected = self.unpublished_recovery_protection;
                if let Some((_, protection)) = protected {
                    if !input.resident_protection_is_current(protection) {
                        return Err(gpui_text_input::RangeTextInputError::Stale);
                    }
                    input.release_resident_protection(protection, input_cx)?;
                }
                let applied = input.set_appearance(theme, scrollbar_style, input_cx);
                if let Some((close, _)) = protected {
                    self.unpublished_recovery_protection = Some((
                        close,
                        input.protect_resident(input_cx).expect(
                            "visual appearance preserves the exact quiescent disabled resident",
                        ),
                    ));
                }
                applied
            })
            .map_err(|error| error.to_string())
    }

    pub fn gpui_input(&self) -> Entity<RangeTextInput> {
        self.input.clone()
    }

    pub const fn selection_identity(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }

    pub fn is_pending_target(&self) -> bool {
        matches!(self.route, MainWindowConversationComposerRoute::Pending(_))
    }

    pub(in crate::main_window) fn matches_pending_target(
        &self,
        receipt: MainWindowComposerActivationReceipt,
    ) -> bool {
        self.route == MainWindowConversationComposerRoute::Pending(receipt)
    }

    pub fn pending_surface_ready(&self, cx: &App) -> bool {
        self.is_pending_target()
            && self.last_error.is_none()
            && self
                .input
                .read_with(cx, |input, _| input.is_surface_current_and_interactive())
    }

    pub(in crate::main_window) fn native_lineage_restoration_ready(&self, cx: &App) -> bool {
        self.selected_first_presentable(cx)
    }

    pub fn selected_first_presentable(&self, cx: &App) -> bool {
        !self.is_pending_target()
            && self.is_live()
            && self.last_error.is_none()
            && self
                .service
                .as_ref()
                .and_then(|service| service.selected_identity())
                == Some(self.selection)
            && self
                .input
                .read_with(cx, |input, _| input.is_surface_current_and_interactive())
    }

    pub(in crate::main_window) fn focus_input(&self, window: &mut Window, cx: &mut App) {
        if self.startup_interaction_gated {
            return;
        }
        self.input.update(cx, |input, _| input.focus(window));
    }

    pub(in crate::main_window) fn admit_pending_surface(&mut self, cx: &App) -> bool {
        let ready = self.pending_surface_ready(cx);
        if ready {
            self.activation_seeds.clear();
        }
        ready
    }

    #[cfg(feature = "test-faults")]
    pub fn test_block_next_selected_dispatch(
        &self,
    ) -> super::service::MainWindowComposerPendingDispatchTestRelease {
        self.bound_service()
            .expect("test composer service is bound")
            .test_block_next_selected_dispatch()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_widget_released(&self) -> bool {
        matches!(self.phase, MainWindowConversationComposerPhase::Released(_))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_has_active_flight(&self) -> bool {
        self.active_flight.is_some() || self.pending_dispatch.is_some()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_set_terminal_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.last_error = Some(error);
        self.input
            .update(cx, |input, cx| input.set_enabled(false, cx));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_pending_seed_count(&self) -> usize {
        self.activation_seeds.len()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_has_pending_realizer(&self) -> bool {
        self.pending_realizer.as_ref().is_some_and(|realizer| {
            realizer.lifetime.upgrade().is_some() && realizer.composer.upgrade().is_some()
        })
    }

    #[cfg(feature = "test-faults")]
    pub fn test_has_pending_render_child(&self, cx: &App) -> bool {
        self.pending_realizer.as_ref().is_some_and(|realizer| {
            realizer.lifetime.upgrade().is_some()
                && realizer.composer.upgrade().is_some_and(|composer| {
                    composer.read(cx).matches_pending_target(realizer.receipt)
                })
        })
    }

    pub(in crate::main_window) const fn residency_bound(&self) -> MainWindowComposerResidencyBound {
        self.residency_bound
    }

    pub(in crate::main_window) fn residency_usage(
        &self,
        cx: &App,
    ) -> Result<MainWindowComposerResidencyUsage, String> {
        let diagnostics = self
            .input
            .read_with(cx, |input, _| input.realization_diagnostics());
        MainWindowComposerResidencyUsage::from_current(
            &diagnostics.current,
            diagnostics.max_resident_object_pages,
        )
        .ok_or_else(|| "composer residency usage overflowed".to_owned())
    }

    pub(in crate::main_window) fn attach_pending_realizer(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        pending: &Entity<MainWindowConversationComposer>,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerPendingRealizerToken, String> {
        if self.route != MainWindowConversationComposerRoute::Selected
            || !pending.read(cx).matches_pending_target(receipt)
        {
            return Err("pending composer realizer identity is stale".to_owned());
        }
        if self
            .pending_realizer
            .as_ref()
            .is_some_and(|realizer| realizer.lifetime.upgrade().is_some())
        {
            return Err("pending composer realizer is already attached".to_owned());
        }
        let lifetime = Arc::new(());
        self.pending_realizer = Some(MainWindowConversationComposerPendingRealizer {
            receipt,
            composer: pending.downgrade(),
            lifetime: Arc::downgrade(&lifetime),
        });
        cx.notify();
        Ok(MainWindowConversationComposerPendingRealizerToken {
            _lifetime: lifetime,
        })
    }

    pub(in crate::main_window) fn detach_pending_realizer(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(realizer) = self.pending_realizer.as_ref() else {
            return Ok(());
        };
        if realizer.lifetime.upgrade().is_none() {
            self.pending_realizer = None;
            cx.notify();
            return Ok(());
        }
        if realizer.receipt != receipt {
            return Err("pending composer realizer receipt is stale".to_owned());
        }
        self.pending_realizer = None;
        cx.notify();
        Ok(())
    }

    pub(in crate::main_window) fn promote_pending(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        selection: MainWindowComposerSelectionIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self
            .service
            .as_ref()
            .and_then(|service| service.selected_identity())
            != Some(selection)
        {
            return Err("pending composer promotion service selection was stale".to_owned());
        }
        self.promote_pending_resident(receipt, selection, window, cx)
    }

    pub(in crate::main_window) fn promote_claim_pending(
        &mut self,
        publication: &MainWindowComposerClaimPublication,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.claim_pending_promotion_ready(publication, cx) {
            return Err("pending composer publication presentation was stale".to_owned());
        }
        self.promote_pending_resident(publication.receipt(), publication.selection(), window, cx)
    }

    fn promote_pending_resident(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        selection: MainWindowComposerSelectionIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.route != MainWindowConversationComposerRoute::Pending(receipt) {
            return Err("pending composer promotion route was stale".to_owned());
        }
        if self.selection != selection {
            return Err("pending composer promotion selection was stale".to_owned());
        }
        if !self.pending_surface_ready(cx) {
            return Err("pending composer promotion surface was stale".to_owned());
        }
        self.activation_seeds.clear();
        self.route = MainWindowConversationComposerRoute::Selected;
        self.sync_mutation_gate(cx);
        self.input.update(cx, |input, input_cx| {
            input.set_enabled(!self.startup_interaction_gated, input_cx);
        });
        self.install_interactive_subscription(window, cx);
        self.schedule_pump(window, cx);
        Ok(())
    }

    pub fn synchronize_lifecycle_selection(
        &mut self,
        expected: MainWindowComposerSelectionIdentity,
        successor: MainWindowComposerSelectionIdentity,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.selection != expected
            || expected.binding().range_binding() != successor.binding().range_binding()
        {
            return Err("composer lifecycle selection changed its editor binding".to_owned());
        }
        self.selection = successor;
        self.image_surfaces.selection_changed(successor);
        self.input
            .update(cx, |input, _| {
                input.set_history_frontier(
                    input.history_frontier(),
                    successor.binding().range_history_frontier(),
                )
            })
            .map_err(|_| "composer input rebind was rejected".to_owned())
    }

    pub(in crate::main_window) fn restore_claim_prior_selection(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.route != MainWindowConversationComposerRoute::Selected
            || !matches!(
                self.phase,
                MainWindowConversationComposerPhase::Live
                    | MainWindowConversationComposerPhase::Fencing
            )
            || self.window_close.is_some()
            || self.selection.window_id() != selection.window_id()
            || self.selection.claim() != selection.claim()
        {
            return Err("composer claim prior presentation is unavailable".to_owned());
        }
        if self.selection == selection {
            return Ok(());
        }
        self.synchronize_lifecycle_selection(self.selection, selection, cx)
    }

    pub fn release_widget(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowComposerWidgetRelease, String> {
        let service = self.bound_service()?;
        let selection = self.selection;
        self.release_widget_with(window, cx, |requests| {
            service.release_widget_work(selection, requests)
        })
    }

    pub(in crate::main_window) fn release_native_lineage_widget(
        &mut self,
        seed: gpui_text_input::RangeRestorationSeed,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Option<MainWindowComposerWidgetRelease>, String> {
        if !self.native_lineage_release_ready(cx) {
            return Ok(None);
        }
        let service = self.bound_service()?;
        let selection = self.selection;
        let mut slot = service
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?;
        slot.begin_native_lineage_suspension(selection, seed)
            .map_err(|error| error.to_string())?;
        let release = self.release_widget_with(window, cx, |requests| {
            slot.release_selected_widget_work(selection, requests)
                .map_err(|error| error.to_string())
        });
        if let Err(error) = &release {
            slot.cancel_native_lineage_suspension(selection)
                .map_err(|rollback| format!("{error}; suspension release failed: {rollback}"))?;
        }
        release.map(Some)
    }

    pub(in crate::main_window) fn widget_release_failed(&self) -> bool {
        matches!(
            self.phase,
            MainWindowConversationComposerPhase::ReleaseFailed
        )
    }

    pub(in crate::main_window) fn claim_pending_promotion_ready(
        &self,
        publication: &MainWindowComposerClaimPublication,
        cx: &App,
    ) -> bool {
        self.route == MainWindowConversationComposerRoute::Pending(publication.receipt())
            && self.selection == publication.selection()
            && self
                .service
                .as_ref()
                .is_some_and(|service| publication.matches_service(service))
            && self.pending_surface_ready(cx)
    }

    pub(in crate::main_window) fn claim_realizer_detach_ready(
        &self,
        receipt: MainWindowComposerActivationReceipt,
    ) -> bool {
        self.pending_realizer.as_ref().is_none_or(|realizer| {
            realizer.lifetime.upgrade().is_none() || realizer.receipt == receipt
        })
    }

    pub(super) fn release_widget_with(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        settle: impl FnOnce(
            Vec<gpui_text_input::RangeTextInputRequest>,
        ) -> Result<MainWindowComposerWidgetRelease, String>,
    ) -> Result<MainWindowComposerWidgetRelease, String> {
        if let MainWindowConversationComposerPhase::Released(release) = self.phase {
            return Ok(release);
        }
        let requests = self.capture_claim_widget_requests(window, cx)?;
        match settle(requests) {
            Ok(release) => {
                self.phase = MainWindowConversationComposerPhase::Released(release);
                Ok(release)
            }
            Err(error) => {
                self.phase = MainWindowConversationComposerPhase::ReleaseFailed;
                self.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub(in crate::main_window) fn capture_claim_widget_release(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowComposerClaimWidgetWork, String> {
        if let MainWindowConversationComposerPhase::Released(release) = self.phase {
            return Ok(MainWindowComposerClaimWidgetWork::Released(release));
        }
        let requests = self.capture_claim_widget_requests(window, cx)?;
        Ok(MainWindowComposerClaimWidgetWork::Requests {
            selection: self.selection,
            requests,
        })
    }

    pub(in crate::main_window) fn accept_claim_widget_release(
        &mut self,
        release: MainWindowComposerWidgetRelease,
    ) -> Result<(), String> {
        if self.selection != release.selection()
            || !matches!(
                self.phase,
                MainWindowConversationComposerPhase::Releasing
                    | MainWindowConversationComposerPhase::Released(_)
            )
        {
            return Err("composer widget release completion was stale".to_owned());
        }
        self.phase = MainWindowConversationComposerPhase::Released(release);
        Ok(())
    }

    fn capture_claim_widget_requests(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Vec<gpui_text_input::RangeTextInputRequest>, String> {
        match self.phase {
            MainWindowConversationComposerPhase::RecoveryFenced
            | MainWindowConversationComposerPhase::Detached => {
                return Err("conversation composer is retained for recovery".to_owned());
            }
            MainWindowConversationComposerPhase::Released(_) => {
                return Err("composer widget was already released".to_owned());
            }
            MainWindowConversationComposerPhase::Fencing => {}
            MainWindowConversationComposerPhase::Live => {
                return Err("conversation composer widget must be fenced before release".to_owned());
            }
            MainWindowConversationComposerPhase::Releasing => {
                return Err("conversation composer widget release is already active".to_owned());
            }
            MainWindowConversationComposerPhase::ReleaseFailed => {
                return Err("conversation composer widget release previously failed".to_owned());
            }
        }
        if !self.widget_release_ready(cx) {
            return Err(
                "conversation composer widget release is waiting for semantic quiescence"
                    .to_owned(),
            );
        }
        self.phase = MainWindowConversationComposerPhase::Releasing;
        self.scheduled = false;
        if let Some(clipboard) = self.propagated_clipboard.take() {
            clipboard.cancel();
        }
        self.propagated_cut = None;
        self.pending_marker_metadata = None;
        self.mutation_evidence = None;
        self.pending_marker_removal = None;
        self.image_surface_attachment = None;
        self.image_surfaces.clear();
        self.admitted_positions = None;
        Ok(self
            .input
            .update(cx, |input, input_cx| input.dispose(window, input_cx)))
    }

    pub(in crate::main_window) fn is_live(&self) -> bool {
        matches!(self.phase, MainWindowConversationComposerPhase::Live)
    }

    pub(super) fn can_pump(&self) -> bool {
        matches!(
            self.phase,
            MainWindowConversationComposerPhase::Live
                | MainWindowConversationComposerPhase::Detached
                | MainWindowConversationComposerPhase::Fencing
        )
    }

    pub fn begin_widget_release_fence(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        match self.phase {
            MainWindowConversationComposerPhase::RecoveryFenced
            | MainWindowConversationComposerPhase::Detached => {
                return Err("conversation composer is retained for recovery".to_owned());
            }
            MainWindowConversationComposerPhase::Live => {
                self.phase = MainWindowConversationComposerPhase::Fencing;
                self.release_fence_requires_restoration = false;
                if let Some(clipboard) = self.propagated_clipboard.take() {
                    clipboard.cancel();
                }
                self.image_surfaces.clear();
                if let Some(attachment) = self.image_surface_attachment.take()
                    && let Err(error) = self.input.update(cx, |input, input_cx| {
                        input.dismiss_active_inline_object_surface(
                            attachment,
                            InlineObjectSurfaceDismissal::ClearObject,
                            window,
                            input_cx,
                        )
                    })
                    && !matches!(error, gpui_text_input::RangeTextInputError::Stale)
                {
                    return Err("composer marker surface dismissal was rejected".into());
                }
                self.input
                    .update(cx, |input, input_cx| input.set_enabled(false, input_cx));
                self.schedule_pump(window, cx);
            }
            MainWindowConversationComposerPhase::Fencing => {}
            MainWindowConversationComposerPhase::Releasing => {
                return Err("conversation composer widget release is already active".to_owned());
            }
            MainWindowConversationComposerPhase::Released(_) => return Ok(true),
            MainWindowConversationComposerPhase::ReleaseFailed => {
                return Err("conversation composer widget release previously failed".to_owned());
            }
        }
        Ok(self.widget_release_ready(cx))
    }

    pub(in crate::main_window) fn begin_native_lineage_release_fence(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        self.begin_widget_release_fence(window, cx)?;
        self.release_fence_requires_restoration = true;
        self.schedule_pump(window, cx);
        Ok(self.native_lineage_release_ready(cx))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_begin_native_lineage_release_fence(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        self.begin_native_lineage_release_fence(window, cx)
    }

    pub fn widget_release_ready(&self, cx: &mut Context<Self>) -> bool {
        matches!(self.phase, MainWindowConversationComposerPhase::Fencing)
            && self.active_flight.is_none()
            && self.pending_dispatch.is_none()
            && self.last_error.is_none()
            && self
                .input
                .update(cx, |input, _| input.is_semantically_quiescent())
    }

    pub(in crate::main_window) fn native_lineage_release_ready(
        &self,
        cx: &mut Context<Self>,
    ) -> bool {
        matches!(self.phase, MainWindowConversationComposerPhase::Fencing)
            && self.active_flight.is_none()
            && self.pending_dispatch.is_none()
            && self.last_error.is_none()
            && self.input.update(cx, |input, _| input.is_quiescent())
            && self.input.update(cx, |input, _| {
                input
                    .surface()
                    .is_some_and(|surface| surface.composition().is_none())
            })
    }

    pub(in crate::main_window) fn export_native_lineage_restoration(
        &self,
        cx: &mut Context<Self>,
    ) -> Result<RangeRestorationSeed, String> {
        if !self.native_lineage_release_ready(cx) {
            return Err(
                "conversation composer restoration is waiting for full quiescence".to_owned(),
            );
        }
        self.input
            .update(cx, |input, _| {
                input.export_restoration(Some(self.selection.binding().range_history_frontier()))
            })
            .map_err(|error| {
                format!("conversation composer restoration export was rejected: {error:?}")
            })
    }

    pub fn resume_after_widget_release_fence(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.startup_release_started {
            return Err("startup composer release cannot resume interaction".to_owned());
        }
        if !matches!(self.phase, MainWindowConversationComposerPhase::Fencing) {
            return Err("conversation composer widget is not fenced".to_owned());
        }
        self.phase = MainWindowConversationComposerPhase::Live;
        self.release_fence_requires_restoration = false;
        self.input.update(cx, |input, input_cx| {
            input.set_enabled(!self.startup_interaction_gated, input_cx)
        });
        self.schedule_pump(window, cx);
        Ok(())
    }

    pub(in crate::main_window) fn resume_retired_claim_prior(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.route != MainWindowConversationComposerRoute::Selected
            || self.window_close.is_some()
        {
            return Err("composer claim prior presentation is unavailable".to_owned());
        }
        match self.phase {
            MainWindowConversationComposerPhase::Live => Ok(()),
            MainWindowConversationComposerPhase::Fencing => {
                self.resume_after_widget_release_fence(window, cx)
            }
            _ => Err("composer claim prior widget cannot resume".to_owned()),
        }
    }
}
