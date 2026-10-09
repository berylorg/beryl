use super::*;

impl ActivationSource {
    #[inline(never)]
    pub(super) fn run_prior_disposal_source(&mut self) -> Result<(), String> {
        #[cfg(all(test, feature = "test-faults"))]
        if let Some(hook) = self.before_disposal.take() {
            hook(&self.cancellation);
        }
        self.advance = Some(self.service.advance_committed_claim_disposal_source(
            self.receipt()?,
            self.expected,
            fresh_piece_operation_id()?,
        )?);
        self.stage = Stage::AcceptDisposal;
        Ok(())
    }
}
