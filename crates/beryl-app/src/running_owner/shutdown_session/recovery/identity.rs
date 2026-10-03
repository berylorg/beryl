use super::*;

pub(crate) trait RecoveryIdentity {
    fn identity(&self) -> Rc<()>;
    fn lifecycle(&self) -> Option<&RunningExitRequest>;
}

impl RecoveryIdentity for RunningExitRequest {
    fn identity(&self) -> Rc<()> {
        self.identity()
    }

    fn lifecycle(&self) -> Option<&RunningExitRequest> {
        Some(self)
    }
}

pub(super) struct OrdinaryHomeRecoveryKey(pub(super) Rc<()>);

impl RecoveryIdentity for OrdinaryHomeRecoveryKey {
    fn identity(&self) -> Rc<()> {
        self.0.clone()
    }

    fn lifecycle(&self) -> Option<&RunningExitRequest> {
        None
    }
}

impl RunningProcessOwner {
    pub(super) fn active_recovery_identity(&self, identity: &Rc<()>) -> bool {
        self.interrupted_exit.as_ref().is_some_and(|recovery| {
            Rc::ptr_eq(&recovery.request, identity)
                && (recovery.ordinary || self.process.commands.is_active_identity(identity))
        })
    }

    pub(super) fn recovery_drafts(
        &self,
    ) -> Result<Rc<RefCell<super::super::super::shutdown_drafts::RunningShutdownDrafts>>, String>
    {
        self.interrupted_exit
            .as_ref()
            .and_then(|recovery| recovery.drafts.clone())
            .or_else(|| {
                self.shutdown
                    .as_ref()
                    .and_then(|attempt| attempt.drafts.clone())
            })
            .ok_or_else(|| "Home recovery drafts are unavailable".into())
    }
}
