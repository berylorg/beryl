use beryl_home_store::HomeStore;
use beryl_model::{ExecutionBinding, SyndicThreadId, SyndicTurnId};

use crate::{
    BindingState, CasRepresentedPrefixProof, SourceEventPayload, SourceEventSequence,
    SyndicPointReadLimit, SyndicStorage, UsableCasBinding,
};

use super::{
    NativeProjectionBasis, NativeProjectionError, NativeProjectionPlan, NativeProjectionRequest,
    NativeProjectionSource, NativeProjectionUnavailable,
};

impl SyndicStorage {
    pub(super) fn classify_native_projection(
        &self,
        store: &HomeStore,
        request: &NativeProjectionRequest,
        current: &crate::BindingRecord,
        parent_thread_id: Option<SyndicThreadId>,
        basis: NativeProjectionBasis,
        limit: SyndicPointReadLimit,
    ) -> Result<NativeProjectionPlan, NativeProjectionError> {
        if let Some(usable) = usable_binding(current.state())
            && usable.represented_prefix() == basis.represented_prefix()
        {
            let source = NativeProjectionSource {
                thread_id: request.thread_id,
                binding_revision: current.revision(),
                selected_path: current.selected_path(),
                binding: usable.clone(),
            };
            if usable.execution() != request.execution() {
                return Ok(NativeProjectionPlan::Unavailable {
                    basis,
                    source: Some(source),
                    reason: NativeProjectionUnavailable::SourceExecutionMismatch,
                });
            }
            if usable.tool_profile() != request.tool_profile() {
                return Ok(NativeProjectionPlan::Unavailable {
                    basis,
                    source: Some(source),
                    reason: NativeProjectionUnavailable::SourceToolProfileMismatch,
                });
            }
            return Ok(NativeProjectionPlan::Current { basis, source });
        }
        let Some(target_turn_id) = basis.represented_prefix().tail() else {
            return Ok(NativeProjectionPlan::Fresh { basis });
        };

        if let Some(source) = self.source_for_correlated_turn(
            store,
            target_turn_id,
            basis.represented_prefix(),
            limit,
        )? {
            let target_count = source.target_native_turn_count;
            let source_record = NativeProjectionSource {
                thread_id: source.thread_id,
                binding_revision: source.binding_revision,
                selected_path: source.selected_path,
                binding: source.binding,
            };
            if source_record.binding.execution() != request.execution() {
                return Ok(NativeProjectionPlan::Unavailable {
                    basis,
                    source: target_owned_source(request.thread_id, &source_record),
                    reason: NativeProjectionUnavailable::SourceExecutionMismatch,
                });
            }
            if source_record.binding.tool_profile() != request.tool_profile() {
                return Ok(NativeProjectionPlan::Unavailable {
                    basis,
                    source: target_owned_source(request.thread_id, &source_record),
                    reason: NativeProjectionUnavailable::SourceToolProfileMismatch,
                });
            }
            if !self.source_prefix_contains_target(
                store,
                &source_record.binding,
                target_turn_id,
                limit,
            )? {
                return Ok(NativeProjectionPlan::Unavailable {
                    basis,
                    source: target_owned_source(request.thread_id, &source_record),
                    reason: NativeProjectionUnavailable::SourcePrefixMismatch,
                });
            }

            if source_record.thread_id == request.thread_id
                && prefix_position_matches(
                    source_record.binding.represented_prefix(),
                    basis.represented_prefix(),
                )
            {
                return Ok(NativeProjectionPlan::Resume {
                    basis,
                    source: source_record,
                });
            }
            return Ok(NativeProjectionPlan::Fork {
                basis,
                source: source_record,
                through_turn: Some(source.cas_turn_id),
                native_turn_count: target_count,
            });
        }

        if let Some(parent_thread_id) = parent_thread_id
            && let Some(source) = self.full_fork_source(
                store,
                parent_thread_id,
                request.execution(),
                basis.represented_prefix(),
                limit,
            )?
        {
            if source.binding.tool_profile() != request.tool_profile() {
                return Ok(NativeProjectionPlan::Unavailable {
                    basis,
                    source: None,
                    reason: NativeProjectionUnavailable::SourceToolProfileMismatch,
                });
            }
            let native_turn_count = source.binding.native_turn_count();
            return Ok(NativeProjectionPlan::Fork {
                basis,
                source,
                through_turn: None,
                native_turn_count,
            });
        }

        Ok(NativeProjectionPlan::Unavailable {
            basis,
            source: None,
            reason: NativeProjectionUnavailable::MissingCasTurnCorrelation,
        })
    }

