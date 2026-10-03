use super::*;

impl RunningProcessOwner {
    pub(crate) fn retire_interrupted_exit_residents(
        &mut self,
        request: &impl RecoveryIdentity,
        app: &mut App,
    ) -> Result<bool, String> {
        let recovery = self
            .interrupted_exit
            .as_ref()
            .ok_or("No reported failed Exit")?;
        if !Rc::ptr_eq(&recovery.request, &request.identity())
            || !self.active_recovery_identity(&request.identity())
        {
            return Err("Interrupted Exit request changed".into());
        }
        if recovery.session.borrow().is_none()
            || recovery.settlement.borrow().is_some()
            || recovery.resident.is_some()
            || self.process.services.is_none()
        {
            return Err("Interrupted Exit retirement custody is unavailable".into());
        }
        if !recovery.ordinary {
            let attempt = self
                .shutdown
                .as_ref()
                .ok_or("No retained shutdown attempt")?;
            if !attempt.work_ready
                || !matches!(attempt.session, Some(RunningShutdownSession::RecoveryOwned))
            {
                return Err("Interrupted Exit has not retained session-publication custody".into());
            }
        }
        let drafts = self.recovery_drafts()?;
        let mut drafts = drafts
            .try_borrow_mut()
            .map_err(|_| "Shutdown drafts are being updated")?;
        drafts.require_complete_capture()?;
        if drafts.recovery_residents() != recovery.residents {
            return Err("Interrupted Exit draft set changed or is not ready".into());
        }
        if recovery.ordinary || drafts.has_failed_residents() {
            let services = self
                .process
                .services
                .as_mut()
                .ok_or("failed service owner is unavailable")?;
            services.capture_failed_markers()?;
            drafts.retire_failed_residents(services, app)
        } else {
            drafts.retire_residents(app)
        }
    }
}
