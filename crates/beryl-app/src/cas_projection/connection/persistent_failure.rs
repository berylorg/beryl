use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
};

use beryl_backend::{
    CallerNoSuccessorFence, PersistentFailureInterruptCorrelation, TurnInterruptDisposition,
};

use super::{ConnectionRegistryAuthority, driver::ConnectionRequestSession, router::EventRouter};
use crate::cas_projection::{
    connection::router::{
        PersistentFailureTargetIneligibility, PersistentFailureTargetProof,
        PersistentFailureTargetWitness,
    },
    persistent_failure::PersistentFailureCutIdentity,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum PersistentFailureInterruptDisposition {
    RequestAccepted,
    RejectedBeforeCoreInterrupt,
    ProvenNotDispatched,
    CompletionUnknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum PersistentFailureNoDispatchReason {
    DriverUnavailable,
    Router(PersistentFailureTargetIneligibility),
    RandomUnavailable,
    BindRejected,
    AuthorizationRejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum PersistentFailureDriverResult {
    NoDispatch(PersistentFailureNoDispatchReason),
    Attempted {
        correlation: PersistentFailureInterruptCorrelation,
        disposition: PersistentFailureInterruptDisposition,
        session_authority_invalidated: bool,
    },
}

#[derive(Clone)]
struct PersistentFailureResultSlot {
    inner: Arc<(Mutex<Option<PersistentFailureDriverResult>>, Condvar)>,
}

impl PersistentFailureResultSlot {
    fn new() -> Self {
        Self {
            inner: Arc::new((Mutex::new(None), Condvar::new())),
        }
    }

    fn publish(&self, result: PersistentFailureDriverResult) {
        let mut slot = self
            .inner
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if slot.is_none() {
            *slot = Some(result);
        }
        drop(slot);
        self.inner.1.notify_all();
    }

    fn wait(&self) -> PersistentFailureDriverResult {
        let mut slot = self
            .inner
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        loop {
            if let Some(result) = *slot {
                return result;
            }
            slot = self
                .inner
                .1
                .wait(slot)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }
}

/// One exact target result paired with the immutable witness frozen before dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct PersistentFailureCompletedTarget {
    witness: PersistentFailureTargetWitness,
    result: PersistentFailureDriverResult,
}

impl PersistentFailureCompletedTarget {
    pub(in crate::cas_projection) const fn witness(&self) -> &PersistentFailureTargetWitness {
        &self.witness
    }

    pub(in crate::cas_projection) const fn result(&self) -> PersistentFailureDriverResult {
        self.result
    }

    pub(in crate::cas_projection) fn into_parts(
        self,
    ) -> (
        PersistentFailureTargetWitness,
        PersistentFailureDriverResult,
    ) {
        (self.witness, self.result)
    }
}

#[derive(Clone)]
pub(in crate::cas_projection) struct PersistentFailureCompletion {
    witness: PersistentFailureTargetWitness,
    result: PersistentFailureResultSlot,
}

impl PersistentFailureCompletion {
    fn new(witness: PersistentFailureTargetWitness) -> Self {
        Self {
            witness,
            result: PersistentFailureResultSlot::new(),
        }
    }

    fn publish(&self, result: PersistentFailureDriverResult) {
        self.result.publish(result);
    }

    pub(in crate::cas_projection) const fn witness(&self) -> &PersistentFailureTargetWitness {
        &self.witness
    }

    pub(in crate::cas_projection) fn wait(&self) -> PersistentFailureDriverResult {
        self.result.wait()
    }

    pub(in crate::cas_projection) fn wait_with_witness(&self) -> PersistentFailureCompletedTarget {
        PersistentFailureCompletedTarget {
            witness: self.witness.clone(),
            result: self.result.wait(),
        }
    }
}

struct PersistentFailureDriverObligation {
    proof: PersistentFailureTargetProof,
    completion: PersistentFailureResultSlot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PersistentFailureDriverClosure {
    OrdinaryRetirement,
    Failure(PersistentFailureCutIdentity),
}

struct PersistentFailureDriverState {
    closure: Option<PersistentFailureDriverClosure>,
    driver_alive: bool,
    obligations: VecDeque<PersistentFailureDriverObligation>,
}

struct InFlightPersistentFailureResult {
    completion: PersistentFailureResultSlot,
    unwind_fallback: PersistentFailureDriverResult,
}

impl InFlightPersistentFailureResult {
    fn new(completion: PersistentFailureResultSlot) -> Self {
        Self {
            completion,
            unwind_fallback: PersistentFailureDriverResult::NoDispatch(
                PersistentFailureNoDispatchReason::DriverUnavailable,
            ),
        }
    }

    fn arm_interrupt_attempt(&mut self, correlation: PersistentFailureInterruptCorrelation) {
        self.unwind_fallback = PersistentFailureDriverResult::Attempted {
            correlation,
            disposition: PersistentFailureInterruptDisposition::CompletionUnknown,
            session_authority_invalidated: true,
        };
    }

    fn publish(self, result: PersistentFailureDriverResult) {
        self.completion.publish(result);
    }
}

impl Drop for InFlightPersistentFailureResult {
    fn drop(&mut self) {
        self.completion.publish(self.unwind_fallback);
    }
}

pub(in crate::cas_projection) struct PersistentFailureDriverSlot {
    state: Mutex<PersistentFailureDriverState>,
    changed: Condvar,
}

impl PersistentFailureDriverSlot {
    pub(in crate::cas_projection) fn new() -> Self {
        Self {
            state: Mutex::new(PersistentFailureDriverState {
                closure: None,
                driver_alive: true,
                obligations: VecDeque::new(),
            }),
            changed: Condvar::new(),
        }
    }

    pub(in crate::cas_projection) fn install(
        &self,
        identity: PersistentFailureCutIdentity,
        proofs: Vec<PersistentFailureTargetProof>,
    ) -> Result<Vec<PersistentFailureCompletion>, ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        match state.closure {
            Some(PersistentFailureDriverClosure::Failure(existing)) => {
                return if existing == identity {
                    Ok(Vec::new())
                } else {
                    Err(())
                };
            }
            Some(PersistentFailureDriverClosure::OrdinaryRetirement) => return Err(()),
            None => {}
        }
        state.closure = Some(PersistentFailureDriverClosure::Failure(identity));
        let mut completions = Vec::with_capacity(proofs.len());
        for proof in proofs {
            let completion = PersistentFailureCompletion::new(proof.witness().clone());
            if state.driver_alive {
                state
                    .obligations
                    .push_back(PersistentFailureDriverObligation {
                        proof,
                        completion: completion.result.clone(),
                    });
            } else {
                completion.publish(PersistentFailureDriverResult::NoDispatch(
                    PersistentFailureNoDispatchReason::DriverUnavailable,
                ));
            }
            completions.push(completion);
        }
        drop(state);
        self.changed.notify_all();
        Ok(completions)
    }

    fn take(&self) -> Option<PersistentFailureDriverObligation> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .obligations
            .pop_front()
    }

    pub(in crate::cas_projection) fn cut_is_installed(&self) -> bool {
        self.state
            .lock()
            .map(|state| {
                matches!(
                    state.closure,
                    Some(PersistentFailureDriverClosure::Failure(_))
                )
            })
            .unwrap_or(true)
    }

    pub(in crate::cas_projection) fn matches_finished_cut(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> Result<bool, ()> {
        let state = self.state.lock().map_err(|_| ())?;
        Ok(
            state.closure == Some(PersistentFailureDriverClosure::Failure(identity))
                && state.obligations.is_empty(),
        )
    }

    pub(in crate::cas_projection) fn begin_ordinary_retirement(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match state.closure {
            Some(PersistentFailureDriverClosure::Failure(_)) => false,
            Some(PersistentFailureDriverClosure::OrdinaryRetirement) => true,
            None => {
                state.closure = Some(PersistentFailureDriverClosure::OrdinaryRetirement);
                drop(state);
                self.changed.notify_all();
                true
            }
        }
    }

    pub(in crate::cas_projection) fn ordinary_retirement_won(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.closure == Some(PersistentFailureDriverClosure::OrdinaryRetirement))
            .unwrap_or(false)
    }

    pub(in crate::cas_projection) fn wait_for_change(&self, timeout: std::time::Duration) {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let _ = self
            .changed
            .wait_timeout(state, timeout)
            .unwrap_or_else(|poison| poison.into_inner());
    }

    pub(in crate::cas_projection) fn driver_stopped(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.driver_alive = false;
        for obligation in state.obligations.drain(..) {
            obligation
                .completion
                .publish(PersistentFailureDriverResult::NoDispatch(
                    PersistentFailureNoDispatchReason::DriverUnavailable,
                ));
        }
        drop(state);
        self.changed.notify_all();
    }
}

pub(in crate::cas_projection) fn dispatch_next_persistent_failure(
    session: &mut ConnectionRequestSession<'_>,
    router: &EventRouter,
    authority: &ConnectionRegistryAuthority,
    slot: &PersistentFailureDriverSlot,
) -> bool {
    let Some(obligation) = slot.take() else {
        return false;
    };
    let mut completion = InFlightPersistentFailureResult::new(obligation.completion);
    let result = dispatch_persistent_failure(
        session,
        router,
        authority,
        &obligation.proof,
        &mut completion,
    );
    completion.publish(result);
    true
}

fn dispatch_persistent_failure(
    session: &mut ConnectionRequestSession<'_>,
    router: &EventRouter,
    authority: &ConnectionRegistryAuthority,
    proof: &PersistentFailureTargetProof,
    completion: &mut InFlightPersistentFailureResult,
) -> PersistentFailureDriverResult {
    if authority.is_retired() {
        return PersistentFailureDriverResult::NoDispatch(
            PersistentFailureNoDispatchReason::DriverUnavailable,
        );
    }
    let authorization = match router.authorize_persistent_failure_dispatch(proof) {
        Ok(authorization) => authorization,
        Err(reason) => {
            return PersistentFailureDriverResult::NoDispatch(
                PersistentFailureNoDispatchReason::Router(reason),
            );
        }
    };
    let mut correlation_bytes = [0_u8; 16];
    if getrandom::fill(&mut correlation_bytes).is_err() {
        return PersistentFailureDriverResult::NoDispatch(
            PersistentFailureNoDispatchReason::RandomUnavailable,
        );
    }
    let correlation = PersistentFailureInterruptCorrelation::from_bytes(correlation_bytes);
    let target = authorization.exact_target();
    if let Err(error) = session.backend.bind_exact_foreground_turn(target.clone()) {
        if error.invalidates_connection_authority() {
            let _ = authority.retire();
        }
        return PersistentFailureDriverResult::NoDispatch(
            PersistentFailureNoDispatchReason::BindRejected,
        );
    }
    let request = match session.backend.authorize_persistent_failure_interrupt(
        target,
        correlation,
        CallerNoSuccessorFence::issue(),
    ) {
        Ok(request) => request,
        Err(error) => {
            let mut invalidated = error.invalidates_connection_authority();
            if let Err(unbind) = session.backend.unbind_exact_foreground_turn() {
                invalidated |= unbind.invalidates_connection_authority();
            }
            if invalidated {
                let _ = authority.retire();
            }
            return PersistentFailureDriverResult::NoDispatch(
                PersistentFailureNoDispatchReason::AuthorizationRejected,
            );
        }
    };
    completion.arm_interrupt_attempt(correlation);
    let outcome = session
        .backend
        .interrupt_for_persistent_failure(request, authorization.request_timeout());
    let (disposition, mut invalidated) = match outcome.disposition() {
        TurnInterruptDisposition::RequestAccepted => (
            PersistentFailureInterruptDisposition::RequestAccepted,
            false,
        ),
        TurnInterruptDisposition::RejectedBeforeCoreInterrupt => (
            PersistentFailureInterruptDisposition::RejectedBeforeCoreInterrupt,
            false,
        ),
        TurnInterruptDisposition::ProvenNotDispatched { error } => (
            PersistentFailureInterruptDisposition::ProvenNotDispatched,
            error.invalidates_connection_authority(),
        ),
        TurnInterruptDisposition::CompletionUnknown { .. } => (
            PersistentFailureInterruptDisposition::CompletionUnknown,
            true,
        ),
    };
    if let Err(error) = session.backend.unbind_exact_foreground_turn() {
        invalidated |= error.invalidates_connection_authority();
    }
    if invalidated {
        let _ = authority.retire();
    }
    PersistentFailureDriverResult::Attempted {
        correlation,
        disposition,
        session_authority_invalidated: invalidated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwind_before_interrupt_attempt_completes_as_driver_unavailable() {
        let completion = PersistentFailureResultSlot::new();
        let in_flight = InFlightPersistentFailureResult::new(completion.clone());
        assert!(
            std::panic::catch_unwind(move || {
                let _in_flight = in_flight;
                panic!("simulated driver unwind after obligation dequeue");
            })
            .is_err()
        );

        assert_eq!(
            completion.wait(),
            PersistentFailureDriverResult::NoDispatch(
                PersistentFailureNoDispatchReason::DriverUnavailable
            )
        );
    }

    #[test]
    fn unwind_after_interrupt_attempt_begins_completes_as_unknown_attempt() {
        let completion = PersistentFailureResultSlot::new();
        let correlation = PersistentFailureInterruptCorrelation::from_bytes([7_u8; 16]);
        let mut in_flight = InFlightPersistentFailureResult::new(completion.clone());
        in_flight.arm_interrupt_attempt(correlation);
        assert!(
            std::panic::catch_unwind(move || {
                let _in_flight = in_flight;
                panic!("simulated driver unwind after interrupt dispatch began");
            })
            .is_err()
        );

        assert_eq!(
            completion.wait(),
            PersistentFailureDriverResult::Attempted {
                correlation,
                disposition: PersistentFailureInterruptDisposition::CompletionUnknown,
                session_authority_invalidated: true,
            }
        );
    }

    #[test]
    fn published_in_flight_result_is_not_replaced_by_guard_drop() {
        let completion = PersistentFailureResultSlot::new();
        InFlightPersistentFailureResult::new(completion.clone()).publish(
            PersistentFailureDriverResult::NoDispatch(
                PersistentFailureNoDispatchReason::AuthorizationRejected,
            ),
        );

        assert_eq!(
            completion.wait(),
            PersistentFailureDriverResult::NoDispatch(
                PersistentFailureNoDispatchReason::AuthorizationRejected
            )
        );
    }
}
