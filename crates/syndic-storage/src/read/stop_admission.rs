use beryl_home_store::HomeStore;
use beryl_model::{InputGateRevision, SyndicThreadId, SyndicTurnId};

use crate::{
    AcceptedRouteHeadProof, AcceptedRouteTarget, AdmitStopOperation, InputGateState, StopCauseSet,
    StopOperationId, StopOperationNonce, StopOperationTarget, SyndicPointReadLimit,
    SyndicReadError, SyndicStorage, TurnStateRevision,
};

use super::delivery_recovery::{
    DeliveryRecoveryCase, DeliveryRecoveryClassificationError, classifier, facts,
};
use super::stop::{StopObservation, observation_authenticates_record, steerable_target_matches};

/// Closed stabilized stop-admission classification for one Syndic thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StopAdmissionRead {
    /// The current active operation is exact and may be submitted to stop admission.
    Admissible(Box<StopAdmissionCandidate>),
    /// The stopping gate selects one authenticated live durable stop operation.
    Stopping(Box<super::delivery_recovery::SyndicLiveStopOperation>),
    /// The current durable gate does not admit a stop operation.
    Ineligible(StopAdmissionIneligibility),
}

/// Exact current source from which one stop-admission request may be built.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StopAdmissionCandidate {
    target: StopOperationTarget,
    current_gate_revision: InputGateRevision,
    selected_route: Option<AcceptedRouteHeadProof>,
    current_turn_state_revision: TurnStateRevision,
}

impl StopAdmissionCandidate {
    /// Returns the immutable exact provider operation selected for interruption.
    #[must_use]
    pub const fn target(&self) -> &StopOperationTarget {
        &self.target
    }

    /// Returns the current steerable gate revision authenticated by this read.
    #[must_use]
    pub const fn current_gate_revision(&self) -> InputGateRevision {
        self.current_gate_revision
    }

    /// Returns the current selected route generation and revision.
    #[must_use]
    pub const fn selected_route(&self) -> AcceptedRouteHeadProof {
        match self.selected_route {
            Some(route) => route,
            None => panic!("provider-operation stop has no selected route"),
        }
    }

    #[must_use]
    pub const fn selected_route_option(&self) -> Option<AcceptedRouteHeadProof> {
        self.selected_route
    }

    /// Returns the current active turn-state revision authenticated by this read.
    #[must_use]
    pub const fn current_turn_state_revision(&self) -> TurnStateRevision {
        self.current_turn_state_revision
    }

    /// Builds an exact admission request whose operation identity cannot name another thread.
    #[must_use]
    pub fn admission(
        &self,
        operation_nonce: StopOperationNonce,
        causes: StopCauseSet,
    ) -> AdmitStopOperation {
        let operation_id = StopOperationId::new(self.target.thread_id(), operation_nonce);
        match self.selected_route {
            Some(route) => AdmitStopOperation::new(
                operation_id,
                self.target.clone(),
                self.current_gate_revision,
                route,
                causes,
            ),
            None => AdmitStopOperation::provider_operation(
                operation_id,
                self.target.clone(),
                self.current_gate_revision,
                causes,
            ),
        }
    }
}

/// Stable reason why one current thread cannot admit a stop operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopAdmissionIneligibility {
    /// The thread has no blocking operation.
    Idle {
        /// Current idle gate revision.
        current_gate_revision: InputGateRevision,
    },
    /// An ordinary turn is pending and has not become interruptible.
    PendingTurn {
        /// Current blocking turn.
        turn_id: SyndicTurnId,
        /// Current pending gate revision.
        current_gate_revision: InputGateRevision,
    },
    /// The active binding has not published a CAS turn yet.
    AwaitingSteering {
        /// Current blocking turn.
        turn_id: SyndicTurnId,
        /// Current awaiting-steering gate revision.
        current_gate_revision: InputGateRevision,
    },
    /// Dispatch is uncertain and only terminal or abandonment convergence is allowed.
    AwaitingTerminal {
        /// Current blocking turn.
        turn_id: SyndicTurnId,
        /// Current awaiting-terminal gate revision.
        current_gate_revision: InputGateRevision,
    },
    /// Context compaction currently owns the thread operation gate.
    Compacting {
        /// Current blocking provider-operation turn.
        turn_id: SyndicTurnId,
        /// Current compaction gate revision.
        current_gate_revision: InputGateRevision,
    },
    /// A terminal turn still requires history finalization.
    FinalizingHistory {
        /// Current terminal turn.
        turn_id: SyndicTurnId,
        /// Current finalizing gate revision.
        current_gate_revision: InputGateRevision,
    },
    /// A steering request already owns the live target-operation election.
    DeliveringSteering {
        /// Current active turn.
        turn_id: SyndicTurnId,
        /// Current steerable gate revision.
        current_gate_revision: InputGateRevision,
        /// Current selected delivering route.
        selected_route: AcceptedRouteHeadProof,
    },
}

