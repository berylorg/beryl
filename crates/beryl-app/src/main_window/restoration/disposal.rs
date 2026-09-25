use super::*;

impl MainWindowRestoreSet {
    pub(super) fn dispose_step(&mut self) -> Result<bool, String> {
        if !self.discovery.settle(&self.services.store)? {
            return Ok(false);
        }
        if let Some(current) = self.current.take() {
            if let RestoredWindowComposerRetirement::Pending(failure) =
                current.retire(CommandCancellation::new())
            {
                self.current = Some(Box::new(failure.custody));
                return Err(failure.error);
            }
        }
        if let Some(unpublished) = self.retiring.take() {
            if let RestoredWindowShellRetirement::Pending { unpublished, error } =
                unpublished.retire(CommandCancellation::new())
            {
                self.retiring = Some(Box::new(unpublished));
                return Err(error);
            }
        }
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
