use std::collections::HashMap;

use beryl_backend::ExactForegroundTurn;
use beryl_model::{
    CasLoadedSessionGeneration, CasThreadId, CasTurnId, SyndicThreadId, SyndicTurnId,
};

use super::{EventRouter, TargetTurn, stop::VolatileStopAdmissionProof};
use crate::cas_projection::connection::lifecycle::ProjectionConnectionIdentityObservation;
use crate::cas_projection::{
    PendingTurnActivation,
    persistent_failure::{PersistentFailureCutIdentity, PersistentFailureGeneration},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FailureTargetGuardState {
    Frozen,
    Spent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FailureTargetGuard {
    witness: PersistentFailureTargetWitness,
    state: FailureTargetGuardState,
}

#[derive(Debug)]
pub(super) struct PersistentFailureRouterCut {
    identity: PersistentFailureCutIdentity,
    targets: HashMap<CasThreadId, FailureTargetGuard>,
}

/// Exact immutable ordinary-turn proof frozen from last-coherent router evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct PersistentFailureTargetWitness {
    cut_identity: PersistentFailureCutIdentity,
    connection: ProjectionConnectionIdentityObservation,
    registration: u64,
    election_token: u64,
    loaded_generation: CasLoadedSessionGeneration,
    syndic_thread_id: SyndicThreadId,
    syndic_turn_id: Option<SyndicTurnId>,
    cas_thread_id: CasThreadId,
    cas_turn_id: Option<CasTurnId>,
    pending_activation: Option<PendingTurnActivation>,
    request_timeout: std::time::Duration,
}

/// Dispatch proof for one complete eligible failure target.
#[derive(Debug)]
pub(in crate::cas_projection) struct PersistentFailureTargetProof {
    witness: PersistentFailureTargetWitness,
    admission: VolatileStopAdmissionProof,
}

/// Bounded reason why one retained target cannot receive the volatile failure request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum PersistentFailureTargetIneligibility {
    RouterUnavailable,
    ActiveOperation,
    ActiveOnlyRegistration,
    ContextCompaction,
    AwaitingActivation,
    Terminal,
    Closing,
    Lost,
    PublicationInFlight,
    IdentityMismatch,
    PriorPrimaryAmbiguous,
    GenerationMismatch,
    AlreadyFrozen,
}

/// One exact snapshot of the bounded targets frozen by a router failure cut.
#[derive(Debug)]
pub(in crate::cas_projection) struct PersistentFailureTargetBatch {
    candidates: Vec<PersistentFailureTargetCandidate>,
}

impl PersistentFailureTargetBatch {
    pub(in crate::cas_projection) fn into_candidates(
        self,
    ) -> Vec<PersistentFailureTargetCandidate> {
        self.candidates
    }
}

#[derive(Debug)]
pub(in crate::cas_projection) struct PersistentFailureTargetCandidate {
    witness: PersistentFailureTargetWitness,
    proof: Result<PersistentFailureTargetProof, PersistentFailureTargetIneligibility>,
}

impl PersistentFailureTargetCandidate {
    pub(in crate::cas_projection) const fn syndic_thread_id(&self) -> SyndicThreadId {
        self.witness.syndic_thread_id
    }

    pub(in crate::cas_projection) const fn syndic_turn_id(&self) -> Option<SyndicTurnId> {
        self.witness.syndic_turn_id
    }

    pub(in crate::cas_projection) fn cas_thread_id(&self) -> &CasThreadId {
        &self.witness.cas_thread_id
    }

    pub(in crate::cas_projection) const fn witness(&self) -> &PersistentFailureTargetWitness {
        &self.witness
    }

    pub(in crate::cas_projection) fn into_proof(
        self,
    ) -> Result<PersistentFailureTargetProof, PersistentFailureTargetIneligibility> {
        self.proof
    }

    pub(in crate::cas_projection) fn into_parts(
        self,
    ) -> (
        PersistentFailureTargetWitness,
        Result<PersistentFailureTargetProof, PersistentFailureTargetIneligibility>,
    ) {
        (self.witness, self.proof)
    }
}

/// Non-cloneable proof that the driver spent this exact failure target guard.
pub(in crate::cas_projection) struct PersistentFailureDispatchAuthorization {
    proof: PersistentFailureTargetProof,
}

impl PersistentFailureDispatchAuthorization {
    pub(in crate::cas_projection) fn exact_target(&self) -> ExactForegroundTurn {
        self.proof.exact_target()
    }

    pub(in crate::cas_projection) const fn failure_generation(
        &self,
    ) -> PersistentFailureGeneration {
        self.proof.witness.cut_identity.failure_generation
    }

