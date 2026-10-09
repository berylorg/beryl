use super::*;

impl MainWindowCompletedThreadSuccessorCleanup {
    pub(crate) fn receipt(&self) -> MainWindowComposerActivationReceipt {
        self.retired.receipt
    }
    pub(crate) fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.retired.selection
    }
    pub(crate) fn settle_current(
        self,
        store: &HomeStore,
        storage: &SyndicStorage,
    ) -> Result<MainWindowCompletedThreadSuccessorProgress, (Self, String)> {
        let Self {
            retired:
                RetiredThreadSuccessor {
                    receipt,
                    selection,
                    host,
                },
        } = self;
        match (*host).settle_current(store, storage) {
            Ok(host) => Ok(MainWindowCompletedThreadSuccessorProgress {
                receipt,
                selection,
                host: Box::new(host),
            }),
            Err((host, error)) => Err((
                Self {
                    retired: RetiredThreadSuccessor {
                        receipt,
                        selection,
                        host: Box::new(host),
                    },
                },
                error,
            )),
        }
    }
}

impl MainWindowCompletedThreadSuccessorProgress {
    pub(crate) fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }
}

impl MainWindowFailedClaimRetirement {
    #[cfg(test)]
    pub(crate) fn test_predecessor_widget_release(
        &self,
    ) -> Option<(
        crate::main_window::MainWindowClaimRetirementKind,
        MainWindowComposerWidgetRelease,
    )> {
        self.predecessor_release.map(|release| (self.kind, release))
    }

    pub(crate) fn prior_selection(&self) -> MainWindowComposerSelectionIdentity {
        self.prior
    }
    pub(crate) fn saved_predecessor(&self) -> Option<&MainWindowRetiredClaimPredecessorSave> {
        self.saved.as_ref()
    }
    pub(crate) fn settle_predecessor_publication(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &SyndicStorage,
    ) -> Result<bool, String> {
        self.predecessor
            .as_mut()
            .ok_or("predecessor was already consumed")?
            .settle_predecessor_publication(candidate, storage)
    }
    pub(crate) fn accept_committed_claim(
        &mut self,
        committed: &crate::same_window_thread_acquisition::SameWindowThreadCommit,
        candidate: &mut HomeRecoveryCandidate,
        state: &beryl_state::BerylState,
    ) -> Result<(), String> {
        if self
            .predecessor
            .as_ref()
            .is_none_or(|predecessor| !predecessor.predecessor_publication_is_settled())
            || self.kind != crate::main_window::MainWindowClaimRetirementKind::ThreadCreation
            || committed.window.window_id() != self.prior.window_id()
            || self
                .committed_target
                .is_some_and(|target| target != committed.selection)
            || self
                .successor
                .as_ref()
                .is_some_and(|successor| successor.selection.claim() != committed.selection)
        {
            return Err("original committed claim does not belong to this retirement".into());
        }
        committed
            .validate_candidate(
                &candidate
                    .recovery_access()
                    .map_err(|error| error.to_string())?,
                state,
            )
            .map_err(|error| error.to_string())?;
        self.committed_target = Some(committed.selection);
        self.claim_settled = true;
        Ok(())
    }
    pub(crate) fn settle_remaining_cleanup(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &SyndicStorage,
    ) -> Result<(), String> {
        if !self.claim_settled {
            return Err("original committed claim has not been authenticated".into());
        }
        self.predecessor
            .as_mut()
            .ok_or("predecessor was already consumed")?
            .settle_disposal(candidate, storage)?;
        if let Some(successor) = self.successor.as_mut() {
            successor.host.settle_cleanup(candidate, storage)?;
        }
        if let Some(completed) = self.completed_successor.as_mut() {
            completed.retired.host.settle_cleanup(candidate, storage)?;
        }
        if let Some(progress) = self.completed_progress.as_ref() {
            progress.host.validate_candidate(candidate, storage)?;
        }
        self.cleanup_settled = true;
        Ok(())
    }
    pub(crate) fn into_prior_after_proven_noncommit(
        mut self: Box<Self>,
    ) -> Result<MainWindowFailedComposerRetirement, Box<Self>> {
        if self.committed_target.is_some()
            || self.kind != crate::main_window::MainWindowClaimRetirementKind::ThreadCreation
            || self.claim_settled
            || self.successor.is_some()
            || self.completed_successor.is_some()
            || self.completed_progress.is_some()
            || self.mounted_successor.is_some()
            || self.predecessor_release.is_some()
        {
            return Err(self);
        }
        match (*self.predecessor.take().unwrap()).into_prior() {
            Ok(host) => Ok(
                MainWindowFailedComposerRetirement::from_failed_claim_cleanup(
                    self.prior,
                    host,
                    self.last_activation_generation,
                ),
            ),
            Err(host) => {
                self.predecessor = Some(Box::new(host));
                Err(self)
            }
        }
    }
    pub(crate) fn accept_predecessor_widget_release(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        requests: &[gpui_text_input::RangeTextInputRequest],
    ) -> Result<MainWindowComposerWidgetRelease, String> {
        if !self.claim_settled || selection != self.prior && selection != self.prior_widget {
            return Err("predecessor release has no exact committed source".into());
        }
        Self::accept_release(&mut self.predecessor_release, selection, requests)
    }
    pub(crate) fn accept_successor_widget_release(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        requests: &[gpui_text_input::RangeTextInputRequest],
    ) -> Result<MainWindowComposerWidgetRelease, String> {
        if !self.claim_settled || self.mounted_successor != Some(selection) {
            return Err("successor release has no exact committed source".into());
        }
        Self::accept_release(&mut self.successor_release, selection, requests)
    }

    pub(crate) fn validate_complete_retirement(&self) -> Result<(), String> {
        if !self.claim_settled
            || !self.cleanup_settled
            || self.predecessor_release.is_none()
            || self.mounted_successor.is_some_and(|mounted| {
                self.successor_release
                    .is_none_or(|release| release.selection() != mounted)
            })
        {
            return Err(
                "failed thread creation original cleanup or widget release is incomplete".into(),
            );
        }
        Ok(())
    }
    fn accept_release(
        saved: &mut Option<MainWindowComposerWidgetRelease>,
        selection: MainWindowComposerSelectionIdentity,
        requests: &[gpui_text_input::RangeTextInputRequest],
    ) -> Result<MainWindowComposerWidgetRelease, String> {
        if requests
            .iter()
            .any(|request| !MainWindowComposerSlot::widget_release_request_is_settled(request))
        {
            return Err("retired widget release still owns dispatcher work".into());
        }
        if let Some(saved) = saved {
            if saved.selection() != selection {
                return Err("retired widget release source changed".into());
            }
            return Ok(*saved);
        }
        let release = MainWindowComposerWidgetRelease::new(selection);
        *saved = Some(release);
        Ok(release)
    }
}
