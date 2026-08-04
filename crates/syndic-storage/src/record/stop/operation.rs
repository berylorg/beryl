use super::*;

/// Closed live-or-consumed state of one retained stop operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopOperationState {
    Admitted,
    DispatchClaimed,
    SafeReopened(StopSafeReopenWitness),
    MatchingTerminal(StopMatchingTerminalWitness),
    Abandoned(StopAbandonmentWitness),
}

impl StopOperationState {
    /// Returns the consumed source revisions when this state is inert.
    #[must_use]
    pub const fn disposition_source(self) -> Option<StopDispositionSource> {
        match self {
            Self::Admitted | Self::DispatchClaimed => None,
            Self::SafeReopened(witness) => Some(witness.source()),
            Self::MatchingTerminal(witness) => Some(witness.source()),
            Self::Abandoned(witness) => Some(witness.source()),
        }
    }

    /// Reports whether this state still contributes live stop authority.
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, Self::Admitted | Self::DispatchClaimed)
    }
}

/// Why one stop-operation record is not internally canonical.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum StopOperationRecordError {
    /// The keyed operation identity and immutable target name different threads.
    #[error("stop operation identity and target thread disagree")]
    TargetThreadMismatch,
    /// One cause must retain admission as its first publication.
    #[error("a stop operation requires at least one cause first published at admission")]
    AdmissionCauseMissing,
    /// A cause cannot name a first publication after the current record.
    #[error(
        "{cause:?} first-publication revision {first_revision} is after current revision {current_revision}"
    )]
    CauseFirstRevisionFuture {
        cause: StopCause,
        first_revision: u64,
        current_revision: u64,
    },
    /// An admitted operation cannot already retain a dispatch claim.
    #[error("an admitted stop operation must not carry a dispatch-claim witness")]
    AdmittedClaimPresent,
    /// A dispatch-claimed operation must retain its exact claim witness.
    #[error("a dispatch-claimed stop operation requires a dispatch-claim witness")]
    ClaimedWitnessMissing,
    /// The claim source has no representable publication successor.
    #[error("dispatch-claim source revision {source_revision} has no successor")]
    ClaimSourceExhausted { source_revision: u64 },
    /// The claim publication cannot be after the current record.
    #[error(
        "dispatch-claim publication revision {publication_revision} is after current revision {current_revision}"
    )]
    ClaimPublicationFuture {
        publication_revision: u64,
        current_revision: u64,
    },
    /// Two fixed transitions cannot occupy one post-admission record revision.
    #[error("stop transition revision {revision} is occupied more than once")]
    DuplicateTransitionRevision { revision: u64 },
    /// The bounded occupied revision count must prove a contiguous ledger.
    #[error(
        "stop transition ledger through revision {current_revision} has {occupied_transitions} occupied post-admission revisions"
    )]
    TransitionLedgerGap {
        current_revision: u64,
        occupied_transitions: u64,
    },
    /// Stop admission must publish the immediate gate successor.
    #[error(
        "admission gate revision {successor} is not the immediate successor of source {source_revision}"
    )]
    AdmissionGateRevisionMismatch {
        source_revision: u64,
        successor: u64,
    },
    /// Provider-operation stop admission must publish the immediate compaction successor.
    #[error(
        "admission compaction revision {successor} is not the immediate successor of source {source_revision}"
    )]
    AdmissionCompactionRevisionMismatch {
        source_revision: u64,
        successor: u64,
    },
    /// Stop admission changes the selected generation in place.
    #[error("admission source and stopped route generations disagree")]
    AdmissionRouteGenerationMismatch,
    /// Stop admission must publish the immediate selected-route successor.
    #[error(
        "admission route revision {successor} is not the immediate successor of source {source_revision}"
    )]
    AdmissionRouteRevisionMismatch {
        source_revision: u64,
        successor: u64,
    },
    /// A consumed record must be exactly one revision beyond its named live source.
    #[error(
        "consumed stop revision {successor} is not the immediate successor of source {source_revision}"
    )]
    ConsumedStopRevisionMismatch {
        source_revision: u64,
        successor: u64,
    },
    /// A consumed gate successor must be exactly one revision beyond its named source.
    #[error(
        "consumed gate revision {successor} is not the immediate successor of source {source_revision}"
    )]
    ConsumedGateRevisionMismatch {
        source_revision: u64,
        successor: u64,
    },
    /// A consumed witness must use the variant selected by the immutable target kind.
    #[error("consumed stop witness target kind disagrees with the immutable target")]
    ConsumedTargetKindMismatch,
    /// A provider-operation disposition must retain its immediate compaction successor.
    #[error(
        "consumed compaction revision {successor} is not the immediate successor of source {source_revision}"
    )]
    ConsumedCompactionRevisionMismatch {
        source_revision: u64,
        successor: u64,
    },
}

