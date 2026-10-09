use super::*;

impl ActivationSource {
    #[inline(never)]
    pub(super) fn run_prior_save_source(&mut self) -> Result<(), String> {
        let receipt = self.receipt()?;
        if self.flush.is_none() {
            self.flush = Some(self.service.begin_claim_publication_save(receipt)?);
        }
        match self.flush.unwrap() {
            FlushAdmission::Started {
                ticket,
                state: FlushState::CaptureRequired,
            }
            | FlushAdmission::Joined {
                ticket,
                state: FlushState::CaptureRequired,
            } => {
                #[cfg(all(test, feature = "test-faults"))]
                {
                    if let Some(hook) = self.before_save.take() {
                        hook(&self.cancellation);
                    }
                }
                let authority = match self.service.autosave_capture_requirement(self.expected)? {
                    crate::main_window::MainWindowComposerAutosaveCaptureRequirement::ChangedMarkers => Some(fresh_marker_authority()?),
                    _ => None,
                };
                let captured = self.service.capture_flush_publication(
                    self.expected,
                    ticket,
                    self.assets.clone(),
                    &self.marker_seals,
                    fresh_piece_operation_id()?,
                    authority,
                    current_timestamp()?,
                    &self.cancellation,
                )?;
                if matches!(captured, FlushCapture::Unsatisfied(_)) {
                    self.error = Some("Prior composer flush was refused".into());
                    self.stage = Stage::Retire;
                    return Ok(());
                }
                if matches!(captured, FlushCapture::Stale) {
                    self.terminal_release_failure = true;
                    return Err(
                        "Prior composer save source was stale; settlement custody is retained"
                            .into(),
                    );
                }
                self.flush = Some(FlushAdmission::Joined {
                    ticket,
                    state: FlushState::PublicationPending,
                });
            }
            FlushAdmission::Started {
                state: FlushState::DisposalRequired,
                ..
            }
            | FlushAdmission::Joined {
                state: FlushState::DisposalRequired,
                ..
            } => return Err("Prior composer flush unexpectedly required disposal".into()),
            _ => {}
        }
        let advance = self.service.advance_claim_publication_source(receipt)?;
        if matches!(advance.advance, PublishAdvance::ReconciliationPending) {
            self.error = Some("Prior editor save is awaiting reconciliation".into());
        }
        if let PublishAdvance::Progress(state) = advance.advance {
            match self.flush.unwrap() {
                FlushAdmission::Started { ticket, .. } | FlushAdmission::Joined { ticket, .. } => {
                    self.flush = Some(FlushAdmission::Joined { ticket, state });
                }
                FlushAdmission::Satisfied(_) => {}
            }
        }
        self.advance = Some(advance);
        self.stage = Stage::AcceptFlush;
        Ok(())
    }
}