impl StopAdmissionIneligibility {
    /// Returns the exact current input-gate revision behind this classification.
    #[must_use]
    pub const fn current_gate_revision(self) -> InputGateRevision {
        match self {
            Self::Idle {
                current_gate_revision,
            }
            | Self::PendingTurn {
                current_gate_revision,
                ..
            }
            | Self::AwaitingSteering {
                current_gate_revision,
                ..
            }
            | Self::AwaitingTerminal {
                current_gate_revision,
                ..
            }
            | Self::Compacting {
                current_gate_revision,
                ..
            }
            | Self::FinalizingHistory {
                current_gate_revision,
                ..
            }
            | Self::DeliveringSteering {
                current_gate_revision,
                ..
            } => current_gate_revision,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StopAdmissionPass {
    facts: facts::RecoveryFacts,
    target: Option<StopOperationTarget>,
    authority: Option<StopObservation>,
}

impl SyndicStorage {
    /// Returns one closed, two-pass-stabilized stop-admission classification for `thread_id`.
    ///
    /// Every mutable selector consumed by the result is read in two complete coherent passes.
    /// Stable missing or contradictory authority is an invariant failure; selector drift is a
    /// concurrent-change failure. Callers never need to stitch gate, route, binding, snapshot,
    /// CAS-turn, turn-state, or live-stop records themselves.
    pub fn stop_admission_read(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<StopAdmissionRead, SyndicReadError> {
        let first = read_pass(self, store, thread_id, limit)?;
        let second = read_pass(self, store, thread_id, limit)?;
        if first != second {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "stop-admission read",
            });
        }
        classify_current(thread_id, first)
    }
}

fn read_pass(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread_id: SyndicThreadId,
    limit: SyndicPointReadLimit,
) -> Result<StopAdmissionPass, SyndicReadError> {
    let facts = facts::read(storage, store, thread_id, limit)?;
    let discovered = discover_target(&facts);
    let target = discovered.as_ref().map(|(_, target)| target.clone());
    let authority =
        match (discovered.as_ref(), target.as_ref()) {
            (Some((Some(operation_id), _)), Some(target)) => Some(
                StopObservation::read_current_stop(storage, store, *operation_id, target, limit)?,
            ),
            (Some((None, _)), Some(target)) => Some(StopObservation::read_current_target(
                storage, store, thread_id, target, limit,
            )?),
            _ => None,
        };
    Ok(StopAdmissionPass {
        facts,
        target,
        authority,
    })
}

fn classify_current(
    thread_id: SyndicThreadId,
    pass: StopAdmissionPass,
) -> Result<StopAdmissionRead, SyndicReadError> {
    let StopAdmissionPass {
        facts,
        target,
        authority,
    } = pass;
    let gate = required(
        facts.gate.as_ref(),
        "stop-admission thread has no input gate",
    )?;
    if matches!(gate.state(), InputGateState::Stopping { .. })
        && target.as_ref().is_some_and(|target| {
            target.turn_kind()
                == crate::TurnKind::ProviderOperation(
                    crate::ProviderOperationKind::ContextCompaction,
                )
        })
    {
        let target = required(target.as_ref(), "provider-stop admission target is missing")?;
        let observed = required(
            authority.as_ref(),
            "provider-stop admission authority observation is missing",
        )?;
        return super::delivery_recovery::stopping::provider::classify(&facts, target, observed)
            .map(Box::new)
            .map(StopAdmissionRead::Stopping)
            .map_err(read_error);
    }
    let classified = classifier::classify(thread_id, &facts).map_err(read_error)?;
    match classified {
        DeliveryRecoveryCase::Stopping(live) => {
            let observed = required(
                authority.as_ref(),
                "live stop-admission authority observation is missing",
            )?;
            if !observation_authenticates_record(observed)
                || observed.stop.as_ref() != Some(live.record())
                || observed.gate.as_ref().map(crate::InputGateRecord::revision)
                    != Some(live.current_gate_revision())
                || observed
                    .turn_state
                    .as_ref()
                    .map(crate::TurnStateRecord::revision)
                    != Some(live.current_state_revision())
                || observed.snapshot.as_ref() != Some(live.snapshot())
            {
                return invariant("live stop-admission authority disagrees");
            }
            Ok(StopAdmissionRead::Stopping(live))
        }
        DeliveryRecoveryCase::Active(_) => match gate.state() {
            InputGateState::AwaitingSteering(turn_id) => Ok(StopAdmissionRead::Ineligible(
                StopAdmissionIneligibility::AwaitingSteering {
                    turn_id: *turn_id,
                    current_gate_revision: gate.revision(),
                },
            )),
            InputGateState::AwaitingTerminal(turn_id) => Ok(StopAdmissionRead::Ineligible(
                StopAdmissionIneligibility::AwaitingTerminal {
                    turn_id: *turn_id,
                    current_gate_revision: gate.revision(),
                },
            )),
            InputGateState::Steerable(turn_id) => {
                classify_steerable(target.as_ref(), authority.as_ref(), gate, *turn_id)
            }
            _ => invariant("active stop-admission authority disagrees with its input gate"),
        },
        DeliveryRecoveryCase::Pending {
            turn_id,
            thread_id: _,
            minimum_timestamp: _,
        }
        | DeliveryRecoveryCase::PostAbandonment {
            turn_id,
            thread_id: _,
            minimum_timestamp: _,
        } => Ok(StopAdmissionRead::Ineligible(
            StopAdmissionIneligibility::PendingTurn {
                turn_id,
                current_gate_revision: gate.revision(),
            },
        )),
        DeliveryRecoveryCase::FinalizingHistory {
            turn_id,
            thread_id: _,
            minimum_timestamp: _,
        } => Ok(StopAdmissionRead::Ineligible(
            StopAdmissionIneligibility::FinalizingHistory {
                turn_id,
                current_gate_revision: gate.revision(),
            },
        )),
        DeliveryRecoveryCase::DeferredCompaction {
            turn_id,
            thread_id: _,
        } => classify_compacting(&facts, target.as_ref(), gate, turn_id),
        DeliveryRecoveryCase::Settled { thread_id: _ } => Ok(StopAdmissionRead::Ineligible(
            StopAdmissionIneligibility::Idle {
                current_gate_revision: gate.revision(),
            },
        )),
    }
}

fn discover_target(
    facts: &facts::RecoveryFacts,
) -> Option<(Option<StopOperationId>, StopOperationTarget)> {
    let gate = facts.gate.as_ref()?;
    match gate.state() {
        InputGateState::Stopping {
            turn_id: _,
            operation_nonce,
        } => {
            let stop = facts.stop.as_ref()?;
            Some((
                Some(StopOperationId::new(gate.thread_id(), *operation_nonce)),
                stop.target().clone(),
            ))
        }
        InputGateState::Steerable(turn_id) => {
            let turn = facts.turn.as_ref()?;
            let snapshot = facts.snapshot.as_ref()?;
            let route = facts.route.as_ref()?;
            let AcceptedRouteTarget::Steering(steering) = route.target() else {
                return None;
            };
            Some((
                None,
                StopOperationTarget::new(
                    gate.thread_id(),
                    *turn_id,
                    turn.kind(),
                    snapshot.binding_revision(),
                    snapshot.id(),
                    snapshot.execution().runtime_id(),
                    snapshot.loaded_generation(),
                    snapshot.cas_thread_id().clone(),
                    steering.cas_turn_id().clone(),
                ),
            ))
        }
        InputGateState::Compacting { turn_id, .. } => {
            let operation = facts.compaction.as_ref()?;
            let cas_turn = operation.cas_turn()?;
            Some((
                None,
                StopOperationTarget::new(
                    gate.thread_id(),
                    *turn_id,
                    crate::TurnKind::ProviderOperation(
                        crate::ProviderOperationKind::ContextCompaction,
                    ),
                    operation.target().binding_revision(),
                    operation.target().snapshot_id(),
                    operation.target().runtime_id(),
                    operation.target().loaded_generation(),
                    operation.target().cas_thread_id().clone(),
                    cas_turn.cas_turn_id().clone(),
                ),
            ))
        }
        InputGateState::Idle
        | InputGateState::PendingTurn(_)
        | InputGateState::AwaitingSteering(_)
        | InputGateState::AwaitingTerminal(_)
        | InputGateState::FinalizingHistory(_) => None,
    }
}

fn classify_compacting(
    facts: &facts::RecoveryFacts,
    target: Option<&StopOperationTarget>,
    gate: &crate::InputGateRecord,
    turn_id: SyndicTurnId,
) -> Result<StopAdmissionRead, SyndicReadError> {
    let Some(target) = target else {
        return Ok(StopAdmissionRead::Ineligible(
            StopAdmissionIneligibility::Compacting {
                turn_id,
                current_gate_revision: gate.revision(),
            },
        ));
    };
    let operation = required(
        facts.compaction.as_ref(),
        "compacting stop-admission operation is missing",
    )?;
    let snapshot = required(
        facts.snapshot.as_ref(),
        "compacting stop-admission snapshot is missing",
    )?;
    let active = required(
        facts.active_turn.as_ref(),
        "compacting stop-admission CAS turn is missing",
    )?;
    let binding = required(
        facts.binding.as_ref(),
        "compacting stop-admission binding is missing",
    )?;
    let crate::BindingState::Valid(usable) = binding.binding().state() else {
        return invariant("compacting stop-admission binding is not valid");
    };
    if operation.target().turn_id() != turn_id
        || !matches!(
            operation.state(),
            crate::CompactionOperationState::DispatchClaimed
                | crate::CompactionOperationState::Live
        )
        || operation
            .cas_turn()
            .is_none_or(|observed| observed.cas_turn_id() != target.cas_turn_id())
        || snapshot.id() != target.snapshot_id()
        || snapshot.active_turn_id() != turn_id
        || snapshot.kind()
            != crate::ExecutionSnapshotKind::ProviderOperation(
                crate::ProviderOperationKind::ContextCompaction,
            )
        || active.cas_turn_id() != target.cas_turn_id()
        || usable.cas_thread_id() != target.cas_thread_id()
    {
        return invariant("compacting stop-admission authority disagrees");
    }
    Ok(StopAdmissionRead::Admissible(Box::new(
        StopAdmissionCandidate {
            target: target.clone(),
            current_gate_revision: gate.revision(),
            selected_route: None,
            current_turn_state_revision: required(
                facts.state.as_ref(),
                "compacting stop-admission turn state is missing",
            )?
            .revision(),
        },
    )))
}

fn classify_steerable(
    target: Option<&StopOperationTarget>,
    observed: Option<&StopObservation>,
    gate: &crate::InputGateRecord,
    turn_id: SyndicTurnId,
) -> Result<StopAdmissionRead, SyndicReadError> {
    let target = required(target, "steerable stop-admission target is missing")?;
    let observed = required(
        observed,
        "steerable stop-admission authority observation is missing",
    )?;
    if observed.gate.as_ref() != Some(gate) || !steerable_target_matches(observed, target) {
        return invariant("steerable stop-admission authority disagrees");
    }
    let route = required(
        observed.route.as_ref(),
        "steerable stop-admission route is missing",
    )?;
    let state = required(
        observed.turn_state.as_ref(),
        "steerable stop-admission turn state is missing",
    )?;
    let selected_route = gate.selected_route().ok_or(SyndicReadError::Invariant(
        "steerable stop-admission gate has no selected route",
    ))?;
    if route.delivering_count() != 0 {
        return Ok(StopAdmissionRead::Ineligible(
            StopAdmissionIneligibility::DeliveringSteering {
                turn_id,
                current_gate_revision: gate.revision(),
                selected_route,
            },
        ));
    }
    Ok(StopAdmissionRead::Admissible(Box::new(
        StopAdmissionCandidate {
            target: target.clone(),
            current_gate_revision: gate.revision(),
            selected_route: Some(selected_route),
            current_turn_state_revision: state.revision(),
        },
    )))
}

fn read_error(error: DeliveryRecoveryClassificationError) -> SyndicReadError {
    match error {
        DeliveryRecoveryClassificationError::Read(source) => source,
        DeliveryRecoveryClassificationError::SourceDrift => SyndicReadError::ConcurrentChange {
            operation: "stop-admission read",
        },
        DeliveryRecoveryClassificationError::Corruption(message) => {
            SyndicReadError::Invariant(message)
        }
    }
}

fn required<'a, T>(value: Option<&'a T>, message: &'static str) -> Result<&'a T, SyndicReadError> {
    value.ok_or(SyndicReadError::Invariant(message))
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicReadError> {
    Err(SyndicReadError::Invariant(message))
}
