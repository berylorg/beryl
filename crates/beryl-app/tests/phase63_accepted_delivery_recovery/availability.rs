use std::{
    path::Path,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use beryl_app::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
    cas_projection::{
        CasProjectionCoordinator, CasProjectionRequest, OrdinaryDynamicToolContext,
        OrdinaryDynamicToolHandlers, OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionOutcome,
        OrdinaryTurnExecutionRequest, ProjectionCancellationToken, ProjectionConnectionService,
        ProjectionServiceConfig, RunningSessionRecoverySupervisor, ScheduledOrdinaryAdmission,
        ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
        ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionProviderFactory,
        ScheduledOrdinaryExecutionUnavailable, ScheduledOrdinaryProviderEpochContext,
    },
};
use beryl_backend::{
    DynamicToolCallResponse, ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions,
};
use beryl_home_store::{
    HomeOpenOptions, HomeSchemaVersion, HomeStore, test_faults::FaultController,
};
use beryl_model::{CasProcessGeneration, RuntimeId};
use beryl_state::BerylState;
use syndic_storage::{BindingState, InputGateState, SyndicStorage, TurnLifecycle};

use crate::{
    app_support::{
        PromotedTurn, SeededHome, close_seeded, point_limit, promote_installed_next,
        reopen_registered, restart_service, restart_service_with, seeded_home, time,
    },
    phase62_support::{
        AUTHORIZATION, CheckoutProvider, NextRecordIds, NormalTerminalServer, SessionSlot, TIMEOUT,
        UnavailableProvider, execution_binding, fail_home_generation_before_promotion,
        install_next_records, ready_provider, supervised_ready_provider, wait_until,
    },
    records::{ActiveSeed, activate_promoted_turn, cancel_activation},
};

struct NoopLifecycle;

impl LifecycleYieldRequestHandler for NoopLifecycle {
    fn respond_lifecycle_yield(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused lifecycle handler")
    }
}

struct NoopBranch;

impl BranchDiscussionResolutionRequestHandler for NoopBranch {
    fn respond_branch_discussion_resolution(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused branch handler")
    }
}

struct CountingUnavailableProvider {
    attempts: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProvider for CountingUnavailableProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

struct RecoveredFixture {
    ids: NextRecordIds,
    promoted: PromotedTurn,
    active: ActiveSeed,
    runtime_id: RuntimeId,
}

fn install_recovered_pending(home: &SeededHome, seed: u8) -> RecoveredFixture {
    let ids = install_next_records(
        &home.store,
        home.storage,
        seed,
        execution_binding(RuntimeId::from_bytes([seed.wrapping_add(60); 16])),
    );
    let promoted = promote_installed_next(
        &home.store,
        home.storage,
        &home.state,
        ids,
        seed.wrapping_add(30),
    );
    let active = activate_promoted_turn(
        &home.store,
        home.storage,
        ids.thread,
        promoted.turn,
        seed.wrapping_add(60),
        false,
    );
    cancel_activation(
        &home.store,
        home.storage,
        ids.thread,
        promoted.turn,
        active.snapshot,
    );
    let binding = home
        .storage
        .current_binding(&home.store, ids.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("recovered-pending fixture must retain valid projection authority");
    };
    let runtime_id = usable.execution().runtime_id();
    RecoveredFixture {
        ids,
        promoted,
        active,
        runtime_id,
    }
}

#[derive(Clone, Default)]
struct ProviderPause {
    shared: Arc<(Mutex<(bool, bool)>, Condvar)>,
}

impl ProviderPause {
    fn pause(&self) {
        let (state, changed) = &*self.shared;
        let mut state = state.lock().unwrap();
        state.0 = true;
        changed.notify_all();
        while !state.1 {
            state = changed.wait(state).unwrap();
        }
    }

    fn wait_until_paused(&self) {
        wait_until("recovered provider generation cut", || {
            self.shared.0.lock().unwrap().0.then_some(())
        });
    }

    fn release(&self) {
        let (state, changed) = &*self.shared;
        state.lock().unwrap().1 = true;
        changed.notify_all();
    }
}

impl Drop for ProviderPause {
    fn drop(&mut self) {
        let (state, changed) = &*self.shared;
        state.lock().unwrap().1 = true;
        changed.notify_all();
    }
}

struct PausingCheckoutProvider {
    checkout: CheckoutProvider,
    session: SessionSlot,
    pause: ProviderPause,
}

impl ScheduledOrdinaryExecutionProvider for PausingCheckoutProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        if self.session.is_ready() {
            self.pause.pause();
        }
        self.checkout.try_issue(admission)
    }

    fn shutdown(&mut self) {
        self.checkout.shutdown();
    }
}

struct PausingProviderFactory {
    slot: SessionSlot,
    pause: ProviderPause,
    epochs: usize,
}

impl ScheduledOrdinaryExecutionProviderFactory for PausingProviderFactory {
    fn create_epoch(
        &mut self,
        context: ScheduledOrdinaryProviderEpochContext,
    ) -> Result<
        Box<dyn ScheduledOrdinaryExecutionProvider>,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    > {
        self.epochs += 1;
        if self.epochs == 1 {
            Ok(Box::new(PausingCheckoutProvider {
                checkout: supervised_ready_provider(self.slot.clone(), context.state().assets()),
                session: self.slot.clone(),
                pause: self.pause.clone(),
            }))
        } else {
            Ok(Box::new(UnavailableProvider))
        }
    }

    fn shutdown(&mut self) {
        self.slot.clear();
    }
}

fn assert_recovered_pending(store: &HomeStore, storage: SyndicStorage, fixture: &RecoveredFixture) {
    assert_eq!(
        storage
            .input_gate(store, fixture.ids.thread, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::PendingTurn(fixture.promoted.turn)
    );
    assert_eq!(
        storage
            .turn_state(store, fixture.promoted.turn, point_limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        TurnLifecycle::Pending
    );
}

#[path = "availability/flight.rs"]
mod flight;
#[path = "availability/lifecycle.rs"]
mod lifecycle;