/// Retained V1 durable stop-operation record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StopOperationRecord {
    id: StopOperationId,
    target: StopOperationTarget,
    admission: StopAdmissionWitness,
    revision: StopOperationRevision,
    cause_first_revisions: StopCauseFirstRevisions,
    dispatch_claim: Option<StopDispatchClaimWitness>,
    state: StopOperationState,
}

impl StopOperationRecord {
    /// Constructs one record after validating its complete fixed transition ledger.
    pub fn new(
        id: StopOperationId,
        target: StopOperationTarget,
        admission: StopAdmissionWitness,
        revision: StopOperationRevision,
        cause_first_revisions: StopCauseFirstRevisions,
        dispatch_claim: Option<StopDispatchClaimWitness>,
        state: StopOperationState,
    ) -> Result<Self, StopOperationRecordError> {
        if id.thread_id() != target.thread_id() {
            return Err(StopOperationRecordError::TargetThreadMismatch);
        }
        if admission.source_gate_revision().get().checked_add(1)
            != Some(admission.successor_gate_revision().get())
        {
            return Err(StopOperationRecordError::AdmissionGateRevisionMismatch {
                source_revision: admission.source_gate_revision().get(),
                successor: admission.successor_gate_revision().get(),
            });
        }
        match (
            admission.source_compaction_revision(),
            admission.successor_compaction_revision(),
        ) {
            (Some(source), Some(successor))
                if source.get().checked_add(1) == Some(successor.get()) => {}
            (Some(source), Some(successor)) => {
                return Err(
                    StopOperationRecordError::AdmissionCompactionRevisionMismatch {
                        source_revision: source.get(),
                        successor: successor.get(),
                    },
                );
            }
            (None, None) => {}
            _ => unreachable!("stop admission exposes both compaction revisions or neither"),
        }
        match (
            admission.source_selected_route_option(),
            admission.successor_stopped_route_option(),
            target.turn_kind(),
        ) {
            (
                Some(source),
                Some(successor),
                TurnKind::OrdinaryUser | TurnKind::BerylLifecycleContinuation,
            ) => {
                if admission.source_compaction_revision().is_some() {
                    return Err(StopOperationRecordError::AdmissionRouteGenerationMismatch);
                }
                if source.generation() != successor.generation() {
                    return Err(StopOperationRecordError::AdmissionRouteGenerationMismatch);
                }
                if source.revision().get().checked_add(1) != Some(successor.revision().get()) {
                    return Err(StopOperationRecordError::AdmissionRouteRevisionMismatch {
                        source_revision: source.revision().get(),
                        successor: successor.revision().get(),
                    });
                }
            }
            (None, None, TurnKind::ProviderOperation(_))
                if admission.source_compaction_revision().is_some() => {}
            _ => return Err(StopOperationRecordError::AdmissionRouteGenerationMismatch),
        }
        match (state, dispatch_claim) {
            (StopOperationState::Admitted, Some(_)) => {
                return Err(StopOperationRecordError::AdmittedClaimPresent);
            }
            (StopOperationState::DispatchClaimed, None) => {
                return Err(StopOperationRecordError::ClaimedWitnessMissing);
            }
            _ => {}
        }
        validate_consumed_target_kind(target.turn_kind(), state)?;
        if let Some(source) = state.disposition_source() {
            if source.stop_revision().get().checked_add(1) != Some(revision.get()) {
                return Err(StopOperationRecordError::ConsumedStopRevisionMismatch {
                    source_revision: source.stop_revision().get(),
                    successor: revision.get(),
                });
            }
            let successor_gate_revision = match state {
                StopOperationState::SafeReopened(witness) => witness.successor_gate_revision(),
                StopOperationState::MatchingTerminal(witness) => witness.successor_gate_revision(),
                StopOperationState::Abandoned(witness) => witness.successor_gate_revision(),
                StopOperationState::Admitted | StopOperationState::DispatchClaimed => {
                    unreachable!("consumed source implies consumed state")
                }
            };
            if source.gate_revision().get().checked_add(1) != Some(successor_gate_revision.get()) {
                return Err(StopOperationRecordError::ConsumedGateRevisionMismatch {
                    source_revision: source.gate_revision().get(),
                    successor: successor_gate_revision.get(),
                });
            }
        }
        validate_transition_ledger(revision, cause_first_revisions, dispatch_claim, state)?;
        Ok(Self {
            id,
            target,
            admission,
            revision,
            cause_first_revisions,
            dispatch_claim,
            state,
        })
    }

