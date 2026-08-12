use std::sync::{Arc, Mutex};

use beryl_app::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
    cas_projection::{
        AdmittedProjectionSession, OrdinaryDynamicToolAuthority, OrdinaryDynamicToolContext,
        OrdinaryDynamicToolHandlers, OrdinaryTurnExecutionRequest, ScheduledOrdinaryAdmission,
        ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
        ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
        ScheduledOrdinaryRequestPolicy, ScheduledProjectionSessionAuthority,
    },
};
use beryl_backend::{DynamicToolCallResponse, ThreadStartOptions, TurnStartOptions};
use beryl_state::AssetState;

use super::TIMEOUT;

#[derive(Clone)]
pub struct SessionSlot(Arc<Mutex<Option<AdmittedProjectionSession>>>);

impl Default for SessionSlot {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }
}

impl SessionSlot {
    pub fn replace(&self, session: AdmittedProjectionSession) {
        assert!(self.0.lock().unwrap().replace(session).is_none());
    }

    pub fn is_ready(&self) -> bool {
        self.0.lock().unwrap().is_some()
    }

    pub fn clear(&self) {
        self.take();
    }

    fn take(&self) -> Option<AdmittedProjectionSession> {
        self.0.lock().unwrap().take()
    }
}

struct ReturningSession {
    session: Option<AdmittedProjectionSession>,
    slot: SessionSlot,
}

impl ScheduledProjectionSessionAuthority for ReturningSession {
    fn session(&mut self) -> &mut AdmittedProjectionSession {
        self.session
            .as_mut()
            .expect("scheduled session remains owned until authority returns")
    }
}

impl Drop for ReturningSession {
    fn drop(&mut self) {
        let session = self
            .session
            .take()
            .expect("scheduled session returns exactly once");
        self.slot.replace(session);
    }
}

struct LifecycleHandler;

impl LifecycleYieldRequestHandler for LifecycleHandler {
    fn respond_lifecycle_yield(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused lifecycle handler")
    }
}

struct BranchHandler;

impl BranchDiscussionResolutionRequestHandler for BranchHandler {
    fn respond_branch_discussion_resolution(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused branch handler")
    }
}

struct ToolAuthority {
    lifecycle: LifecycleHandler,
    branch: BranchHandler,
}

impl OrdinaryDynamicToolAuthority for ToolAuthority {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        OrdinaryDynamicToolHandlers::new(&mut self.lifecycle, &mut self.branch)
    }
}

pub struct CheckoutProvider {
    slot: SessionSlot,
    assets: AssetState,
    clear_session_on_shutdown: bool,
}

impl ScheduledOrdinaryExecutionProvider for CheckoutProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        let Some(session) = self.slot.take() else {
            return Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::SessionBusy));
        };
        admission
            .issue(
                Box::new(ReturningSession {
                    session: Some(session),
                    slot: self.slot.clone(),
                }),
                request_policy(),
                self.assets,
                Box::new(ToolAuthority {
                    lifecycle: LifecycleHandler,
                    branch: BranchHandler,
                }),
            )
            .map(ScheduledOrdinaryAdmissionResult::Issued)
    }

    fn shutdown(&mut self) {
        if self.clear_session_on_shutdown {
            self.slot.take();
        }
    }
}

pub struct UnavailableProvider;

impl ScheduledOrdinaryExecutionProvider for UnavailableProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

pub fn ready_provider(slot: SessionSlot, assets: AssetState) -> CheckoutProvider {
    CheckoutProvider {
        slot,
        assets,
        clear_session_on_shutdown: true,
    }
}

fn request_policy() -> ScheduledOrdinaryRequestPolicy {
    ScheduledOrdinaryRequestPolicy::new(
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        TIMEOUT,
        OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
    )
}
