use super::*;
use crate::main_window::{MainWindowComposerSubmissionRequestSource, MainWindowCreationServices};

impl MainWindowFirstConversationPreparation {
    pub(crate) fn home_failed(&self) -> bool {
        self.candidate.store.health().state() == HomeHealthState::Failed
    }
    pub(crate) fn prepare_mount(
        &mut self,
        services: &MainWindowCreationServices,
        cancellation: &CommandCancellation,
    ) -> Result<bool, String> {
        if self.mounted || self.published || self.candidate.retirement_started {
            return Err("first conversation preparation has transferred or retired".into());
        }
        if self.mount_inputs.is_some() {
            self.revalidate_publication()?;
            return Ok(true);
        }
        if cancellation.is_cancelled() {
            return Err("first conversation preparation cancelled".into());
        }
        match self.advance(cancellation)? {
            MainWindowInitialComposerProgress::Activated => {}
            MainWindowInitialComposerProgress::Pending
            | MainWindowInitialComposerProgress::Retry => {
                return Ok(false);
            }
        }
        let mut configure = (services.configurator_source)();
        let prepared = self.prepare(&mut configure)?;
        self.mount_inputs = Some((
            prepared,
            configure,
            services.marker_seals.clone(),
            MainWindowComposerSubmissionRequestSource::new(
                services.submission_execution.clone(),
                services.turn_start_requirement,
            ),
        ));
        self.revalidate_publication()?;
        Ok(true)
    }

    pub(crate) fn revalidate_publication(&mut self) -> Result<(), String> {
        self.publication_ready = false;
        self.validate_source()?;
        if !self.candidate.activated
            || !self.candidate.preparation_started
            || self.candidate.retirement_started
            || self.candidate.open_terminal
            || self.candidate.open_reconciliation.is_some()
            || self.candidate.service.is_none()
        {
            return Err("first conversation is not ready for publication".into());
        }
        self.publication_ready = true;
        Ok(())
    }

    pub(crate) fn publication_current(&self) -> bool {
        self.publication_ready
            && !self.published
            && !self.candidate.retirement_started
            && self.candidate.store.health().state() == HomeHealthState::Healthy
            && self.candidate.store.health().generation() == Some(self.candidate.home_generation)
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.mount_inputs.is_some() && self.publication_current()
    }

    pub(in crate::main_window) fn take_mount_inputs(
        &mut self,
    ) -> Result<FirstConversationMountInputs, String> {
        if !self.publication_current() || self.mounted {
            return Err("first conversation mount preparation is unavailable".into());
        }
        let inputs = self
            .mount_inputs
            .take()
            .ok_or("first conversation mount inputs are missing")?;
        self.mounted = true;
        Ok(inputs)
    }

    pub(crate) fn selection(&self) -> Option<MainWindowComposerSelectionIdentity> {
        self.candidate
            .service
            .as_ref()
            .and_then(|service| service.selected_identity())
    }

    pub(crate) fn service(&self) -> Option<Arc<MainWindowConversationComposerService>> {
        self.candidate.service.clone()
    }

    pub(in crate::main_window) fn mark_mount_released(&mut self) {
        self.mounted = false;
        self.publication_ready = false;
    }

    pub(in crate::main_window) fn mark_published(&mut self) -> Result<(), String> {
        if !self.publication_current() || !self.mounted {
            return Err("first conversation publication is unavailable".into());
        }
        self.mark_published_at_coherent_boundary()
    }

    pub(in crate::main_window) fn mark_published_at_coherent_boundary(
        &mut self,
    ) -> Result<(), String> {
        if !self.publication_ready
            || self.published
            || self.candidate.retirement_started
            || !self.mounted
        {
            return Err("first conversation coherent publication is unavailable".into());
        }
        let service = self
            .candidate
            .service
            .as_ref()
            .ok_or("first conversation service is missing")?
            .clone();
        self.candidate.release_recovery_service(&service)?;
        self.published = true;
        self.mounted = false;
        Ok(())
    }

    pub(crate) fn is_published(&self) -> bool {
        self.published
    }

    pub(crate) fn retire_healthy(&mut self) -> Result<bool, String> {
        if self.published {
            return Ok(true);
        }
        self.publication_ready = false;
        self.mount_inputs.take();
        if self.mounted {
            return Err("first conversation mounted presentation has not retired".into());
        }
        self.candidate.retirement_started = true;
        super::super::fresh_candidate::retire_runtime(&mut self.candidate)?;
        self.candidate.drive_retirement(CommandCancellation::new())
    }
}
