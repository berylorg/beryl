use beryl_app::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
    cas_projection::{OrdinaryDynamicToolContext, test_faults::TerminalHistoryBarrierController},
};
use beryl_backend::DynamicToolCallResponse;

#[derive(Default)]
pub(super) struct NoopLifecycle;

impl LifecycleYieldRequestHandler for NoopLifecycle {
    fn respond_lifecycle_yield(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused lifecycle handler")
    }
}

#[derive(Default)]
pub(super) struct NoopBranch;

impl BranchDiscussionResolutionRequestHandler for NoopBranch {
    fn respond_branch_discussion_resolution(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused branch handler")
    }
}

pub(super) struct TerminalHistoryReleaseGuard {
    controller: Option<TerminalHistoryBarrierController>,
}

impl TerminalHistoryReleaseGuard {
    pub(super) fn new(controller: TerminalHistoryBarrierController) -> Self {
        Self {
            controller: Some(controller),
        }
    }

    pub(super) fn wait(&self) {
        self.controller
            .as_ref()
            .expect("terminal-history release guard remains armed")
            .wait();
    }

    pub(super) fn release(&mut self) {
        self.controller
            .take()
            .expect("terminal-history release guard releases once")
            .release();
    }
}

impl Drop for TerminalHistoryReleaseGuard {
    fn drop(&mut self) {
        if let Some(controller) = self.controller.take() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                controller.release();
            }));
        }
    }
}
