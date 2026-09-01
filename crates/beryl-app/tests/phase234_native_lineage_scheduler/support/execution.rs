use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

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
use beryl_model::RuntimeId;
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

#[derive(Clone, Default)]
pub struct SessionPool(Arc<Mutex<VecDeque<AdmittedProjectionSession>>>);

impl SessionPool {
    pub fn push(&self, session: AdmittedProjectionSession) {
        self.0.lock().unwrap().push_back(session);
    }

    pub fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    fn take_for(&self, runtime_id: &RuntimeId) -> Option<AdmittedProjectionSession> {
        let mut sessions = self.0.lock().unwrap();
        let index = sessions
            .iter()
            .position(|session| session.runtime_id() == *runtime_id)?;
        sessions.remove(index)
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
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

struct ReturningPooledSession {
    session: Option<AdmittedProjectionSession>,
    pool: SessionPool,
}

impl ScheduledProjectionSessionAuthority for ReturningPooledSession {
    fn session(&mut self) -> &mut AdmittedProjectionSession {
        self.session
            .as_mut()
            .expect("scheduled pooled session remains owned until authority returns")
    }
}

impl Drop for ReturningPooledSession {
    fn drop(&mut self) {
        let session = self
            .session
            .take()
            .expect("scheduled pooled session returns exactly once");
        self.pool.push(session);
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

pub struct PooledCheckoutProvider {
    pool: SessionPool,
    assets: AssetState,
}

impl ScheduledOrdinaryExecutionProvider for PooledCheckoutProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        let runtime_id = admission.execution_binding().runtime_id().clone();
        let Some(session) = self.pool.take_for(&runtime_id) else {
            return Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::SessionBusy));
        };
        admission
            .issue(
                Box::new(ReturningPooledSession {
                    session: Some(session),
                    pool: self.pool.clone(),
                }),
                request_policy(),
                self.assets.clone(),
                Box::new(ToolAuthority {
                    lifecycle: LifecycleHandler,
                    branch: BranchHandler,
                }),
            )
            .map(ScheduledOrdinaryAdmissionResult::Issued)
    }

    fn shutdown(&mut self) {
        self.pool.clear();
    }
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
                self.assets.clone(),
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

pub fn pooled_ready_provider(pool: SessionPool, assets: AssetState) -> PooledCheckoutProvider {
    PooledCheckoutProvider { pool, assets }
}

fn request_policy() -> ScheduledOrdinaryRequestPolicy {
    ScheduledOrdinaryRequestPolicy::new(
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        TIMEOUT,
        OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
    )
}
