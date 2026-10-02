use super::*;

impl MainWindowRestoreSet {
    pub(super) fn dispose_step(&mut self) -> Result<bool, String> {
        if !self.discovery.settle(&self.services.store)? {
            return Ok(false);
        }
        self.dispose_current()?;
        self.dispose_unpublished()?;
        if !self.dispose_replacement()? {
            return Ok(false);
        }
        self.dispose_member()
    }

    #[inline(never)]
    fn dispose_current(&mut self) -> Result<(), String> {
        if let Some(current) = self.current.as_mut() {
            if !current.drive_retirement(CommandCancellation::new())? {
                return Err("restored editor retirement remains pending".to_owned());
            }
            self.current.take();
        }
        Ok(())
    }

    #[inline(never)]
    fn dispose_unpublished(&mut self) -> Result<(), String> {
        if let Some(unpublished) = self.retiring.take() {
            if let RestoredWindowShellRetirement::Pending { unpublished, error } =
                unpublished.retire(CommandCancellation::new())
            {
                self.retiring = Some(Box::new(unpublished));
                return Err(error);
            }
        }
        Ok(())
    }

    #[inline(never)]
    fn dispose_replacement(&mut self) -> Result<bool, String> {
        if let Some(work) = self.replacement.take() {
            work.cancellation().cancel();
            match work.advance(self.appearance.clone()) {
                MainWindowCreationOutcome::Pending(work) => {
                    self.replacement = Some(Box::new(work));
                    return Ok(false);
                }
                MainWindowCreationOutcome::Settled { .. } => {}
                MainWindowCreationOutcome::Prepared { prepared, .. } => {
                    self.replacement = Some(Box::new(MainWindowCreation::abandon(
                        self.services.clone(),
                        prepared.into_unpublished(),
                        "startup disposal".to_owned(),
                    )));
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    #[inline(never)]
    fn dispose_member(&mut self) -> Result<bool, String> {
        if let Some(member) = self.members.pop() {
            match member {
                PreparedRestoreSetMember::Restored(prepared) => {
                    self.retiring = Some(Box::new(prepared.into_unpublished()));
                }
                PreparedRestoreSetMember::Threadless(prepared) => drop(prepared),
                PreparedRestoreSetMember::Replacement(prepared) => {
                    self.replacement = Some(Box::new(MainWindowCreation::abandon(
                        self.services.clone(),
                        prepared.into_unpublished(),
                        "startup disposal".to_owned(),
                    )));
                }
            }
            return Ok(false);
        }
        self.cleanup_error = None;
        Ok(true)
    }
}
