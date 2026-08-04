use beryl_home_store::HomeStore;
use beryl_model::{
    BerylHomeId, BindingRevision, CasLoadedSessionGeneration, CasThreadId, InputGateRevision,
    RuntimeId, SyndicThreadId,
};

use crate::{
    AdmitCompactionOperation, BindingLifecycle, BindingState, CasRepresentedPrefixProof,
    CompactionAttemptNonce, CompactionMarkerLifecycle, CompactionOperationId,
    CompactionOperationNonce, CompactionOperationRecord, CompactionOperationState,
    CompactionOperationTarget, CompactionRequestDisposition, CompactionThreadStatus,
    InputGateRecord, InputGateState, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    SyndicTimestamp, UsableCasBinding, codec::*, derive_compaction_snapshot_id,
};

mod reconciliation;
mod successor;

/// Fixed-work reconciliation result for one compact-start response observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactionRequestTransitionStatus {
    /// The exact live operation still admits publication of the observation.
    Prior,
    /// The retained request observation proves that publication committed.
    Exact,
    /// Exact provider terminal settlement consumed the operation before the response arrived.
    TerminalAlreadySettled,
    /// Durable state contradicts the request observation.
    Collision,
}

/// Stabilized current admission authority for context compaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompactionAdmissionRead {
    Admissible(Box<CompactionAdmissionCandidate>),
    Existing(Box<CompactionOperationRecord>),
    Ineligible(CompactionAdmissionIneligibility),
}

/// Closed startup classification for one current or retained compaction operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompactionRecoveryCase {
    CancelBeforeDispatch(Box<CompactionOperationRecord>),
    FinishLocalNondispatch(Box<CompactionOperationRecord>),
    RetireRejectedTarget(Box<CompactionOperationRecord>),
    PossibleDispatch(Box<CompactionOperationRecord>),
    FinalizeSuccess(Box<CompactionOperationRecord>),
    FinalizeInterruptedWithIdleEvidence(Box<CompactionOperationRecord>),
    FinalizeFailure(Box<CompactionOperationRecord>),
    Stopping(Box<CompactionOperationRecord>),
    Settled(Box<CompactionOperationRecord>),
}

impl CompactionRecoveryCase {
    #[must_use]
    pub fn record(&self) -> &CompactionOperationRecord {
        match self {
            Self::CancelBeforeDispatch(record)
            | Self::FinishLocalNondispatch(record)
            | Self::RetireRejectedTarget(record)
            | Self::PossibleDispatch(record)
            | Self::FinalizeSuccess(record)
            | Self::FinalizeInterruptedWithIdleEvidence(record)
            | Self::FinalizeFailure(record)
            | Self::Stopping(record)
            | Self::Settled(record) => record,
        }
    }
}

/// Exact valid-binding source from which callers can build one collision-safe admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionAdmissionCandidate {
    home_id: BerylHomeId,
    thread_id: SyndicThreadId,
    source_gate_revision: InputGateRevision,
    binding_revision: BindingRevision,
    usable: UsableCasBinding,
}

impl CompactionAdmissionCandidate {
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn source_gate_revision(&self) -> InputGateRevision {
        self.source_gate_revision
    }
    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }
    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        self.usable.execution().runtime_id()
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        self.usable.cas_thread_id()
    }
    #[must_use]
    pub const fn represented_prefix(&self) -> CasRepresentedPrefixProof {
        self.usable.represented_prefix()
    }

    /// Binds fresh caller-owned operation and attempt identities to this exact stabilized source.
    #[must_use]
    pub fn admission(
        &self,
        operation_nonce: CompactionOperationNonce,
        attempt: CompactionAttemptNonce,
        loaded_generation: CasLoadedSessionGeneration,
        started_at: SyndicTimestamp,
    ) -> AdmitCompactionOperation {
        let operation_id = CompactionOperationId::new(self.thread_id, operation_nonce);
        let turn_id = operation_id.provider_turn_id();
        let snapshot_id = derive_compaction_snapshot_id(
            self.home_id,
            operation_id,
            turn_id,
            self.source_gate_revision,
            self.binding_revision,
            self.usable.represented_prefix(),
            self.usable.cas_thread_id(),
            loaded_generation,
        );
        AdmitCompactionOperation::new(
            self.home_id,
            operation_id,
            CompactionOperationTarget::new(
                self.thread_id,
                turn_id,
                snapshot_id,
                self.binding_revision,
                self.usable.execution().runtime_id(),
                loaded_generation,
                self.usable.cas_thread_id().clone(),
            ),
            self.source_gate_revision,
            attempt,
            started_at,
        )
    }
}

