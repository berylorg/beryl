use beryl_home_store::HomeStore;
use beryl_model::{BindingRevision, CasNativeTurnCount, ExecutionBinding, InputGateRevision};

use super::*;
use crate::{BindingState, CasLineageProof, InputGateRecord, NativeCasLineage};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecoverySourceAnchor {
    pub(crate) thread_id: SyndicThreadId,
    pub(crate) binding_revision: BindingRevision,
    pub(crate) selected_path: SelectedPathProof,
    pub(crate) current_selected_path: SelectedPathProof,
    pub(crate) current_binding_revision: BindingRevision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryOperation {
    Current,
    Fresh,
    Resume,
    Fork,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryAnchor {
    pub(crate) gate_revision: InputGateRevision,
    pub(crate) source: Option<RecoverySourceAnchor>,
    pub(crate) operation: RecoveryOperation,
    pub(crate) native_turn_count: CasNativeTurnCount,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeProjectionRecoveryBasis {
    native_basis: NativeProjectionBasis,
    execution: ExecutionBinding,
    gate: InputGateRecord,
    source: Option<NativeProjectionSource>,
}

impl NativeProjectionRecoveryBasis {
    #[must_use]
    pub const fn native_basis(&self) -> NativeProjectionBasis {
        self.native_basis
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.gate.revision()
    }

    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }

    #[must_use]
    pub const fn source(&self) -> Option<&NativeProjectionSource> {
        self.source.as_ref()
    }

    fn accepts_successor(&self, current: &Self) -> bool {
        let Some(anchor) = self.native_basis.recovery else {
            return false;
        };
        let Some(source) = current.source() else {
            return false;
        };
        let usable = source.binding();
        if usable.native_turn_count() != anchor.native_turn_count {
            return false;
        }
        let expected = match anchor.operation {
            RecoveryOperation::Fresh => CasLineageProof::native(
                NativeCasLineage::Fresh,
                self.native_basis.represented_prefix(),
            ),
            RecoveryOperation::Fork => CasLineageProof::native(
                NativeCasLineage::Fork,
                self.native_basis.represented_prefix(),
            ),
            RecoveryOperation::Current | RecoveryOperation::Resume => {
                let Some(original) = self.source() else {
                    return false;
                };
                if usable.cas_thread_id() != original.binding().cas_thread_id() {
                    return false;
                }
                if usable.lineage() == original.binding().lineage() {
                    return true;
                }
                CasLineageProof::native(
                    NativeCasLineage::Resume,
                    original.binding().represented_prefix(),
                )
            }
        };
        expected.ok() == Some(usable.lineage())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeProjectionRecoveryUnavailable {
    Active,
    UnknownTerminal,
    RepairPending,
    UnsupportedContext,
    PendingPrefixUnavailable,
    Source(NativeProjectionUnavailable),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeProjectionRecoveryPlan {
    Ready {
        plan: NativeProjectionPlan,
        basis: NativeProjectionRecoveryBasis,
    },
    Unavailable(NativeProjectionRecoveryUnavailable),
}

impl SyndicStorage {
    pub fn prepare_native_projection_recovery(
        &self,
        store: &HomeStore,
        request: &NativeProjectionRequest,
        limit: SyndicPointReadLimit,
    ) -> Result<NativeProjectionRecoveryPlan, NativeProjectionError> {
        let before = self.revision(store)?;
        let result = self.prepare_native_projection_recovery_inner(store, request, limit)?;
        if self.revision(store)? != before {
            return Err(NativeProjectionError::ConcurrentChange);
        }
        Ok(result)
    }

    fn prepare_native_projection_recovery_inner(
        &self,
        store: &HomeStore,
        request: &NativeProjectionRequest,
        limit: SyndicPointReadLimit,
    ) -> Result<NativeProjectionRecoveryPlan, NativeProjectionError> {
        use NativeProjectionRecoveryUnavailable as Unavailable;
        let thread = self.thread(store, request.thread_id, limit)?.ok_or(
            NativeProjectionError::Invariant("recovery projection thread is missing"),
        )?;
        let selected = SelectedPathProof::new(
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
        );
        if selected != request.selected_path {
            return Err(NativeProjectionError::StaleSelectedPath);
        }
        if thread.context_owner_id().is_some() {
            return Ok(NativeProjectionRecoveryPlan::Unavailable(
                Unavailable::UnsupportedContext,
            ));
        }
        let execution = self
            .thread_execution(store, request.thread_id, limit)?
            .ok_or(NativeProjectionError::Invariant(
                "recovery projection execution is missing",
            ))?;
        if execution.execution() != request.execution() {
            return Ok(NativeProjectionRecoveryPlan::Unavailable(
                Unavailable::Source(NativeProjectionUnavailable::SourceExecutionMismatch),
            ));
        }
        let current = self
            .current_binding(store, request.thread_id, limit)?
            .ok_or(NativeProjectionError::Invariant(
                "recovery projection binding is missing",
            ))?;
        let gate = self.input_gate(store, request.thread_id, limit)?.ok_or(
            NativeProjectionError::Invariant("recovery projection gate is missing"),
        )?;
        if let Some(tail) = selected.tail() {
            let state =
                self.turn_state(store, tail, limit)?
                    .ok_or(NativeProjectionError::Invariant(
                        "recovery selected state is missing",
                    ))?;
            if state.lifecycle() == TurnLifecycle::UnknownTerminal {
                return Ok(NativeProjectionRecoveryPlan::Unavailable(
                    Unavailable::UnknownTerminal,
                ));
            }
        }
        let unavailable = match gate.state() {
            InputGateState::RepairRequired(_) => Some(Unavailable::RepairPending),
            InputGateState::Idle | InputGateState::PendingTurn(_) => None,
            _ => Some(Unavailable::Active),
        };
        if let Some(reason) = unavailable {
            return Ok(NativeProjectionRecoveryPlan::Unavailable(reason));
        }
        if matches!(current.binding().state(), BindingState::Active(_)) {
            return Ok(NativeProjectionRecoveryPlan::Unavailable(
                Unavailable::Active,
            ));
        }
        let prefix_tail = match gate.state() {
            InputGateState::Idle => selected.tail(),
            InputGateState::PendingTurn(pending_id) => {
                if Some(*pending_id) != selected.tail() {
                    return Err(NativeProjectionError::Invariant(
                        "recovery pending gate and selected tail disagree",
                    ));
                }
                let pending = self.turn(store, *pending_id, limit)?.ok_or(
                    NativeProjectionError::Invariant("recovery pending turn is missing"),
                )?;
                if pending.origin_thread_id() != request.thread_id
                    || pending.chain_digest() != selected.digest()
                {
                    return Err(NativeProjectionError::Invariant(
                        "recovery pending identity and selected path disagree",
                    ));
                }
                match pending.parent().turn() {
                    Some(parent_id) => {
                        let parent = self.turn(store, parent_id, limit)?.ok_or(
                            NativeProjectionError::Invariant("recovery pending parent is missing"),
                        )?;
                        if parent.depth().get().checked_add(1) != Some(pending.depth().get())
                            || pending.chain_digest()
                                != crate::child_turn_chain_digest(
                                    pending.id(),
                                    parent_id,
                                    parent.chain_digest(),
                                )
                        {
                            return Err(NativeProjectionError::Invariant(
                                "recovery pending parent proof disagrees",
                            ));
                        }
                    }
                    None => {
                        if pending.depth() != crate::TurnDepth::FIRST
                            || pending.chain_digest() != crate::root_turn_chain_digest(pending.id())
                        {
                            return Err(NativeProjectionError::Invariant(
                                "recovery pending root proof disagrees",
                            ));
                        }
                    }
                }
                let state = self.turn_state(store, *pending_id, limit)?.ok_or(
                    NativeProjectionError::Invariant("recovery pending state is missing"),
                )?;
                if state.lifecycle() != TurnLifecycle::Pending
                    || !matches!(
                        pending.kind(),
                        TurnKind::OrdinaryUser
                            | TurnKind::BerylLifecycleContinuation
                            | TurnKind::BerylDiscussionHandoff
                    )
                {
                    return Ok(NativeProjectionRecoveryPlan::Unavailable(
                        Unavailable::PendingPrefixUnavailable,
                    ));
                }
                pending.parent().turn()
            }
            _ => unreachable!(),
        };
        let prefix_digest = match prefix_tail {
            Some(tail) => {
                let turn =
                    self.turn(store, tail, limit)?
                        .ok_or(NativeProjectionError::Invariant(
                            "recovery represented turn is missing",
                        ))?;
                if matches!(gate.state(), InputGateState::Idle)
                    && turn.chain_digest() != selected.digest()
                {
                    return Err(NativeProjectionError::Invariant(
                        "recovery committed prefix and selected path disagree",
                    ));
                }
                let state = self.turn_state(store, tail, limit)?.ok_or(
                    NativeProjectionError::Invariant("recovery represented state is missing"),
                )?;
                if !state.lifecycle().is_proven_terminal() {
                    let reason = if state.lifecycle() == TurnLifecycle::UnknownTerminal {
                        Unavailable::UnknownTerminal
                    } else {
                        Unavailable::Active
                    };
                    return Ok(NativeProjectionRecoveryPlan::Unavailable(reason));
                }
                turn.chain_digest()
            }
            None => empty_selected_path_digest(),
        };
        let mut native_basis = NativeProjectionBasis {
            thread_id: request.thread_id,
            expected_binding_revision: current.binding().revision(),
            selected_path: selected,
            represented_prefix: CasRepresentedPrefixProof::new(
                prefix_tail,
                selected.thread_revision(),
                prefix_digest,
            ),
            tool_profile: request.tool_profile,
            recovery: None,
        };
        let plan = self.classify_native_projection(
            store,
            request,
            current.binding(),
            thread.parent_thread_id(),
            native_basis,
            limit,
        )?;
        let (operation, source, native_turn_count) = match &plan {
            NativeProjectionPlan::Current { source, .. } => (
                RecoveryOperation::Current,
                Some(source.clone()),
                source.binding().native_turn_count(),
            ),
            NativeProjectionPlan::Fresh { .. } => {
                (RecoveryOperation::Fresh, None, CasNativeTurnCount::ZERO)
            }
            NativeProjectionPlan::Resume { source, .. } => (
                RecoveryOperation::Resume,
                Some(source.clone()),
                source.binding().native_turn_count(),
            ),
            NativeProjectionPlan::Fork {
                source,
                native_turn_count,
                ..
            } => (
                RecoveryOperation::Fork,
                Some(source.clone()),
                *native_turn_count,
            ),
            NativeProjectionPlan::Unavailable { reason, .. } => {
                return Ok(NativeProjectionRecoveryPlan::Unavailable(
                    Unavailable::Source(*reason),
                ));
            }
        };
        let source_anchor = if let Some(source) = &source {
            let source_thread = self.thread(store, source.thread_id(), limit)?.ok_or(
                NativeProjectionError::Invariant("recovery source thread is missing"),
            )?;
            let source_current = self
                .current_binding(store, source.thread_id(), limit)?
                .ok_or(NativeProjectionError::Invariant(
                    "recovery source current binding is missing",
                ))?;
            Some(RecoverySourceAnchor {
                thread_id: source.thread_id(),
                binding_revision: source.binding_revision(),
                selected_path: source.selected_path(),
                current_selected_path: SelectedPathProof::new(
                    source_thread.committed_tail(),
                    source_thread.revision(),
                    source_thread.selected_path_digest(),
                ),
                current_binding_revision: source_current.binding().revision(),
            })
        } else {
            None
        };
        native_basis.recovery = Some(RecoveryAnchor {
            gate_revision: gate.revision(),
            source: source_anchor,
            operation,
            native_turn_count,
        });
        let plan = match plan {
            NativeProjectionPlan::Current { source, .. } => NativeProjectionPlan::Current {
                basis: native_basis,
                source,
            },
            NativeProjectionPlan::Fresh { .. } => NativeProjectionPlan::Fresh {
                basis: native_basis,
            },
            NativeProjectionPlan::Resume { source, .. } => NativeProjectionPlan::Resume {
                basis: native_basis,
                source,
            },
            NativeProjectionPlan::Fork {
                source,
                through_turn,
                native_turn_count,
                ..
            } => NativeProjectionPlan::Fork {
                basis: native_basis,
                source,
                through_turn,
                native_turn_count,
            },
            NativeProjectionPlan::Unavailable { .. } => unreachable!(),
        };
        Ok(NativeProjectionRecoveryPlan::Ready {
            plan,
            basis: NativeProjectionRecoveryBasis {
                native_basis,
                execution: request.execution.clone(),
                gate,
                source,
            },
        })
    }

    pub fn validate_native_projection_recovery_basis(
        &self,
        store: &HomeStore,
        basis: &NativeProjectionRecoveryBasis,
        limit: SyndicPointReadLimit,
    ) -> Result<bool, NativeProjectionError> {
        let native = basis.native_basis;
        let request = NativeProjectionRequest::new(
            native.thread_id(),
            native.selected_path(),
            basis.execution.clone(),
            native.tool_profile(),
        );
        match self.prepare_native_projection_recovery(store, &request, limit) {
            Ok(NativeProjectionRecoveryPlan::Ready { basis: current, .. }) => Ok(current == *basis),
            Ok(NativeProjectionRecoveryPlan::Unavailable(_))
            | Err(NativeProjectionError::StaleSelectedPath) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub fn native_projection_recovery_successor_basis(
        &self,
        store: &HomeStore,
        basis: &NativeProjectionRecoveryBasis,
        binding_revision: BindingRevision,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<NativeProjectionRecoveryBasis>, NativeProjectionError> {
        let native = basis.native_basis;
        let same_binding = binding_revision == native.expected_binding_revision();
        let next_binding =
            native.expected_binding_revision().checked_next().ok() == Some(binding_revision);
        if !(next_binding
            || same_binding
                && native
                    .recovery
                    .is_some_and(|anchor| anchor.operation == RecoveryOperation::Current))
        {
            return Ok(None);
        }
        let request = NativeProjectionRequest::new(
            native.thread_id(),
            native.selected_path(),
            basis.execution.clone(),
            native.tool_profile(),
        );
        match self.prepare_native_projection_recovery(store, &request, limit) {
            Ok(NativeProjectionRecoveryPlan::Ready {
                plan: NativeProjectionPlan::Current { .. },
                basis: current,
            }) if current.gate == basis.gate
                && current.native_basis.expected_binding_revision() == binding_revision
                && if same_binding {
                    current == *basis
                } else {
                    basis.accepts_successor(&current)
                } =>
            {
                Ok(Some(current))
            }
            Ok(_) | Err(NativeProjectionError::StaleSelectedPath) => Ok(None),
            Err(error) => Err(error),
        }
    }
}
