use std::sync::Arc;

use beryl_backend::DynamicToolCallResponse;

use crate::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    lifecycle_attention::ProcessLifecycleAttentionPool,
};

use super::{
    OrdinaryDynamicToolAuthority, OrdinaryDynamicToolContext, OrdinaryDynamicToolHandlers,
    ProcessLifecycleYieldHandler, ProjectionConnectionService,
};

pub struct ProcessOrdinaryDynamicToolAuthority {
    lifecycle: ProcessLifecycleYieldHandler,
    branch: UnavailableBranchResolution,
}

impl ProjectionConnectionService {
    pub fn ordinary_dynamic_tool_authority(
        &self,
        attention: &Arc<ProcessLifecycleAttentionPool>,
    ) -> ProcessOrdinaryDynamicToolAuthority {
        ProcessOrdinaryDynamicToolAuthority {
            lifecycle: self.lifecycle_yield_handler(attention),
            branch: UnavailableBranchResolution,
        }
    }
}

impl OrdinaryDynamicToolAuthority for ProcessOrdinaryDynamicToolAuthority {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        OrdinaryDynamicToolHandlers::new(&mut self.lifecycle, &mut self.branch)
    }
}

struct UnavailableBranchResolution;

impl BranchDiscussionResolutionRequestHandler for UnavailableBranchResolution {
    fn respond_branch_discussion_resolution(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::failure_text("Branch discussion resolution is unavailable.")
    }
}
