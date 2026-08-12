use std::{
    collections::BTreeMap,
    num::NonZeroU64,
    sync::{Arc, Mutex, MutexGuard},
};

use beryl_home_store::ThemeReconciliationEvidence;

use super::{InstalledThemeId, execution::ExpectedPublication};

/// Maximum retained ambiguous theme operations in one service generation.
pub const THEME_RETAINED_OPERATION_MAX: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ThemeOperationScope {
    Repository,
    Document(InstalledThemeId),
}

impl ThemeOperationScope {
    pub(super) fn overlaps(&self, other: &Self) -> bool {
        matches!(self, Self::Repository) || matches!(other, Self::Repository) || self == other
    }
}

#[derive(Clone, Debug)]
pub(super) struct RetainedOperation {
    pub(super) scope: ThemeOperationScope,
    pub(super) evidence: ThemeReconciliationEvidence,
    pub(super) expected: ExpectedPublication,
}

#[derive(Debug)]
enum RetainedState {
    Pending(RetainedOperation),
    Reconciling(RetainedOperation),
    Collision(RetainedOperation),
}

impl RetainedState {
    fn scope(&self) -> &ThemeOperationScope {
        match self {
            Self::Pending(operation)
            | Self::Reconciling(operation)
            | Self::Collision(operation) => &operation.scope,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ThemeScopeGateError {
    Gated,
    CapacityExhausted,
    UnknownOperation,
    ReconciliationBusy,
    CollisionClosed,
}

#[derive(Debug, Default)]
struct ThemeRuntimeState {
    active_mutations: Vec<ThemeOperationScope>,
    retained: BTreeMap<NonZeroU64, RetainedState>,
    diagnostics: ThemeServiceDiagnostics,
}

#[derive(Debug, Default)]
pub(super) struct ThemeServiceRuntime {
    state: Mutex<ThemeRuntimeState>,
}

impl ThemeServiceRuntime {
    pub(super) fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn lock(&self) -> MutexGuard<'_, ThemeRuntimeState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn diagnostics(&self) -> ThemeServiceDiagnostics {
        let state = self.lock();
        let mut diagnostics = state.diagnostics;
        diagnostics.open_scopes = state
            .retained
            .values()
            .filter(|value| !matches!(value, RetainedState::Collision(_)))
            .count();
        diagnostics.closed_collision_scopes = state
            .retained
            .values()
            .filter(|value| matches!(value, RetainedState::Collision(_)))
            .count();
        diagnostics.home_generation_present = true;
        diagnostics
    }

    pub(super) fn note_repository_observed(&self) {
        self.lock().diagnostics.repository_generation_present = true;
    }

    pub(super) fn refresh_is_gated(&self, scope: &ThemeOperationScope) -> bool {
        let state = self.lock();
        state
            .active_mutations
            .iter()
            .chain(state.retained.values().map(RetainedState::scope))
            .any(|active| match (active, scope) {
                (ThemeOperationScope::Repository, _) => true,
                (ThemeOperationScope::Document(active), ThemeOperationScope::Document(refresh)) => {
                    active == refresh
                }
                (ThemeOperationScope::Document(_), ThemeOperationScope::Repository) => false,
            })
    }

    pub(super) fn begin_mutation(
        self: &Arc<Self>,
        scope: ThemeOperationScope,
    ) -> Result<ThemeMutationGuard, ThemeScopeGateError> {
        let mut state = self.lock();
        if state
            .active_mutations
            .iter()
            .chain(state.retained.values().map(RetainedState::scope))
            .any(|active| active.overlaps(&scope))
        {
            return Err(ThemeScopeGateError::Gated);
        }
        if state
            .active_mutations
            .len()
            .saturating_add(state.retained.len())
            >= THEME_RETAINED_OPERATION_MAX
        {
            return Err(ThemeScopeGateError::CapacityExhausted);
        }
        state.active_mutations.push(scope.clone());
        state.diagnostics.mutations_started = state.diagnostics.mutations_started.saturating_add(1);
        Ok(ThemeMutationGuard {
            runtime: Arc::clone(self),
            scope: Some(scope),
        })
    }

    pub(super) fn retain(&self, operation: NonZeroU64, retained: RetainedOperation) {
        let mut state = self.lock();
        let replaced = state
            .retained
            .insert(operation, RetainedState::Pending(retained));
        debug_assert!(replaced.is_none(), "theme operation identifiers are unique");
        state.diagnostics.mutations_indeterminate =
            state.diagnostics.mutations_indeterminate.saturating_add(1);
    }

    pub(super) fn begin_reconciliation(
        &self,
        operation: NonZeroU64,
    ) -> Result<RetainedOperation, ThemeScopeGateError> {
        let mut state = self.lock();
        let custody = match state
            .retained
            .get(&operation)
            .ok_or(ThemeScopeGateError::UnknownOperation)?
        {
            RetainedState::Pending(retained) => retained.clone(),
            RetainedState::Reconciling(_) => {
                return Err(ThemeScopeGateError::ReconciliationBusy);
            }
            RetainedState::Collision(_) => return Err(ThemeScopeGateError::CollisionClosed),
        };
        state
            .retained
            .insert(operation, RetainedState::Reconciling(custody.clone()));
        state.diagnostics.reconciliations_in_flight = state
            .diagnostics
            .reconciliations_in_flight
            .saturating_add(1);
        Ok(custody)
    }

    pub(super) fn restore_reconciliation(
        &self,
        operation: NonZeroU64,
        retained: RetainedOperation,
    ) {
        let mut state = self.lock();
        state
            .retained
            .insert(operation, RetainedState::Pending(retained));
        state.diagnostics.reconciliations_in_flight = state
            .diagnostics
            .reconciliations_in_flight
            .saturating_sub(1);
    }

    pub(super) fn finish_reconciliation(
        &self,
        operation: NonZeroU64,
        retained: RetainedOperation,
        outcome: ThemeReconciliationMetric,
    ) {
        let mut state = self.lock();
        state.diagnostics.reconciliations_in_flight = state
            .diagnostics
            .reconciliations_in_flight
            .saturating_sub(1);
        match outcome {
            ThemeReconciliationMetric::ExactOld => {
                state.retained.remove(&operation);
                state.diagnostics.reconciliations_exact_old = state
                    .diagnostics
                    .reconciliations_exact_old
                    .saturating_add(1);
            }
            ThemeReconciliationMetric::ExactNew => {
                state.retained.remove(&operation);
                state.diagnostics.reconciliations_exact_new = state
                    .diagnostics
                    .reconciliations_exact_new
                    .saturating_add(1);
            }
            ThemeReconciliationMetric::Collision => {
                state
                    .retained
                    .insert(operation, RetainedState::Collision(retained));
                state.diagnostics.reconciliations_collision = state
                    .diagnostics
                    .reconciliations_collision
                    .saturating_add(1);
            }
        }
    }

    pub(super) fn note_mutation_committed(&self) {
        let mut state = self.lock();
        state.diagnostics.mutations_committed =
            state.diagnostics.mutations_committed.saturating_add(1);
    }

    pub(super) fn note_mutation_not_committed(&self) {
        let mut state = self.lock();
        state.diagnostics.mutations_not_committed =
            state.diagnostics.mutations_not_committed.saturating_add(1);
    }

    pub(super) fn begin_activity(self: &Arc<Self>, kind: ThemeActivityKind) -> ThemeActivityGuard {
        let mut state = self.lock();
        match kind {
            ThemeActivityKind::ManifestSession => {
                state.diagnostics.active_manifest_sessions =
                    state.diagnostics.active_manifest_sessions.saturating_add(1);
            }
            ThemeActivityKind::Subscription => {
                state.diagnostics.active_subscriptions =
                    state.diagnostics.active_subscriptions.saturating_add(1);
            }
            ThemeActivityKind::DocumentLoad => {
                state.diagnostics.document_loads_in_flight =
                    state.diagnostics.document_loads_in_flight.saturating_add(1);
            }
        }
        ThemeActivityGuard {
            runtime: Arc::clone(self),
            kind,
        }
    }

    fn finish_activity(&self, kind: ThemeActivityKind) {
        let mut state = self.lock();
        match kind {
            ThemeActivityKind::ManifestSession => {
                state.diagnostics.active_manifest_sessions =
                    state.diagnostics.active_manifest_sessions.saturating_sub(1);
            }
            ThemeActivityKind::Subscription => {
                state.diagnostics.active_subscriptions =
                    state.diagnostics.active_subscriptions.saturating_sub(1);
            }
            ThemeActivityKind::DocumentLoad => {
                state.diagnostics.document_loads_in_flight =
                    state.diagnostics.document_loads_in_flight.saturating_sub(1);
            }
        }
    }

    pub(super) fn note_change_hint(&self, overflow: bool) {
        let mut state = self.lock();
        state.diagnostics.change_hints = state.diagnostics.change_hints.saturating_add(1);
        state.diagnostics.coalesced_change_hints =
            state.diagnostics.coalesced_change_hints.saturating_add(1);
        if overflow {
            state.diagnostics.overflow_hints = state.diagnostics.overflow_hints.saturating_add(1);
        }
    }

    pub(super) fn note_document_load_retry_rejection(&self) {
        let mut state = self.lock();
        state.diagnostics.document_load_retry_rejections = state
            .diagnostics
            .document_load_retry_rejections
            .saturating_add(1);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ThemeReconciliationMetric {
    ExactOld,
    ExactNew,
    Collision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ThemeActivityKind {
    ManifestSession,
    Subscription,
    DocumentLoad,
}

pub(super) struct ThemeActivityGuard {
    runtime: Arc<ThemeServiceRuntime>,
    kind: ThemeActivityKind,
}

impl Drop for ThemeActivityGuard {
    fn drop(&mut self) {
        self.runtime.finish_activity(self.kind);
    }
}

pub(super) struct ThemeMutationGuard {
    runtime: Arc<ThemeServiceRuntime>,
    scope: Option<ThemeOperationScope>,
}

impl ThemeMutationGuard {
    pub(super) fn scope(&self) -> &ThemeOperationScope {
        self.scope
            .as_ref()
            .expect("live mutation guard has a scope")
    }
}

impl Drop for ThemeMutationGuard {
    fn drop(&mut self) {
        let Some(scope) = self.scope.take() else {
            return;
        };
        let mut state = self.runtime.lock();
        if let Some(index) = state
            .active_mutations
            .iter()
            .position(|value| value == &scope)
        {
            state.active_mutations.swap_remove(index);
        }
    }
}

/// Bounded content-free counters for one theme-service generation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ThemeServiceDiagnostics {
    home_generation_present: bool,
    repository_generation_present: bool,
    active_manifest_sessions: usize,
    active_subscriptions: usize,
    change_hints: u64,
    coalesced_change_hints: u64,
    overflow_hints: u64,
    mutations_started: u64,
    mutations_committed: u64,
    mutations_not_committed: u64,
    mutations_indeterminate: u64,
    reconciliations_in_flight: usize,
    reconciliations_exact_old: u64,
    reconciliations_exact_new: u64,
    reconciliations_collision: u64,
    open_scopes: usize,
    closed_collision_scopes: usize,
    document_loads_in_flight: usize,
    document_load_retry_rejections: u64,
}

macro_rules! diagnostic_getters {
    ($(($name:ident, $type:ty)),+ $(,)?) => {
        impl ThemeServiceDiagnostics {
            $(
                #[must_use]
                pub const fn $name(self) -> $type {
                    self.$name
                }
            )+
        }
    };
}

diagnostic_getters!(
    (home_generation_present, bool),
    (repository_generation_present, bool),
    (active_manifest_sessions, usize),
    (active_subscriptions, usize),
    (change_hints, u64),
    (coalesced_change_hints, u64),
    (overflow_hints, u64),
    (mutations_started, u64),
    (mutations_committed, u64),
    (mutations_not_committed, u64),
    (mutations_indeterminate, u64),
    (reconciliations_in_flight, usize),
    (reconciliations_exact_old, u64),
    (reconciliations_exact_new, u64),
    (reconciliations_collision, u64),
    (open_scopes, usize),
    (closed_collision_scopes, usize),
    (document_loads_in_flight, usize),
    (document_load_retry_rejections, u64),
);

#[cfg(test)]
mod tests {
    use super::ThemeServiceRuntime;

    #[test]
    fn registry_memory_releases_after_the_last_generation_owner_drops() {
        let runtime = ThemeServiceRuntime::shared();
        let weak = std::sync::Arc::downgrade(&runtime);
        let clone = runtime.clone();
        drop(runtime);
        assert!(weak.upgrade().is_some());
        drop(clone);
        assert!(weak.upgrade().is_none());
    }
}
