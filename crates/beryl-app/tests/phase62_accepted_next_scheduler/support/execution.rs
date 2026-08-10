use std::sync::{Arc, Mutex};

use beryl_app::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
    cas_projection::{
        AdmittedProjectionSession, OrdinaryDynamicToolAuthority, OrdinaryDynamicToolContext,
        OrdinaryDynamicToolHandlers, OrdinaryTurnExecutionRequest, ScheduledOrdinaryAdmission,
        ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
        ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionProviderFactory,
        ScheduledOrdinaryExecutionUnavailable, ScheduledOrdinaryProviderEpochContext,
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

pub fn supervised_ready_provider(slot: SessionSlot, assets: AssetState) -> CheckoutProvider {
    CheckoutProvider {
        slot,
        assets,
        clear_session_on_shutdown: false,
    }
}

pub struct ReadyProviderFactory {
    slot: SessionSlot,
    ready_epochs: Option<usize>,
    epochs: usize,
}

impl ReadyProviderFactory {
    pub fn every_epoch(slot: SessionSlot) -> Self {
        Self {
            slot,
            ready_epochs: None,
            epochs: 0,
        }
    }

    pub fn first_epoch_only(slot: SessionSlot) -> Self {
        Self {
            slot,
            ready_epochs: Some(1),
            epochs: 0,
        }
    }
}

impl ScheduledOrdinaryExecutionProviderFactory for ReadyProviderFactory {
    fn create_epoch(
        &mut self,
        context: ScheduledOrdinaryProviderEpochContext,
    ) -> Result<
        Box<dyn ScheduledOrdinaryExecutionProvider>,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    > {
        let ready = self
            .ready_epochs
            .is_none_or(|ready_epochs| self.epochs < ready_epochs);
        self.epochs += 1;
        if ready {
            Ok(Box::new(CheckoutProvider {
                slot: self.slot.clone(),
                assets: context.state().assets(),
                clear_session_on_shutdown: false,
            }))
        } else {
            Ok(Box::new(UnavailableProvider))
        }
    }

    fn shutdown(&mut self) {
        self.slot.take();
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
