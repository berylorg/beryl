use super::*;

impl MainWindowShellRoot {
    pub(crate) fn test_captured_branch_activation_attempts(&self) -> Option<u32> {
        Some(
            self.branch_discussion
                .operation
                .as_ref()?
                .try_lock()
                .ok()?
                .activation_attempts,
        )
    }
    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_fill_transcript_worker_capacity(
        &self,
    ) -> Vec<crate::main_window::conversation_composer_mount::worker::WorkerLease> {
        let admission = self.running_threads.workers.admission(9);
        let mut leases = Vec::new();
        while let Some(lease) = admission.acquire() {
            leases.push(lease);
        }
        leases
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_transcript_worker_retained(&self) -> usize {
        self.running_threads.workers.retained()
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_captured_branch_hooks(
        &mut self,
        creation: Option<Box<dyn FnOnce(&beryl_home_store::CommandCancellation) + Send>>,
        reconciliation: Option<Box<dyn FnOnce(CreatedDiscussion) + Send>>,
    ) {
        self.branch_discussion.before_creation = creation;
        self.branch_discussion.before_reconciliation = reconciliation;
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_captured_branch_error(&self) -> Option<String> {
        self.branch_discussion
            .operation
            .as_ref()?
            .try_lock()
            .ok()?
            .error
            .clone()
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_captured_branch_state(
        &self,
    ) -> Option<(
        &'static str,
        beryl_model::SyndicThreadId,
        crate::syndic_transcript::CapturedDiscussionSource,
    )> {
        let state = self.branch_discussion.operation.as_ref()?.try_lock().ok()?;
        Some((
            match state.stage {
                BranchStage::Creating => "Creating",
                BranchStage::Reconciling => "Reconciling",
                BranchStage::Activating => "Activating",
                BranchStage::Retry => "Retry",
                BranchStage::NotCommitted => "NotCommitted",
                BranchStage::Unavailable => "Unavailable",
                BranchStage::Complete => "Complete",
            },
            state.request.thread_id(),
            state.captured.captured.clone(),
        ))
    }
}
