use super::*;

impl MainWindowConversationComposerMount {
    pub(in crate::main_window::conversation_composer_mount) fn recovery_fenced(&self) -> bool {
        self.window_close.is_some_and(|close| close.recovery_fenced)
    }

    pub fn detach_interrupted_exit_native_lineage_control(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<crate::cas_projection::NativeLineageRecoveryControl>, String> {
        self.validate_recovery_adapter_detachment(ticket, cx)?;
        self.native_lineage_refresh_task.take();
        Ok(self.native_lineage_recovery.take())
    }

    pub(in crate::main_window) fn bound_service(
        &self,
    ) -> Result<&Arc<MainWindowConversationComposerService>, String> {
        self.service.as_ref().ok_or_else(|| {
            "conversation composer mount service is detached for recovery".to_owned()
        })
    }

    pub fn detach_interrupted_exit_service(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<Arc<MainWindowConversationComposerService>>, String> {
        self.validate_recovery_adapter_detachment(ticket, cx)?;
        if self.native_lineage_config.is_some()
            || self.native_lineage_environment.is_some()
            || self.native_lineage_session.is_some()
            || self.native_lineage_candidate.is_some()
            || !self.native_lineage_effects.is_empty()
            || self.native_lineage_cleanup.is_some()
            || self.native_lineage_source.is_some()
            || self.native_lineage_refresh_task.is_some()
        {
            return Err("mount native lineage resources are not drained".to_owned());
        }
        Ok(self.service.take())
    }

    #[cfg(feature = "test-faults")]
    pub fn test_native_disposal_worker(
        &self,
        run: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let service = self
            .bound_service()
            .expect("test requires a bound mount")
            .clone();
        Box::pin(self.native_disposal_workers.track_future(async move {
            let _service = service;
            run.await;
        }))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_native_disposal_retained_workers(&self) -> usize {
        self.native_disposal_workers.retained()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_pending_cleanup_worker(
        &self,
        run: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let service = self
            .bound_service()
            .expect("test requires a bound mount")
            .clone();
        Box::pin(self.pending_cleanup_workers.track_future(async move {
            let _service = service;
            run.await;
        }))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_pending_cleanup_retained_workers(&self) -> usize {
        self.pending_cleanup_workers.retained()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_native_lineage_worker(
        &self,
        run: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let service = self
            .bound_service()
            .expect("test requires a bound mount")
            .clone();
        Box::pin(self.native_lineage_workers.track_future(async move {
            let _service = service;
            run.await;
        }))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_native_lineage_retained_workers(&self) -> usize {
        self.native_lineage_workers.retained()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_window_close_cleanup(
        &self,
        run: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let service = self
            .bound_service()
            .expect("test requires a bound mount")
            .clone();
        Box::pin(self.window_close_workers.track_future(async move {
            let _service = service;
            run.await;
        }))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_window_close_completion(
        &self,
        run: impl FnOnce() + Send + 'static,
    ) -> Box<dyn FnOnce() + Send> {
        let service = self
            .bound_service()
            .expect("test requires a bound mount")
            .clone();
        let completion = self.window_close_workers.track(move |()| {
            let _service = service;
            run();
        });
        Box::new(move || completion.run_with(()))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_window_close_worker(
        &self,
        run: impl FnOnce() + Send + 'static,
    ) -> Result<Box<dyn FnOnce() + Send>, String> {
        let resources = (
            self.bound_service()?.clone(),
            self.submission_assets()?,
            self.submission_marker_seals()?,
        );
        let worker = self.window_close_workers.track(move || {
            let _resources = resources;
            run();
        });
        Ok(Box::new(move || worker.run()))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_window_close_retained_workers(&self) -> usize {
        self.window_close_workers.retained()
    }

    pub fn detach_interrupted_exit_submission_source(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<MainWindowComposerSubmissionRequestSource>, String> {
        self.validate_recovery_adapter_detachment(ticket, cx)?;
        self.submission.detach_recovery_source()
    }

    pub fn detach_interrupted_exit_configurator(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<MainWindowConversationComposerConfigurator>, String> {
        self.validate_recovery_adapter_detachment(ticket, cx)?;
        Ok(self.configurator.take())
    }

    pub(in crate::main_window) fn configure_selection(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<MainWindowConversationComposerConfig, String> {
        let configurator = self.configurator.as_mut().ok_or_else(|| {
            "conversation composer configurator is detached for recovery".to_owned()
        })?;
        configurator(selection)
    }

    pub fn detach_interrupted_exit_publication_adapters(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<(beryl_state::AssetState, DraftMarkerSealService)>, String> {
        self.validate_recovery_adapter_detachment(ticket, cx)?;
        self.autosave.detach_recovery_adapters()
    }

    fn validate_recovery_adapter_detachment(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &Context<Self>,
    ) -> Result<(), String> {
        let close = self
            .window_close
            .filter(|close| {
                close.ticket == ticket && ticket.owner == cx.entity_id() && close.recovery_fenced
            })
            .ok_or_else(|| {
                "mount adapter detachment requires the exact recovery fence".to_owned()
            })?;
        if close.state != MainWindowConversationComposerCloseAdvance::Ready
            || close.disposing
            || close.release_requested
            || self.window_close_task.is_some()
            || self.window_close_workers.retained() != 0
            || !self.autosave.workers_drained()
            || !self.submission.workers_drained()
            || self.submission.is_active()
            || self.pending_presentation.is_some()
            || self.native_lineage_snapshot.is_some()
            || self.native_lineage_validation_task.is_some()
            || self.native_lineage_workers.retained() != 0
            || self.pending_cleanup_workers.retained() != 0
            || self.native_disposal_workers.retained() != 0
            || self.native_lineage_disposal_active
            || self.native_lineage_disposal_task.is_some()
            || self.native_lineage_disposal_flush.is_some()
        {
            return Err("mount adapter work is not drained".to_owned());
        }
        let resident = self
            .contribution
            .as_ref()
            .ok_or_else(|| "mount adapter recovery lost its editor".to_owned())?;
        if !resident
            .read(cx)
            .recovery_snapshot()
            .is_some_and(|snapshot| {
                snapshot.close_ticket() == ticket && Some(snapshot.flush_ticket()) == close.flush
            })
        {
            return Err("mount adapter recovery lost its resident proof".to_owned());
        }
        Ok(())
    }

    pub fn fence_interrupted_exit_resident(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let close = self
            .window_close
            .filter(|close| close.ticket == ticket && ticket.owner == cx.entity_id())
            .ok_or_else(|| "resident recovery close ticket is stale".to_owned())?;
        if close.state != MainWindowConversationComposerCloseAdvance::Ready
            || close.disposing
            || close.release_requested
            || self.window_close_task.is_some()
            || self.submission.is_active()
            || self.pending_presentation.is_some()
            || self.native_lineage_snapshot.is_some()
            || self.native_lineage_disposal_active
        {
            return Ok(false);
        }
        let flush = close
            .flush
            .ok_or_else(|| "resident recovery lost its flush ticket".to_owned())?;
        let resident = self
            .contribution
            .clone()
            .ok_or_else(|| "resident recovery lost its editor".to_owned())?;
        if !resident.update(cx, |resident, cx| {
            resident.fence_clean_recovery(ticket, flush, cx)
        })? {
            return Ok(false);
        }
        self.window_close.as_mut().unwrap().recovery_fenced = true;
        cx.notify();
        Ok(true)
    }
}
