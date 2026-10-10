use super::*;
use syndic_storage::{InputGateState, TurnKind, TurnLifecycle};

#[derive(Clone, Eq, PartialEq)]
pub struct ExactOperationOrigin {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
    thread: SyndicThreadId,
    turn: beryl_model::SyndicTurnId,
    kind: TurnKind,
}

impl std::fmt::Debug for ExactOperationOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExactOperationOrigin")
            .finish_non_exhaustive()
    }
}

impl ExactOperationOrigin {
    pub(crate) fn same_selected_thread(&self, other: &Self) -> bool {
        self.home_id == other.home_id
            && self.home_generation == other.home_generation
            && self.service_generation == other.service_generation
            && self.thread == other.thread
    }
    pub(crate) fn belongs_to_service(
        &self,
        identity: (BerylHomeId, HomeGeneration, ProjectionServiceGeneration),
    ) -> bool {
        (self.home_id, self.home_generation, self.service_generation) == identity
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactParentState {
    Unknown,
    Working,
    Compacting,
    RepairPending,
    Complete,
    Failed,
    Interrupted,
    Incomplete,
    UnknownTerminal,
}

impl ExactParentState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Working => "working",
            Self::Compacting => "compacting",
            Self::RepairPending => "repair pending",
            Self::Complete => "ok",
            Self::Failed => "error",
            Self::Interrupted => "interrupted",
            Self::Incomplete => "incomplete",
            Self::UnknownTerminal => "unknown terminal",
        }
    }

    pub fn active(self) -> bool {
        matches!(self, Self::Working | Self::Compacting)
    }
}

pub struct ExactSelectedOperationSnapshot {
    pub context: Option<beryl_backend::ContextTokenUsage>,
    context_origin: Option<context::SelectedContext>,
    pub state: ExactParentState,
    pub operation_active: bool,
    pub origin: Option<ExactOperationOrigin>,
    pub stop: ExactSoftStopAvailability,
}

impl ExactSelectedOperationSnapshot {
    pub(crate) fn revalidate_context(&mut self) {
        if !self
            .context_origin
            .as_ref()
            .is_some_and(|origin| origin.is_current())
        {
            self.context = None;
            self.context_origin = None;
        }
    }

    pub fn unavailable() -> Self {
        Self {
            context: None,
            context_origin: None,
            state: ExactParentState::Unknown,
            operation_active: false,
            origin: None,
            stop: ExactSoftStopAvailability::Unavailable(
                ExactSoftStopUnavailable::AuthorityUnavailable,
            ),
        }
    }
}

impl ExactStopRead {
    pub(in crate::cas_projection) fn operation_origin(
        &self,
        thread: SyndicThreadId,
        turn: beryl_model::SyndicTurnId,
        kind: TurnKind,
    ) -> ExactOperationOrigin {
        ExactOperationOrigin {
            home_id: self.home_id,
            home_generation: self.home_generation,
            service_generation: self.connections.service_generation(),
            thread,
            turn,
            kind,
        }
    }