    pub(in crate::cas_projection) const fn request_timeout(&self) -> std::time::Duration {
        self.proof.witness.request_timeout
    }
}

impl PersistentFailureTargetProof {
    pub(in crate::cas_projection) fn exact_target(&self) -> ExactForegroundTurn {
        ExactForegroundTurn::new(
            self.witness.connection.runtime_id(),
            self.witness.loaded_generation,
            self.witness.cas_thread_id.clone(),
            self.witness
                .cas_turn_id
                .clone()
                .expect("eligible failure target retains its exact CAS turn"),
        )
    }

    pub(in crate::cas_projection) const fn failure_generation(
        &self,
    ) -> PersistentFailureGeneration {
        self.witness.cut_identity.failure_generation
    }

    pub(in crate::cas_projection) const fn syndic_thread_id(&self) -> SyndicThreadId {
        self.witness.syndic_thread_id
    }

    pub(in crate::cas_projection) const fn syndic_turn_id(&self) -> SyndicTurnId {
        self.witness
            .syndic_turn_id
            .expect("eligible failure target retains its exact Syndic turn")
    }

    pub(in crate::cas_projection) fn cas_thread_id(&self) -> &CasThreadId {
        &self.witness.cas_thread_id
    }

    pub(in crate::cas_projection) fn cas_turn_id(&self) -> &CasTurnId {
        self.witness
            .cas_turn_id
            .as_ref()
            .expect("eligible failure target retains its exact CAS turn")
    }

    pub(in crate::cas_projection) const fn request_timeout(&self) -> std::time::Duration {
        self.witness.request_timeout
    }

    pub(in crate::cas_projection) const fn witness(&self) -> &PersistentFailureTargetWitness {
        &self.witness
    }
}

impl PersistentFailureTargetWitness {
    pub(in crate::cas_projection) const fn cut_identity(&self) -> PersistentFailureCutIdentity {
        self.cut_identity
    }

    pub(in crate::cas_projection) const fn connection(
        &self,
    ) -> ProjectionConnectionIdentityObservation {
        self.connection
    }

    pub(in crate::cas_projection) const fn registration(&self) -> u64 {
        self.registration
    }

    pub(in crate::cas_projection) const fn election_token(&self) -> u64 {
        self.election_token
    }

    pub(in crate::cas_projection) const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }

    pub(in crate::cas_projection) const fn syndic_thread_id(&self) -> SyndicThreadId {
        self.syndic_thread_id
    }

    pub(in crate::cas_projection) const fn syndic_turn_id(&self) -> Option<SyndicTurnId> {
        self.syndic_turn_id
    }

    pub(in crate::cas_projection) fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }

    pub(in crate::cas_projection) fn cas_turn_id(&self) -> Option<&CasTurnId> {
        self.cas_turn_id.as_ref()
    }

    pub(in crate::cas_projection) const fn pending_activation(
        &self,
    ) -> Option<&PendingTurnActivation> {
        self.pending_activation.as_ref()
    }

    pub(in crate::cas_projection) const fn request_timeout(&self) -> std::time::Duration {
        self.request_timeout
    }
}