    /// Constructs the first admitted revision of one new operation.
    pub fn admitted(
        id: StopOperationId,
        target: StopOperationTarget,
        admission: StopAdmissionWitness,
        causes: StopCauseSet,
    ) -> Result<Self, StopOperationRecordError> {
        Self::new(
            id,
            target,
            admission,
            StopOperationRevision::FIRST,
            StopCauseFirstRevisions::for_admission(causes),
            None,
            StopOperationState::Admitted,
        )
    }

    #[must_use]
    pub const fn id(&self) -> StopOperationId {
        self.id
    }

    #[must_use]
    pub const fn target(&self) -> &StopOperationTarget {
        &self.target
    }

    #[must_use]
    pub const fn admission(&self) -> StopAdmissionWitness {
        self.admission
    }

    #[must_use]
    pub const fn revision(&self) -> StopOperationRevision {
        self.revision
    }

    /// Returns the four persisted cause-first-publication revision slots.
    #[must_use]
    pub const fn cause_first_revisions(&self) -> StopCauseFirstRevisions {
        self.cause_first_revisions
    }

    /// Returns the derived aggregate convenience view of present causes.
    #[must_use]
    pub fn causes(&self) -> StopCauseSet {
        self.cause_first_revisions.causes()
    }

    /// Returns exactly the causes that were present at admission revision one.
    #[must_use]
    pub fn admission_causes(&self) -> StopCauseSet {
        self.cause_first_revisions.admission_causes()
    }

    /// Returns the retained exact dispatch-claim source and attempt, when any.
    #[must_use]
    pub const fn dispatch_claim(&self) -> Option<StopDispatchClaimWitness> {
        self.dispatch_claim
    }

    /// Returns the derived caller-owned attempt identity, when a claim exists.
    #[must_use]
    pub const fn attempt(&self) -> Option<StopAttemptNonce> {
        match self.dispatch_claim {
            Some(claim) => Some(claim.attempt()),
            None => None,
        }
    }

    #[must_use]
    pub const fn state(&self) -> StopOperationState {
        self.state
    }
}

