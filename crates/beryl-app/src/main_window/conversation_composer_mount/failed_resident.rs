use super::*;
use crate::main_window::{
    MainWindowFailedComposerRetirement, MainWindowFailedResidentCapture,
    MainWindowFailedResidentTicket,
};

pub struct MainWindowFailedResidentMountResources {
    pub resident: crate::main_window::MainWindowComposerRecoveryResources,
    pub service: Option<Arc<MainWindowConversationComposerService>>,
    pub publication_adapters: Option<(beryl_state::AssetState, DraftMarkerSealService)>,
    pub configurator: Option<MainWindowConversationComposerConfigurator>,
    pub submission_source: Option<MainWindowComposerSubmissionRequestSource>,
    pub native_lineage_control: Option<crate::cas_projection::NativeLineageRecoveryControl>,
}

impl MainWindowFailedResidentMountResources {
    pub(crate) fn retire_with_marker_custody(
        mut self,
        custody: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    ) -> Result<MainWindowFailedComposerRetirement, Self> {
        if self
            .service
            .as_ref()
            .or(self.resident.service.as_ref())
            .is_none_or(|service| !service.failed_resident_marker_custody_matches(custody))
        {
            return Err(self);
        }
        match (&self.service, &self.resident.service) {
            (None, None) => return Err(self),
            (Some(mount), Some(resident)) if !Arc::ptr_eq(mount, resident) => return Err(self),
            _ => {}
        }
        let service = self
            .service
            .take()
            .or_else(|| self.resident.service.take())
            .unwrap();
        self.resident.service.take();
        self.resident.clipboard_writer.take();
        self.resident.mutation_failure.take();
        self.publication_adapters.take();
        self.configurator.take();
        self.submission_source.take();
        self.native_lineage_control.take();
        match service.retire_failed_resident_with_marker_custody(custody) {
            Ok(retired) => Ok(retired),
            Err(service) => {
                self.service = Some(service);
                Err(self)
            }
        }
    }

    pub fn retire(mut self) -> Result<MainWindowFailedComposerRetirement, Self> {
        if self
            .service
            .as_ref()
            .or(self.resident.service.as_ref())
            .is_none_or(|service| !service.failed_resident_marker_custody_is_drained())
        {
            return Err(self);
        }
        if self
            .publication_adapters
            .as_ref()
            .is_some_and(|(_, seals)| seals.diagnostics().current_flights() != 0)
        {
            return Err(self);
        }
        match (&self.service, &self.resident.service) {
            (None, None) => return Err(self),
            (Some(mount), Some(resident)) if !Arc::ptr_eq(mount, resident) => return Err(self),
            _ => {}
        }
        let service = self
            .service
            .take()
            .or_else(|| self.resident.service.take())
            .unwrap();
        self.resident.service.take();
        self.resident.clipboard_writer.take();
        self.resident.mutation_failure.take();
        self.publication_adapters.take();
        self.configurator.take();
        self.submission_source.take();
        self.native_lineage_control.take();
        match service.retire_failed_resident() {
            Ok(retired) => Ok(retired),
            Err(service) => {
                self.service = Some(service);
                Err(self)
            }
        }
    }
}

impl MainWindowConversationComposerMount {
    pub(crate) fn adopt_failed_recovery<C: Send + 'static>(
        &mut self,
        close: crate::main_window::MainWindowConversationComposerCloseTicket,
        preparation: &mut crate::main_window::MainWindowFailedResidentPreparation<C>,
        adapters: &mut Option<
            crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters,
        >,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<
        (
            C,
            crate::main_window::MainWindowFailedResidentAdoption,
            crate::main_window::MainWindowConversationComposerCloseTicket,
        ),
        String,
    > {
        if !self.failed_resident_detached
            || self.service.is_some()
            || self.configurator.is_some()
            || self.native_lineage_recovery.is_some()
            || self.failed_recovery_close.is_some()
        {
            return Err("failed resident old mount resources remain attached".into());
        }
        let (_, selection) = preparation
            .authenticated_source()
            .ok_or("failed resident source is unavailable")?;
        if !adapters.as_ref().is_some_and(|a| {
            a.matches(
                selection.binding().home_id(),
                selection.binding().home_generation(),
            )
        }) || configurator.is_none()
        {
            return Err("failed resident fresh adapters do not match".into());
        }
        let publication = self.autosave.recovery_adapters()?;
        let submission = self.submission.recovery_source()?;
        if publication.is_some() || submission.is_some() {
            return Err("failed resident adapter custody is occupied".into());
        }
        let resident = self
            .contribution
            .as_ref()
            .ok_or("failed resident editor is unavailable")?
            .clone();
        let (graph, service, adoption, fresh) = resident.update(cx, |resident, cx| {
            preparation.adopt_failed_recovery(resident, close, current, window, cx)
        })?;
        if let Some(owner) = adapters.as_ref().unwrap().private_clipboard_owner() {
            service.set_private_clipboard_owner(owner.clone());
            resident.update(cx, |composer, cx| {
                composer.attach_private_clipboard_owner(owner, cx)
            });
        }
        let (assets, marker, submission_source, native) = adapters.take().unwrap().into_parts();
        *publication = Some((assets, marker));
        *submission = Some(submission_source);
        self.service = Some(service);
        self.configurator = configurator.take();
        self.native_lineage_recovery = Some(native);
        self.failed_recovery_close = Some(fresh);
        self.window_close = Some(super::close::ActiveWindowClose {
            ticket: fresh,
            flush: None,
            state: crate::main_window::MainWindowConversationComposerCloseAdvance::Preparing,
            disposing: false,
            disposal_captured: false,
            release_requested: false,
            recovery_fenced: true,
            resources_detached: false,
            restore_enabled: None,
            #[cfg(feature = "test-faults")]
            cancel_disposal: false,
        });
        Ok((graph, adoption, fresh))
    }