impl EventRouter {
    pub(in crate::cas_projection) fn freeze_persistent_failure_targets(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> Result<PersistentFailureTargetBatch, PersistentFailureTargetIneligibility> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PersistentFailureTargetIneligibility::RouterUnavailable)?;
        if state.retired.is_some() || state.persistent_failure.is_some() {
            return Err(PersistentFailureTargetIneligibility::RouterUnavailable);
        }
        if self.commands.service_generation() != identity.service_generation {
            return Err(PersistentFailureTargetIneligibility::GenerationMismatch);
        }
        let keys = sorted_target_keys(&state.targets);
        let mut volatile_admission = state.volatile_stop_admission.take();
        let mut guards = HashMap::with_capacity(keys.len());
        let mut candidates = Vec::with_capacity(keys.len());
        let mut retained_target_projections = Vec::new();
        for (index, cas_thread_id) in keys.into_iter().enumerate() {
            let target = state
                .targets
                .get(&cas_thread_id)
                .expect("bounded failure target remains registered under one router lock");
            let admission = if volatile_admission
                .as_ref()
                .is_some_and(|admission| admission.proof.cas_thread_id == cas_thread_id)
            {
                volatile_admission.take()
            } else {
                None
            };
            let token = admission.as_ref().map_or_else(
                || u64::try_from(index + 1).expect("bounded live-target capacity fits u64"),
                |admission| admission.token,
            );
            let witness = build_witness(self, target, &cas_thread_id, identity, token);
            let proof = build_proof(
                self,
                target,
                &witness,
                admission,
                state.active_steering_attempt.is_some(),
            );
            guards.insert(
                cas_thread_id.clone(),
                FailureTargetGuard {
                    witness: witness.clone(),
                    state: FailureTargetGuardState::Frozen,
                },
            );
            if self.terminal_disposer.is_some()
                && let Some(projection) = state
                    .targets
                    .get_mut(&cas_thread_id)
                    .and_then(|target| target.persistent_failure_projection.take())
            {
                retained_target_projections.push(projection);
            }
            candidates.push(PersistentFailureTargetCandidate { witness, proof });
        }
        state.persistent_failure = Some(PersistentFailureRouterCut {
            identity,
            targets: guards,
        });
        super::state::advance_revision(&mut state);
        drop(state);
        self.publication_changed.notify_all();
        if let Some(disposer) = self.terminal_disposer.clone() {
            for projection in retained_target_projections {
                disposer.dispose_target(projection);
            }
        }
        Ok(PersistentFailureTargetBatch { candidates })
    }

    pub(in crate::cas_projection) fn authorize_persistent_failure_dispatch(
        &self,
        proof: PersistentFailureTargetProof,
    ) -> Result<PersistentFailureDispatchAuthorization, PersistentFailureTargetIneligibility> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PersistentFailureTargetIneligibility::RouterUnavailable)?;
        let cut = state
            .persistent_failure
            .as_ref()
            .ok_or(PersistentFailureTargetIneligibility::GenerationMismatch)?;
        if !proof_matches_router(self, &proof, cut.identity) {
            return Err(PersistentFailureTargetIneligibility::GenerationMismatch);
        }
        let guard = cut
            .targets
            .get(&proof.witness.cas_thread_id)
            .ok_or(PersistentFailureTargetIneligibility::IdentityMismatch)?;
        if guard.witness != proof.witness {
            return Err(PersistentFailureTargetIneligibility::IdentityMismatch);
        }
        if guard.state != FailureTargetGuardState::Frozen {
            return Err(PersistentFailureTargetIneligibility::AlreadyFrozen);
        }
        let target = state
            .targets
            .get(&proof.witness.cas_thread_id)
            .ok_or(PersistentFailureTargetIneligibility::Lost)?;
        validate_frozen_target(target, &proof)?;
        state
            .persistent_failure
            .as_mut()
            .expect("validated failure cut remains installed")
            .targets
            .get_mut(&proof.witness.cas_thread_id)
            .expect("validated failure guard remains installed")
            .state = FailureTargetGuardState::Spent;
        Ok(PersistentFailureDispatchAuthorization { proof })
    }
}

fn build_witness(
    router: &EventRouter,
    target: &super::TargetEntry,
    cas_thread_id: &CasThreadId,
    identity: PersistentFailureCutIdentity,
    election_token: u64,
) -> PersistentFailureTargetWitness {
    PersistentFailureTargetWitness {
        cut_identity: identity,
        connection: ProjectionConnectionIdentityObservation::new(
            router.connection_generation,
            router.runtime_id,
            router.process_generation,
        ),
        registration: target.registration,
        election_token,
        loaded_generation: target.loaded_generation,
        syndic_thread_id: target.owner,
        syndic_turn_id: target
            .pending_activation
            .as_ref()
            .map(PendingTurnActivation::turn_id),
        cas_thread_id: cas_thread_id.clone(),
        cas_turn_id: target.turn_id.clone(),
        pending_activation: target.pending_activation.clone(),
        request_timeout: target.request_timeout,
    }
}

