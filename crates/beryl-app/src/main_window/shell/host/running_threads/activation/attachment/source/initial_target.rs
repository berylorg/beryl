use super::*;

impl ActivationSource {
    #[inline(never)]
    pub(super) fn run_initial_target_source(&mut self) -> Result<(), String> {
        if self.creation.is_some() {
            self.capture_completed_successor()?;
            if let Some(completed) = self.completed_successor.take() {
                match self
                    .service
                    .settle_completed_thread_successor_cleanup(completed)
                {
                    Ok(progress) => self.completed_successor_progress = Some(progress),
                    Err((completed, error)) => {
                        self.completed_successor = Some(completed);
                        return Err(error);
                    }
                }
            }
        }
        let (home, state, syndic) = self
            .reader
            .activation_sources()
            .ok_or("Running threads source retired")?;
        self.home = Some(home);
        self.state = Some(state);
        self.syndic = Some(syndic);
        let (request, retirement) =
            crate::bootstrap::thread_activation_request(self.target.thread_id())?;
        let committed_preparation = beryl_home_store::CommandCancellation::new();
        let preparation_cancel = if self.creation.is_some() {
            &committed_preparation
        } else {
            &self.cancellation
        };
        let admission = if self.creation.is_some() && self.receipt.is_some() {
            Ok(ActivationAdvance::Ready(self.receipt()?))
        } else if self.creation.is_some() {
            self.service.begin_thread_creation_activation(
                self.target,
                request,
                retirement,
                preparation_cancel,
            )
        } else {
            self.service.begin_ordinary_claim_activation(
                self.target,
                request,
                retirement,
                preparation_cancel,
            )
        };
        let admission = match admission {
            Ok(value) => value,
            Err(error) => {
                self.receipt = self.service.pending_receipt();
                self.capture_completed_successor()?;
                return Err(error);
            }
        };
        match admission {
            ActivationAdvance::Ready(receipt) => self.receipt = Some(receipt),
            ActivationAdvance::RetirementPending(receipt)
            | ActivationAdvance::FailureRetirementPending { receipt, .. } => {
                self.receipt = Some(receipt);
                return Err("Target composer preparation requires retirement".into());
            }
            _ => return Err("Target composer preparation was refused".into()),
        }
        let prepared = self
            .service
            .prepare_claim_presentation_source(self.receipt()?)?;
        self.pending_selection = Some(prepared.selection());
        self.request.activation = prepared.selection().binding().host_generation().get();
        self.presentation = Some(prepared);
        if self.provider.is_none() {
            self.provider = Some(
                self.reader
                    .transcript_provider()
                    .map_err(|error| error.to_string())?,
            );
        }
        self.transcript = Some(
            self.provider
                .as_ref()
                .unwrap()
                .prepare_attachment(self.request.clone(), &self.transcript_cancel)
                .map_err(|error| error.to_string())?,
        );
        self.stage = Stage::Install;
        Ok(())
    }
}