    fn source_prefix_contains_target(
        &self,
        store: &HomeStore,
        source: &UsableCasBinding,
        target_turn_id: SyndicTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<bool, NativeProjectionError> {
        let Some(source_tail) = source.represented_prefix().tail() else {
            return Ok(false);
        };
        let outer =
            self.turn(store, source_tail, limit)?
                .ok_or(NativeProjectionError::Invariant(
                    "source represented-prefix tail is missing",
                ))?;
        let target =
            self.turn(store, target_turn_id, limit)?
                .ok_or(NativeProjectionError::Invariant(
                    "native target prefix tail is missing",
                ))?;
        if outer.record().chain_digest() != source.represented_prefix().digest() {
            return Err(NativeProjectionError::Invariant(
                "source binding represented-prefix digest disagrees",
            ));
        }
        crate::selected_path::includes_turn(
            outer.record().clone(),
            target.record(),
            |id| {
                self.turn(store, id, limit)?
                    .map(|stored| stored.record().clone())
                    .ok_or(NativeProjectionError::Invariant(
                        "native source ancestry turn is missing",
                    ))
            },
            NativeProjectionError::Invariant,
        )
    }
}

fn target_owned_source(
    target: SyndicThreadId,
    source: &NativeProjectionSource,
) -> Option<NativeProjectionSource> {
    (source.thread_id == target).then(|| source.clone())
}

struct CorrelatedNativeSource {
    thread_id: SyndicThreadId,
    binding_revision: beryl_model::BindingRevision,
    selected_path: crate::SelectedPathProof,
    binding: UsableCasBinding,
    cas_turn_id: beryl_model::CasTurnId,
    target_native_turn_count: beryl_model::CasNativeTurnCount,
}

impl SyndicStorage {
    fn source_for_correlated_turn(
        &self,
        store: &HomeStore,
        target_turn_id: SyndicTurnId,
        target_prefix: CasRepresentedPrefixProof,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<CorrelatedNativeSource>, NativeProjectionError> {
        let state = self.turn_state(store, target_turn_id, limit)?.ok_or(
            NativeProjectionError::Invariant("native target turn state is missing"),
        )?;
        if !state.record().lifecycle().is_proven_terminal()
            || state.record().source_event_count() == 0
        {
            return Ok(None);
        }
        let sequence = SourceEventSequence::new(state.record().source_event_count())
            .map_err(|_| NativeProjectionError::Invariant("terminal event sequence is invalid"))?;
        let event = self
            .source_event(store, target_turn_id, sequence, limit)?
            .ok_or(NativeProjectionError::Invariant(
                "native target terminal event is missing",
            ))?;
        if event.record().turn_id() != target_turn_id || event.record().sequence() != sequence {
            return Err(NativeProjectionError::Invariant(
                "native target terminal event identity disagrees",
            ));
        }
        if !matches!(
            event.record().payload(),
            SourceEventPayload::TurnEnded(status)
                if state.record().end_status() == Some(*status)
        ) {
            return Err(NativeProjectionError::Invariant(
                "native target terminal event outcome disagrees",
            ));
        }
        let Some(source) = event.record().source() else {
            return Ok(None);
        };
        let turn_index = self
            .cas_turn_owner(
                store,
                source.thread_id().clone(),
                source.turn_id().clone(),
                limit,
            )?
            .ok_or(NativeProjectionError::Invariant(
                "native target CAS-turn correlation is missing",
            ))?;
        if turn_index.record().cas_thread_id() != source.thread_id()
            || turn_index.record().cas_turn_id() != source.turn_id()
            || turn_index.record().turn_id() != target_turn_id
        {
            return Err(NativeProjectionError::Invariant(
                "native target CAS-turn correlation disagrees",
            ));
        }
        let target =
            self.turn(store, target_turn_id, limit)?
                .ok_or(NativeProjectionError::Invariant(
                    "native target turn is missing",
                ))?;
        if target.record().chain_digest() != target_prefix.digest() {
            return Err(NativeProjectionError::Invariant(
                "native target prefix digest disagrees",
            ));
        }

        let reservation = self
            .cas_thread_owner(store, source.thread_id().clone(), limit)?
            .ok_or(NativeProjectionError::Invariant(
                "native source CAS-thread reservation is missing",
            ))?;
        if reservation.record().cas_thread_id() != source.thread_id()
            || reservation.record().thread_id() != turn_index.record().thread_id()
        {
            return Err(NativeProjectionError::Invariant(
                "native source CAS-thread reservation disagrees",
            ));
        }
        if reservation.record().retired_binding_revision().is_some() {
            return Ok(None);
        }
        let binding = self
            .binding(
                store,
                reservation.record().thread_id(),
                reservation.record().latest_binding_revision(),
                limit,
            )?
            .ok_or(NativeProjectionError::Invariant(
                "native source latest binding is missing",
            ))?;
        if binding.record().thread_id() != reservation.record().thread_id()
            || binding.record().revision() != reservation.record().latest_binding_revision()
        {
            return Err(NativeProjectionError::Invariant(
                "native source latest binding identity disagrees",
            ));
        }
        let Some(usable) = usable_binding(binding.record().state()) else {
            return Ok(None);
        };
        if usable.cas_thread_id() != source.thread_id()
            || usable.native_turn_count() < turn_index.record().post_turn_native_count()
        {
            return Err(NativeProjectionError::Invariant(
                "native source binding and CAS-turn position disagree",
            ));
        }
        Ok(Some(CorrelatedNativeSource {
            thread_id: binding.record().thread_id(),
            binding_revision: binding.record().revision(),
            selected_path: binding.record().selected_path(),
            binding: usable.clone(),
            cas_turn_id: source.turn_id().clone(),
            target_native_turn_count: turn_index.record().post_turn_native_count(),
        }))
    }
}

impl SyndicStorage {
    fn full_fork_source(
        &self,
        store: &HomeStore,
        source_thread_id: SyndicThreadId,
        execution: &ExecutionBinding,
        target_prefix: CasRepresentedPrefixProof,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<NativeProjectionSource>, NativeProjectionError> {
        let Some(current) = self.current_binding(store, source_thread_id, limit)? else {
            return Ok(None);
        };
        let BindingState::Valid(usable) = current.binding().state() else {
            return Ok(None);
        };
        if usable.execution() != execution
            || !prefix_position_matches(usable.represented_prefix(), target_prefix)
        {
            return Ok(None);
        }
        Ok(Some(NativeProjectionSource {
            thread_id: source_thread_id,
            binding_revision: current.binding().revision(),
            selected_path: current.binding().selected_path(),
            binding: usable.clone(),
        }))
    }
}

fn usable_binding(state: &BindingState) -> Option<&UsableCasBinding> {
    match state {
        BindingState::Valid(usable) => Some(usable),
        BindingState::Active(active) => Some(active.usable()),
        BindingState::Unbound { .. } | BindingState::Stale(_) => None,
    }
}

fn prefix_position_matches(
    left: CasRepresentedPrefixProof,
    right: CasRepresentedPrefixProof,
) -> bool {
    left.tail() == right.tail() && left.digest() == right.digest()
}