/// Stable reason that the current durable thread cannot admit compaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompactionAdmissionIneligibility {
    MissingThread,
    Busy {
        current_gate: InputGateRecord,
    },
    AcceptedNextEffective {
        current_gate: InputGateRecord,
    },
    NoValidBinding {
        current_gate_revision: InputGateRevision,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AdmissionPass {
    thread: Option<crate::ThreadRecord>,
    gate: Option<InputGateRecord>,
    binding: Option<crate::SyndicCurrentBinding>,
    owner: Option<crate::CasThreadIndexRecord>,
    membership: Option<crate::CasThreadBindingIndexRecord>,
    selected_operation: Option<CompactionOperationRecord>,
}

impl SyndicStorage {
    /// Reads one retained compaction operation by its exact natural identity.
    pub fn compaction_operation(
        &self,
        store: &HomeStore,
        id: CompactionOperationId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<CompactionOperationRecord>, SyndicReadError> {
        self.point::<CompactionOperationsFamily>(store, id, limit)
    }

    /// Reads the immutable gate-transition receipt for one consumed compaction.
    pub fn compaction_settlement_receipt(
        &self,
        store: &HomeStore,
        id: CompactionOperationId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<crate::CompactionSettlementReceiptRecord>, SyndicReadError> {
        self.point::<CompactionSettlementReceiptsFamily>(store, id, limit)
    }

    /// Returns a two-pass-stabilized compaction admission classification.
    pub fn compaction_admission_read(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<CompactionAdmissionRead, SyndicReadError> {
        let first = self.compaction_admission_pass(store, thread_id, limit)?;
        let second = self.compaction_admission_pass(store, thread_id, limit)?;
        if first != second {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "compaction-admission read",
            });
        }
        classify(store.home_id(), first)
    }

    /// Stabilizes one exact operation and classifies the only permitted startup convergence.
    pub fn compaction_recovery_read(
        &self,
        store: &HomeStore,
        operation_id: CompactionOperationId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<CompactionRecoveryCase>, SyndicReadError> {
        let first = self.compaction_recovery_pass(store, operation_id, limit)?;
        let second = self.compaction_recovery_pass(store, operation_id, limit)?;
        if first != second {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "compaction-recovery read",
            });
        }
        first.map(classify_recovery).transpose()
    }

    fn compaction_recovery_pass(
        &self,
        store: &HomeStore,
        operation_id: CompactionOperationId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<(CompactionOperationRecord, Option<InputGateRecord>)>, SyndicReadError> {
        let Some(operation) = self.compaction_operation(store, operation_id, limit)? else {
            return Ok(None);
        };
        let gate = self.input_gate(store, operation_id.thread_id(), limit)?;
        let receipt = self.compaction_settlement_receipt(store, operation_id, limit)?;
        let turn = self.turn(store, operation.target().turn_id(), limit)?;
        let state = self.turn_state(store, operation.target().turn_id(), limit)?;
        let snapshot = self.execution_snapshot(store, operation.target().snapshot_id(), limit)?;
        if turn.as_ref().is_none_or(|turn| {
            turn.id() != operation.target().turn_id()
                || turn.origin_thread_id() != operation.target().thread_id()
        }) || state
            .as_ref()
            .is_none_or(|state| state.turn_id() != operation.target().turn_id())
            || snapshot.as_ref().is_none_or(|snapshot| {
                snapshot.id() != operation.target().snapshot_id()
                    || snapshot.active_turn_id() != operation.target().turn_id()
            })
        {
            return Err(SyndicReadError::Invariant(
                "compaction recovery operation authority is incomplete",
            ));
        }
        if matches!(operation.state(), CompactionOperationState::Consumed(_)) {
            if receipt.as_ref().is_none_or(|receipt| {
                !operation.consumed_receipt_is_exact(receipt)
                    || gate
                        .as_ref()
                        .is_none_or(|gate| !receipt.current_gate_is_descendant(gate))
            }) {
                return Err(SyndicReadError::Invariant(
                    "consumed compaction witness and durable successor disagree",
                ));
            }
        } else if receipt.is_some() {
            return Err(SyndicReadError::Invariant(
                "live compaction has a consumed settlement receipt",
            ));
        }
        Ok(Some((operation, gate)))
    }

    fn compaction_admission_pass(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<AdmissionPass, SyndicReadError> {
        let thread = self.thread(store, thread_id, limit)?;
        let gate = self.input_gate(store, thread_id, limit)?;
        let binding = self.current_binding(store, thread_id, limit)?;
        let (owner, membership) = match binding.as_ref().and_then(|current| {
            if let BindingState::Valid(usable) = current.binding().state() {
                Some((usable.cas_thread_id().clone(), current.binding().revision()))
            } else {
                None
            }
        }) {
            Some((cas_thread, revision)) => (
                self.cas_thread_owner(store, cas_thread.clone(), limit)?,
                self.point::<CasThreadBindingIndexFamily>(
                    store,
                    CasThreadBindingKey::Record(cas_thread, revision),
                    limit,
                )?,
            ),
            None => (None, None),
        };
        let selected_operation = match gate.as_ref().map(InputGateRecord::state) {
            Some(InputGateState::Compacting {
                operation_nonce, ..
            }) => self.compaction_operation(
                store,
                CompactionOperationId::new(thread_id, *operation_nonce),
                limit,
            )?,
            _ => None,
        };
        Ok(AdmissionPass {
            thread,
            gate,
            binding,
            owner,
            membership,
            selected_operation,
        })
    }
}

fn classify_recovery(
    (operation, gate): (CompactionOperationRecord, Option<InputGateRecord>),
) -> Result<CompactionRecoveryCase, SyndicReadError> {
    if let CompactionOperationState::Consumed(_) = operation.state() {
        return Ok(CompactionRecoveryCase::Settled(Box::new(operation)));
    }
    if matches!(operation.state(), CompactionOperationState::Stopping(_)) {
        return Ok(CompactionRecoveryCase::Stopping(Box::new(operation)));
    }
    let gate = gate.ok_or(SyndicReadError::Invariant(
        "live compaction recovery operation has no input gate",
    ))?;
    if gate.state()
        != &InputGateState::compacting(operation.target().turn_id(), operation.id().nonce())
    {
        return Err(SyndicReadError::Invariant(
            "live compaction recovery operation is not gate-selected",
        ));
    }
    if let Some(terminal) = operation.terminal() {
        let record = Box::new(operation);
        return Ok(match terminal.status().outcome() {
            crate::TurnTerminalOutcome::Complete
                if record.marker().is_some_and(|marker| {
                    marker.lifecycle() == CompactionMarkerLifecycle::Completed
                        && marker.sequence() < terminal.sequence()
                }) =>
            {
                CompactionRecoveryCase::FinalizeSuccess(record)
            }
            crate::TurnTerminalOutcome::Interrupted
                if record
                    .status()
                    .is_some_and(|status| status.status() == CompactionThreadStatus::Idle) =>
            {
                CompactionRecoveryCase::FinalizeInterruptedWithIdleEvidence(record)
            }
            _ => CompactionRecoveryCase::FinalizeFailure(record),
        });
    }
    if operation.state() == &CompactionOperationState::Admitted {
        return Ok(CompactionRecoveryCase::CancelBeforeDispatch(Box::new(
            operation,
        )));
    }
    let record = Box::new(operation);
    Ok(
        match record.request().map(|request| request.disposition()) {
            Some(CompactionRequestDisposition::ProvenLocalNondispatch) => {
                CompactionRecoveryCase::FinishLocalNondispatch(record)
            }
            Some(CompactionRequestDisposition::RejectedBeforeCore) => {
                CompactionRecoveryCase::RetireRejectedTarget(record)
            }
            None
            | Some(CompactionRequestDisposition::Accepted)
            | Some(CompactionRequestDisposition::CompletionUnknown) => {
                CompactionRecoveryCase::PossibleDispatch(record)
            }
        },
    )
}

fn classify(
    home_id: BerylHomeId,
    pass: AdmissionPass,
) -> Result<CompactionAdmissionRead, SyndicReadError> {
    let Some(thread) = pass.thread else {
        return Ok(CompactionAdmissionRead::Ineligible(
            CompactionAdmissionIneligibility::MissingThread,
        ));
    };
    let gate = pass.gate.ok_or(SyndicReadError::Invariant(
        "compaction-admission thread has no input gate",
    ))?;
    if let InputGateState::Compacting {
        turn_id,
        operation_nonce,
    } = gate.state()
    {
        let operation = pass.selected_operation.ok_or(SyndicReadError::Invariant(
            "compacting gate selects a missing compaction operation",
        ))?;
        if operation.id() != CompactionOperationId::new(thread.id(), *operation_nonce)
            || operation.target().turn_id() != *turn_id
            || !operation.state().is_live()
        {
            return Err(SyndicReadError::Invariant(
                "compacting gate and compaction operation disagree",
            ));
        }
        return Ok(CompactionAdmissionRead::Existing(Box::new(operation)));
    }
    if gate.state() != &InputGateState::Idle {
        return Ok(CompactionAdmissionRead::Ineligible(
            CompactionAdmissionIneligibility::Busy { current_gate: gate },
        ));
    }
    if gate.live_count() != 0 || gate.live_logical_utf8_bytes() != 0 {
        return Ok(CompactionAdmissionRead::Ineligible(
            CompactionAdmissionIneligibility::AcceptedNextEffective { current_gate: gate },
        ));
    }
    let Some(current) = pass.binding else {
        return Ok(CompactionAdmissionRead::Ineligible(
            CompactionAdmissionIneligibility::NoValidBinding {
                current_gate_revision: gate.revision(),
            },
        ));
    };
    let BindingState::Valid(usable) = current.binding().state() else {
        return Ok(CompactionAdmissionRead::Ineligible(
            CompactionAdmissionIneligibility::NoValidBinding {
                current_gate_revision: gate.revision(),
            },
        ));
    };
    let owner_valid = pass.owner.as_ref().is_some_and(|owner| {
        owner.thread_id() == thread.id()
            && owner.latest_binding_revision() == current.binding().revision()
            && owner.retired_binding_revision().is_none()
    });
    let membership_valid = pass.membership.as_ref().is_some_and(|member| {
        member.cas_thread_id() == usable.cas_thread_id()
            && member.thread_id() == thread.id()
            && member.binding_revision() == current.binding().revision()
    });
    if current.head().lifecycle() != BindingLifecycle::Valid
        || current.binding().thread_id() != thread.id()
        || current.binding().selected_path().tail() != thread.committed_tail()
        || current.binding().selected_path().digest() != thread.selected_path_digest()
        || !owner_valid
        || !membership_valid
    {
        return Err(SyndicReadError::Invariant(
            "valid compaction binding and reverse authority disagree",
        ));
    }
    Ok(CompactionAdmissionRead::Admissible(Box::new(
        CompactionAdmissionCandidate {
            home_id,
            thread_id: thread.id(),
            source_gate_revision: gate.revision(),
            binding_revision: current.binding().revision(),
            usable: usable.clone(),
        },
    )))
}