    pub(super) fn selected_operation_snapshot(
        &self,
        thread: SyndicThreadId,
    ) -> Option<ExactSelectedOperationSnapshot> {
        let home = self.home.as_deref()?;
        let context = self.selected_context(thread);
        let limit = SyndicPointReadLimit::new(1_000_000).ok()?;
        let thread_record = self.storage.thread(home, thread, limit).ok()??;
        let gate = self.storage.input_gate(home, thread, limit).ok()??;
        let turn_id = gate
            .state()
            .blocking_turn_id()
            .or(thread_record.committed_tail());
        let Some(turn_id) = turn_id else {
            let stop = self.exact_soft_stop_eligibility(thread);
            if self
                .storage
                .thread(home, thread, limit)
                .ok()
                .flatten()
                .as_ref()
                != Some(&thread_record)
                || self
                    .storage
                    .input_gate(home, thread, limit)
                    .ok()
                    .flatten()
                    .as_ref()
                    != Some(&gate)
                || !self.command_authorizer.is_open()
                || admission::ensure_current_home(
                    Some(home),
                    self.home_id,
                    self.home_generation,
                    &self.storage,
                )
                .is_err()
            {
                return None;
            }
            let context =
                context.filter(|value| self.selected_context(thread).as_ref() == Some(value));
            return Some(ExactSelectedOperationSnapshot {
                context: context.as_ref().and_then(|value| value.usage()),
                context_origin: context,
                state: ExactParentState::Unknown,
                operation_active: false,
                origin: None,
                stop,
            });
        };
        let turn = self.storage.turn(home, turn_id, limit).ok()??;
        let state = self.storage.turn_state(home, turn_id, limit).ok()??;
        let live = match self.prepare_stop(thread) {
            Ok(PreparedStop::Exact { target, proof, .. })
                if target.turn_id() == turn_id && target.turn_kind() == turn.kind() =>
            {
                Some((target, proof))
            }
            _ => None,
        };
        let stop = self.exact_soft_stop_eligibility(thread);
        if self
            .storage
            .thread(home, thread, limit)
            .ok()
            .flatten()
            .as_ref()
            != Some(&thread_record)
            || self
                .storage
                .input_gate(home, thread, limit)
                .ok()
                .flatten()
                .as_ref()
                != Some(&gate)
            || self
                .storage
                .turn_state(home, turn_id, limit)
                .ok()
                .flatten()
                .as_ref()
                != Some(&state)
            || !self.command_authorizer.is_open()
            || admission::ensure_current_home(
                Some(home),
                self.home_id,
                self.home_generation,
                &self.storage,
            )
            .is_err()
        {
            return None;
        }
        let live = match (live, self.prepare_stop(thread)) {
            (
                Some((before_target, before_proof)),
                Ok(PreparedStop::Exact { target, proof, .. }),
            ) if before_target == target && before_proof == proof => true,
            (Some(_), _) => return None,
            (None, _) if matches!(stop, ExactSoftStopAvailability::Eligible(_)) => return None,
            _ => false,
        };
        let origin = self.operation_origin(thread, turn_id, turn.kind());
        if let ExactSoftStopAvailability::Eligible(eligibility) = &stop
            && eligibility.operation_origin() != origin
        {
            return None;
        }
        let parent = match state.lifecycle() {
            lifecycle
                if lifecycle.is_proven_terminal()
                    && matches!(gate.state(), InputGateState::RepairRequired(_)) =>
            {
                ExactParentState::RepairPending
            }
            _ if matches!(turn.kind(), TurnKind::ProviderOperation(_))
                && matches!(gate.state(), InputGateState::Compacting { .. }) =>
            {
                ExactParentState::Compacting
            }
            TurnLifecycle::Complete if !matches!(turn.kind(), TurnKind::ProviderOperation(_)) => {
                ExactParentState::Complete
            }
            TurnLifecycle::Failed if !matches!(turn.kind(), TurnKind::ProviderOperation(_)) => {
                ExactParentState::Failed
            }
            TurnLifecycle::Interrupted
                if !matches!(turn.kind(), TurnKind::ProviderOperation(_)) =>
            {
                ExactParentState::Interrupted
            }
            TurnLifecycle::Incomplete if !matches!(turn.kind(), TurnKind::ProviderOperation(_)) => {
                ExactParentState::Incomplete
            }
            TurnLifecycle::UnknownTerminal
                if !matches!(turn.kind(), TurnKind::ProviderOperation(_)) =>
            {
                ExactParentState::UnknownTerminal
            }
            TurnLifecycle::Pending | TurnLifecycle::Active
                if matches!(gate.state(), InputGateState::Compacting { .. })
                    || (live && matches!(turn.kind(), TurnKind::ProviderOperation(_))) =>
            {
                ExactParentState::Compacting
            }
            TurnLifecycle::Active if live => ExactParentState::Working,
            _ => ExactParentState::Unknown,
        };
        let operation_active = live
            && parent.active()
            && matches!(
                state.lifecycle(),
                TurnLifecycle::Pending | TurnLifecycle::Active
            );
        let context = context.filter(|value| self.selected_context(thread).as_ref() == Some(value));
        Some(ExactSelectedOperationSnapshot {
            context: context.as_ref().and_then(|value| value.usage()),
            context_origin: context,
            state: parent,
            operation_active,
            origin: Some(origin),
            stop,
        })
    }
}