fn build_proof(
    router: &EventRouter,
    target: &super::TargetEntry,
    witness: &PersistentFailureTargetWitness,
    admission: Option<VolatileStopAdmissionProof>,
    active_steering: bool,
) -> Result<PersistentFailureTargetProof, PersistentFailureTargetIneligibility> {
    if target.home_generation != witness.cut_identity.home_generation.get() {
        return Err(PersistentFailureTargetIneligibility::GenerationMismatch);
    }
    if active_steering {
        return Err(PersistentFailureTargetIneligibility::ActiveOperation);
    }
    if target
        .queued_operations
        .load(std::sync::atomic::Ordering::Acquire)
        != 0
        || !target.dynamic_tool_responses.is_empty()
    {
        return Err(PersistentFailureTargetIneligibility::ActiveOperation);
    }
    let admission = admission.ok_or(PersistentFailureTargetIneligibility::PriorPrimaryAmbiguous)?;
    if admission.service_generation != witness.cut_identity.service_generation
        || admission.failure_generation != witness.cut_identity.failure_generation
    {
        return Err(PersistentFailureTargetIneligibility::GenerationMismatch);
    }
    if target.compaction.is_some() {
        return Err(PersistentFailureTargetIneligibility::ContextCompaction);
    }
    if target.turn_state == TargetTurn::Terminal {
        return Err(PersistentFailureTargetIneligibility::Terminal);
    }
    if target.publication_closing.is_some() {
        return Err(PersistentFailureTargetIneligibility::Closing);
    }
    if target.loss_requested {
        return Err(PersistentFailureTargetIneligibility::Lost);
    }
    if target.publication_in_flight.is_some() {
        return Err(PersistentFailureTargetIneligibility::PublicationInFlight);
    }
    if target.turn_state != TargetTurn::Exact
        || !target.start_dispatched
        || !target.activation_durable
    {
        return Err(PersistentFailureTargetIneligibility::AwaitingActivation);
    }
    let pending = target
        .pending_activation
        .as_ref()
        .ok_or(PersistentFailureTargetIneligibility::ActiveOnlyRegistration)?;
    let cas_turn_id = target
        .turn_id
        .as_ref()
        .ok_or(PersistentFailureTargetIneligibility::AwaitingActivation)?;
    if pending.thread_id() != target.owner
        || target.key.runtime_id != router.runtime_id
        || target.key.process_generation != router.process_generation
        || target.loaded_generation.process() != router.process_generation
        || target.key.cas_thread_id != witness.cas_thread_id
    {
        return Err(PersistentFailureTargetIneligibility::IdentityMismatch);
    }
    if !matches!(
        target
            .terminal
            .lock()
            .map(|terminal| *terminal)
            .map_err(|_| PersistentFailureTargetIneligibility::RouterUnavailable)?,
        super::TargetTerminalSignal::Open
    ) {
        return Err(PersistentFailureTargetIneligibility::Terminal);
    }
    if witness.syndic_turn_id != Some(pending.turn_id())
        || witness.cas_turn_id.as_ref() != Some(cas_turn_id)
        || witness.pending_activation.as_ref() != Some(pending)
        || admission.proof.connection_generation != router.connection_generation
        || admission.proof.registration != target.registration
        || admission.proof.runtime_id != router.runtime_id
        || admission.proof.loaded_generation != target.loaded_generation
        || admission.proof.syndic_thread_id != target.owner
        || admission.syndic_turn_id != pending.turn_id()
        || admission.proof.cas_thread_id != witness.cas_thread_id
        || admission.proof.cas_turn_id != *cas_turn_id
    {
        return Err(PersistentFailureTargetIneligibility::IdentityMismatch);
    }
    Ok(PersistentFailureTargetProof {
        witness: witness.clone(),
        admission,
    })
}

fn proof_matches_router(
    router: &EventRouter,
    proof: &PersistentFailureTargetProof,
    identity: PersistentFailureCutIdentity,
) -> bool {
    witness_matches_router(router, &proof.witness, identity)
}

fn witness_matches_router(
    router: &EventRouter,
    witness: &PersistentFailureTargetWitness,
    identity: PersistentFailureCutIdentity,
) -> bool {
    witness.cut_identity == identity
        && witness.connection.connection_generation() == router.connection_generation
        && witness.connection.runtime_id() == router.runtime_id
        && witness.connection.process_generation() == router.process_generation
        && witness.loaded_generation.process() == router.process_generation
}

fn validate_frozen_target(
    target: &super::TargetEntry,
    proof: &PersistentFailureTargetProof,
) -> Result<(), PersistentFailureTargetIneligibility> {
    let witness = &proof.witness;
    if target.registration != witness.registration
        || target.owner != witness.syndic_thread_id
        || target.loaded_generation != witness.loaded_generation
        || target.home_generation != witness.cut_identity.home_generation.get()
        || target.turn_state != TargetTurn::Exact
        || target.turn_id.as_ref() != witness.cas_turn_id.as_ref()
        || !target.start_dispatched
        || !target.activation_durable
        || target.compaction.is_some()
        || target.publication_in_flight.is_some()
        || target.publication_closing.is_some()
        || target.loss_requested
        || target.pending_activation.as_ref() != witness.pending_activation.as_ref()
    {
        return Err(PersistentFailureTargetIneligibility::IdentityMismatch);
    }
    Ok(())
}

fn sorted_target_keys(targets: &HashMap<CasThreadId, super::TargetEntry>) -> Vec<CasThreadId> {
    let mut keys = targets.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    keys
}
