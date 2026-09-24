use std::sync::Arc;

use crate::lifecycle_attention::ProcessLifecycleAttentionPool;

mod resolution;
use resolution::ProcessResolutionHandler;
pub(in crate::cas_projection) use resolution::ResolutionAuthority;

use super::{
    OrdinaryDynamicToolAuthority, OrdinaryDynamicToolHandlers, ProcessLifecycleYieldHandler,
    ProjectionConnectionService,
};

#[derive(Clone)]
pub struct ProcessOrdinaryDynamicToolAuthority {
    lifecycle: ProcessLifecycleYieldHandler,
    branch: ProcessResolutionHandler,
}

impl ProjectionConnectionService {
    pub fn ordinary_dynamic_tool_authority(
        &self,
        attention: &Arc<ProcessLifecycleAttentionPool>,
    ) -> ProcessOrdinaryDynamicToolAuthority {
        ProcessOrdinaryDynamicToolAuthority {
            lifecycle: self.lifecycle_yield_handler(attention),
            branch: ProcessResolutionHandler::new(Arc::downgrade(self.resolution_authority())),
        }
    }
}

impl OrdinaryDynamicToolAuthority for ProcessOrdinaryDynamicToolAuthority {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        OrdinaryDynamicToolHandlers::new(&mut self.lifecycle, &mut self.branch)
    }
}

#[cfg(feature = "test-faults")]
impl ProcessOrdinaryDynamicToolAuthority {
    pub fn test_resolve(
        &mut self,
        context: super::BranchDiscussionResolutionContext,
        resolution: String,
    ) -> beryl_backend::DynamicToolCallResponse {
        use crate::BranchDiscussionResolutionRequestHandler;
        self.branch.respond_branch_discussion_resolution(
            context,
            crate::BranchDiscussionResolutionRequest::for_test(resolution),
        )
    }
}