    pub(crate) fn failed_recovery_close_ticket(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<crate::main_window::MainWindowConversationComposerCloseTicket, String> {
        self.begin_failed_resident(cx)?;
        if let Some(close) = self.window_close {
            return Ok(close.ticket);
        }
        let selection = self
            .contribution
            .as_ref()
            .unwrap()
            .read(cx)
            .selection_identity();
        let generation = self
            .window_close_generation
            .checked_add(1)
            .ok_or("failed recovery close identity exhausted")?;
        self.window_close_generation = generation;
        Ok(
            crate::main_window::MainWindowConversationComposerCloseTicket::for_recovery(
                cx.entity_id(),
                generation,
                selection,
            ),
        )
    }

    pub fn begin_failed_resident(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowFailedResidentTicket, String> {
        if let Some(ticket) = self.failed_resident {
            self.suspend_autosave()?;
            return Ok(ticket);
        }
        if self.pending_presentation.is_some() {
            return Err("failed resident pending target is not drained".into());
        }
        let resident = self
            .contribution
            .as_ref()
            .ok_or("failed resident editor is unavailable")?;
        let ticket = resident.update(cx, |resident, cx| resident.begin_failed_resident(cx))?;
        self.failed_resident = Some(ticket);
        self.suspend_autosave()?;
        Ok(ticket)
    }

    pub fn capture_failed_resident(
        &mut self,
        ticket: MainWindowFailedResidentTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<MainWindowFailedResidentCapture>, String> {
        self.validate_failed_mount(ticket)?;
        if !self.failed_mount_drained() {
            return Ok(None);
        }
        self.contribution
            .as_ref()
            .unwrap()
            .update(cx, |resident, cx| {
                resident.capture_failed_resident(ticket, cx)
            })
    }

    pub fn detach_failed_resident_resources(
        &mut self,
        capture: &MainWindowFailedResidentCapture,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowFailedResidentMountResources, String> {
        self.detach_failed_resident_resources_inner(capture, None, cx)
    }

    pub(crate) fn detach_failed_resident_resources_with_marker_custody(
        &mut self,
        capture: &MainWindowFailedResidentCapture,
        custody: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowFailedResidentMountResources, String> {
        self.detach_failed_resident_resources_inner(capture, Some(custody), cx)
    }

    fn detach_failed_resident_resources_inner(
        &mut self,
        capture: &MainWindowFailedResidentCapture,
        custody: Option<&crate::composer_marker_seal::DraftMarkerSealRetainedFlights>,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowFailedResidentMountResources, String> {
        self.validate_failed_mount(capture.ticket())?;
        if self.failed_resident_detached || !self.failed_mount_drained() {
            return Err("failed resident mount is detached or work is not drained".into());
        }
        self.validate_failed_native_resources()?;
        if !match custody {
            Some(custody) => self
                .bound_service()?
                .failed_resident_marker_custody_matches(custody),
            None => self
                .bound_service()?
                .failed_resident_marker_custody_is_drained(),
        } {
            return Err("failed resident original marker authority is not drained".into());
        }
        if custody.is_none()
            && self
                .autosave
                .recovery_adapters()?
                .as_ref()
                .is_some_and(|(_, seals)| seals.diagnostics().current_flights() != 0)
        {
            return Err("failed resident marker seal custody is not drained".into());
        }
        self.submission.recovery_source()?;
        let resident = self
            .contribution
            .as_ref()
            .unwrap()
            .update(cx, |resident, cx| {
                resident.detach_failed_resident_resources(capture, cx)
            })?;
        let resources = MainWindowFailedResidentMountResources {
            resident,
            service: self.service.take(),
            publication_adapters: self
                .autosave
                .detach_recovery_adapters()
                .expect("drained failed resident adapters were checked before exact transfer"),
            configurator: self.configurator.take(),
            submission_source: self.submission.detach_recovery_source().expect(
                "drained failed resident submission source was checked before exact transfer",
            ),
            native_lineage_control: self.native_lineage_recovery.take(),
        };
        self.native_lineage_refresh_task.take();
        self.failed_resident_detached = true;
        Ok(resources)
    }

    fn validate_failed_mount(&self, ticket: MainWindowFailedResidentTicket) -> Result<(), String> {
        if self.failed_resident != Some(ticket) || self.contribution.is_none() {
            return Err("failed resident mount request changed".into());
        }
        Ok(())
    }

    fn failed_mount_drained(&self) -> bool {
        self.window_close_task.is_none()
            && self.window_close_workers.retained() == 0
            && self.autosave.workers_drained()
            && self.submission.workers_drained()
            && self.native_lineage_workers.retained() == 0
            && self.pending_cleanup_workers.retained() == 0
            && self.native_disposal_workers.retained() == 0
            && self.native_lineage_disposal_task.is_none()
            && self.native_lineage_validation_task.is_none()
            && self.pending_presentation.is_none()
            && !self.submission.is_active()
            && self.native_lineage_snapshot.is_none()
            && !self.native_lineage_disposal_active
            && self.native_lineage_disposal_flush.is_none()
    }

    fn validate_failed_native_resources(&self) -> Result<(), String> {
        if self.native_lineage_config.is_some()
            || self.native_lineage_environment.is_some()
            || self.native_lineage_session.is_some()
            || self.native_lineage_candidate.is_some()
            || !self.native_lineage_effects.is_empty()
            || self.native_lineage_cleanup.is_some()
            || self.native_lineage_source.is_some()
            || self.native_lineage_host_result.is_some()
        {
            return Err("failed resident native lineage resources are not drained".into());
        }
        Ok(())
    }
}