fn validate_consumed_target_kind(
    target_kind: TurnKind,
    state: StopOperationState,
) -> Result<(), StopOperationRecordError> {
    let provider_target = matches!(target_kind, TurnKind::ProviderOperation(_));
    match state {
        StopOperationState::Admitted | StopOperationState::DispatchClaimed => Ok(()),
        StopOperationState::SafeReopened(StopSafeReopenWitness::Ordinary { .. })
        | StopOperationState::MatchingTerminal(StopMatchingTerminalWitness::Ordinary { .. })
        | StopOperationState::Abandoned(StopAbandonmentWitness::Ordinary { .. })
            if !provider_target =>
        {
            Ok(())
        }
        StopOperationState::SafeReopened(StopSafeReopenWitness::ProviderOperation {
            source_compaction_revision,
            successor_compaction_revision,
            ..
        })
        | StopOperationState::MatchingTerminal(StopMatchingTerminalWitness::ProviderOperation {
            source_compaction_revision,
            successor_compaction_revision,
            ..
        })
        | StopOperationState::Abandoned(StopAbandonmentWitness::ProviderOperation {
            source_compaction_revision,
            successor_compaction_revision,
            ..
        }) if provider_target => {
            validate_compaction_successor(source_compaction_revision, successor_compaction_revision)
        }
        _ => Err(StopOperationRecordError::ConsumedTargetKindMismatch),
    }
}

fn validate_compaction_successor(
    source: crate::CompactionOperationRevision,
    successor: crate::CompactionOperationRevision,
) -> Result<(), StopOperationRecordError> {
    if source.checked_next().ok() == Some(successor) {
        Ok(())
    } else {
        Err(
            StopOperationRecordError::ConsumedCompactionRevisionMismatch {
                source_revision: source.get(),
                successor: successor.get(),
            },
        )
    }
}

fn validate_transition_ledger(
    current: StopOperationRevision,
    cause_first_revisions: StopCauseFirstRevisions,
    dispatch_claim: Option<StopDispatchClaimWitness>,
    state: StopOperationState,
) -> Result<(), StopOperationRecordError> {
    if !StopCause::ALL.into_iter().any(|cause| {
        cause_first_revisions.first_revision(cause) == Some(StopOperationRevision::FIRST)
    }) {
        return Err(StopOperationRecordError::AdmissionCauseMissing);
    }

    let mut occupied = [None; 6];
    let mut occupied_count = 0usize;
    for cause in StopCause::ALL {
        let Some(first_revision) = cause_first_revisions.first_revision(cause) else {
            continue;
        };
        if first_revision > current {
            return Err(StopOperationRecordError::CauseFirstRevisionFuture {
                cause,
                first_revision: first_revision.get(),
                current_revision: current.get(),
            });
        }
        if first_revision != StopOperationRevision::FIRST {
            occupy_transition(&mut occupied, &mut occupied_count, first_revision)?;
        }
    }

    if let Some(claim) = dispatch_claim {
        let publication = claim.source_revision().checked_next().map_err(|_| {
            StopOperationRecordError::ClaimSourceExhausted {
                source_revision: claim.source_revision().get(),
            }
        })?;
        if publication > current {
            return Err(StopOperationRecordError::ClaimPublicationFuture {
                publication_revision: publication.get(),
                current_revision: current.get(),
            });
        }
        occupy_transition(&mut occupied, &mut occupied_count, publication)?;
    }

    if state.disposition_source().is_some() {
        occupy_transition(&mut occupied, &mut occupied_count, current)?;
    }

    let expected_count = current.get() - StopOperationRevision::FIRST.get();
    if occupied_count as u64 != expected_count {
        return Err(StopOperationRecordError::TransitionLedgerGap {
            current_revision: current.get(),
            occupied_transitions: occupied_count as u64,
        });
    }
    Ok(())
}

fn occupy_transition(
    occupied: &mut [Option<StopOperationRevision>; 6],
    occupied_count: &mut usize,
    revision: StopOperationRevision,
) -> Result<(), StopOperationRecordError> {
    if occupied[..*occupied_count].contains(&Some(revision)) {
        return Err(StopOperationRecordError::DuplicateTransitionRevision {
            revision: revision.get(),
        });
    }
    occupied[*occupied_count] = Some(revision);
    *occupied_count += 1;
    Ok(())
}
