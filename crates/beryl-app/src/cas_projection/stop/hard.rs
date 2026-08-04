use std::{collections::HashMap, sync::Arc};

use beryl_backend::{
    CoarseThreadCleanupDisposition, CoarseThreadCleanupOutcome, ExactForegroundTurn,
    ExactHardStopLimitation, StopAttemptCorrelation, StopOperationCorrelation,
};
use beryl_model::{
    CasLoadedSessionGeneration, CasThreadId, CasTurnId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{StopAttemptNonce, StopOperationId, StopOperationTarget};

use super::{
    LocalDispatchState, StopCoordinationError, StopCoordinator, StopDispatchSettlement,
    attempt_correlation, operation_correlation,
};
use crate::cas_projection::connection::{StopElectionPermit, StopTargetProof};

const HARD_STOP_SNAPSHOT_CAPACITY: usize = 64;

/// Closed normalized provider activity kind relevant to hard-stop admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishedHardStopActivityKind {
    Command,
    ChildOrSubagent,
}

/// Closed normalized lifecycle effect applied only after provider publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishedHardStopActivityLifecycle {
    Active,
    Completed,
}

/// One compact process-local activity effect bound to an exact published turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedHardStopActivity {
    syndic_thread_id: SyndicThreadId,
    syndic_turn_id: SyndicTurnId,
    loaded_generation: CasLoadedSessionGeneration,
    cas_thread_id: CasThreadId,
    cas_turn_id: CasTurnId,
    provider_item_id: SyndicItemId,
    kind: PublishedHardStopActivityKind,
    lifecycle: PublishedHardStopActivityLifecycle,
}

impl PublishedHardStopActivity {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn new(
        syndic_thread_id: SyndicThreadId,
        syndic_turn_id: SyndicTurnId,
        loaded_generation: CasLoadedSessionGeneration,
        cas_thread_id: CasThreadId,
        cas_turn_id: CasTurnId,
        provider_item_id: SyndicItemId,
        kind: PublishedHardStopActivityKind,
        lifecycle: PublishedHardStopActivityLifecycle,
    ) -> Self {
        Self {
            syndic_thread_id,
            syndic_turn_id,
            loaded_generation,
            cas_thread_id,
            cas_turn_id,
            provider_item_id,
            kind,
            lifecycle,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ActivityTarget {
    pub(super) turn_id: SyndicTurnId,
    loaded_generation: CasLoadedSessionGeneration,
    cas_thread_id: CasThreadId,
    cas_turn_id: CasTurnId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActiveItem {
    provider_item_id: SyndicItemId,
    kind: PublishedHardStopActivityKind,
}

#[derive(Clone, Debug)]
pub(super) struct ActiveActivity {
    pub(super) target: ActivityTarget,
    items: Vec<ActiveItem>,
    omitted_commands: u32,
    omitted_children: u32,
    command_overflow: bool,
    child_overflow: bool,
}

impl ActiveActivity {
    fn new(effect: &PublishedHardStopActivity) -> Self {
        Self {
            target: ActivityTarget {
                turn_id: effect.syndic_turn_id,
                loaded_generation: effect.loaded_generation,
                cas_thread_id: effect.cas_thread_id.clone(),
                cas_turn_id: effect.cas_turn_id.clone(),
            },
            items: Vec::with_capacity(HARD_STOP_SNAPSHOT_CAPACITY),
            omitted_commands: 0,
            omitted_children: 0,
            command_overflow: false,
            child_overflow: false,
        }
    }

    fn matches(&self, effect: &PublishedHardStopActivity) -> bool {
        self.target.turn_id == effect.syndic_turn_id
            && self.target.loaded_generation == effect.loaded_generation
            && self.target.cas_thread_id == effect.cas_thread_id
            && self.target.cas_turn_id == effect.cas_turn_id
    }

    fn apply(&mut self, effect: PublishedHardStopActivity) {
        let position = self.items.iter().position(|item| {
            item.provider_item_id == effect.provider_item_id && item.kind == effect.kind
        });
        match (effect.lifecycle, position) {
            (PublishedHardStopActivityLifecycle::Active, Some(_)) => {}
            (PublishedHardStopActivityLifecycle::Active, None)
                if self.items.len() < HARD_STOP_SNAPSHOT_CAPACITY =>
            {
                self.items.push(ActiveItem {
                    provider_item_id: effect.provider_item_id,
                    kind: effect.kind,
                });
            }
            (PublishedHardStopActivityLifecycle::Active, None) => {
                self.increment_omitted(effect.kind)
            }
            (PublishedHardStopActivityLifecycle::Completed, Some(position)) => {
                self.items.remove(position);
            }
            (PublishedHardStopActivityLifecycle::Completed, None) => {
                self.decrement_omitted(effect.kind)
            }
        }
    }

    fn increment_omitted(&mut self, kind: PublishedHardStopActivityKind) {
        let (count, overflow) = match kind {
            PublishedHardStopActivityKind::Command => {
                (&mut self.omitted_commands, &mut self.command_overflow)
            }
            PublishedHardStopActivityKind::ChildOrSubagent => {
                (&mut self.omitted_children, &mut self.child_overflow)
            }
        };
        match count.checked_add(1) {
            Some(next) => *count = next,
            None => *overflow = true,
        }
    }

    fn decrement_omitted(&mut self, kind: PublishedHardStopActivityKind) {
        let count = match kind {
            PublishedHardStopActivityKind::Command => &mut self.omitted_commands,
            PublishedHardStopActivityKind::ChildOrSubagent => &mut self.omitted_children,
        };
        *count = count.saturating_sub(1);
    }
}

/// One fixed unsupported-family report with its frozen active count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HardStopLimitation {
    limitation: ExactHardStopLimitation,
    omitted_active: u32,
    count_overflowed: bool,
}

impl HardStopLimitation {
    #[must_use]
    pub const fn limitation(&self) -> ExactHardStopLimitation {
        self.limitation
    }

    #[must_use]
    pub const fn omitted_active(&self) -> u32 {
        self.omitted_active
    }

    #[must_use]
    pub const fn count_overflowed(&self) -> bool {
        self.count_overflowed
    }
}

/// The only release-proven pinned hard target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardStopTargetKind {
    CoarseThreadCleanup,
}

/// Closed bounded outcome of one frozen hard target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardStopTargetDisposition {
    RequestAccepted,
    ProvenNotDispatched,
    SessionAuthorityLost,
    CompletionUnknown,
    UnavailableWithoutDispatch,
}

/// Result of one target in the immutable frozen order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HardStopTargetResult {
    target: HardStopTargetKind,
    disposition: HardStopTargetDisposition,
}

impl HardStopTargetResult {
    #[must_use]
    pub const fn target(&self) -> HardStopTargetKind {
        self.target
    }

    #[must_use]
    pub const fn disposition(&self) -> HardStopTargetDisposition {
        self.disposition
    }
}

/// Cloneable fixed-capacity result shared by every duplicate hard caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedHardStopResult {
    operation_id: StopOperationId,
    limitations: [HardStopLimitation; 2],
    targets: Vec<HardStopTargetResult>,
}

impl BoundedHardStopResult {
    #[must_use]
    pub const fn operation_id(&self) -> StopOperationId {
        self.operation_id
    }

    #[must_use]
    pub const fn limitations(&self) -> &[HardStopLimitation; 2] {
        &self.limitations
    }

    #[must_use]
    pub fn targets(&self) -> &[HardStopTargetResult] {
        &self.targets
    }
}

#[derive(Clone, Debug)]
struct FrozenSnapshot {
    limitations: [HardStopLimitation; 2],
    cleanup: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PrimaryDisposition {
    Accepted,
    ProvenNondispatch,
}

#[derive(Debug)]
pub(super) enum SlotPhase {
    AwaitingPrimary,
    Running(PrimaryDisposition),
    Finished(BoundedHardStopResult),
}

#[derive(Debug)]
pub(super) struct HardStopSlot {
    pub(super) target: StopOperationTarget,
    pub(super) attempt: StopAttemptNonce,
    snapshot: FrozenSnapshot,
    command: Option<crate::cas_projection::LiveCommandPermit>,
    pub(super) phase: SlotPhase,
    pub(super) joined_waiters: usize,
    pub(super) consumed: bool,
    pub(super) terminal_successor: bool,
}

#[derive(Default)]
pub(super) struct HardStopCoordinatorState {
    pub(super) slots: HashMap<StopOperationId, HardStopSlot>,
}

impl HardStopCoordinatorState {
    pub(super) fn freeze_for_persistent_failure(
        &mut self,
    ) -> Vec<crate::cas_projection::LiveCommandPermit> {
        let mut commands = Vec::new();
        for (operation_id, slot) in &mut self.slots {
            let disposition = match slot.phase {
                SlotPhase::AwaitingPrimary => {
                    if let Some(command) = slot.command.take() {
                        commands.push(command);
                    }
                    HardStopTargetDisposition::UnavailableWithoutDispatch
                }
                SlotPhase::Running(_) => HardStopTargetDisposition::CompletionUnknown,
                SlotPhase::Finished(_) => continue,
            };
            slot.phase =
                SlotPhase::Finished(snapshot_result(*operation_id, &slot.snapshot, disposition));
        }
        commands
    }
}

#[derive(Default)]
pub(super) struct HardStopActivityState {
    by_thread: HashMap<SyndicThreadId, ActiveActivity>,
}

/// One joined caller for a running or finished escalation slot.
pub struct HardStopAttachment {
    coordinator: Arc<StopCoordinator>,
    operation_id: StopOperationId,
    acknowledged: bool,
}

/// One joined hard-stop waiter and, for an acceptance-first attachment, its sole continuation.
pub(in crate::cas_projection) struct HardStopAdmission {
    attachment: HardStopAttachment,
    run_owner: Option<HardStopRunOwner>,
}

impl HardStopAdmission {
    pub(in crate::cas_projection) fn into_parts(
        self,
    ) -> (HardStopAttachment, Option<HardStopRunOwner>) {
        (self.attachment, self.run_owner)
    }
}

impl std::fmt::Debug for HardStopAttachment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HardStopAttachment")
            .field("operation_id", &self.operation_id)
            .finish_non_exhaustive()
    }
}

/// The sole non-cloneable continuation consumed by the foreground driver.
pub struct HardStopRunOwner {
    coordinator: Arc<StopCoordinator>,
    operation_id: StopOperationId,
    target: StopOperationTarget,
    attempt: StopAttemptNonce,
    primary: PrimaryDisposition,
    cleanup: bool,
    timeout: std::time::Duration,
    election: HardStopElection,
    _command: Option<crate::cas_projection::LiveCommandPermit>,
    finished: bool,
}

enum HardStopElection {
    /// The original primary stop election remains held until cleanup authorization is minted.
    Inherited(Option<StopElectionPermit>),
    /// An acceptance-first attachment must acquire a fresh exact election in the driver.
    FreshRequired,
}

impl std::fmt::Debug for HardStopRunOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HardStopRunOwner")
            .field("operation_id", &self.operation_id)
            .field("cleanup", &self.cleanup)
            .finish_non_exhaustive()
    }
}

impl StopCoordinator {
    pub(in crate::cas_projection) fn record_published_activity(
        &self,
        effect: PublishedHardStopActivity,
    ) {
        let Ok(mut activity) = self.hard_activity.lock() else {
            return;
        };
        match activity.by_thread.get_mut(&effect.syndic_thread_id) {
            Some(active) if active.matches(&effect) => active.apply(effect),
            Some(_) if effect.lifecycle == PublishedHardStopActivityLifecycle::Completed => {}
            Some(_) => {}
            None if effect.lifecycle == PublishedHardStopActivityLifecycle::Active => {
                let thread_id = effect.syndic_thread_id;
                let mut active = ActiveActivity::new(&effect);
                active.apply(effect);
                activity.by_thread.insert(thread_id, active);
            }
            None => {}
        }
    }

    pub(in crate::cas_projection) fn clear_published_activity(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) {
        if let Ok(mut activity) = self.hard_activity.lock()
            && activity
                .by_thread
                .get(&thread_id)
                .is_some_and(|active| active.target.turn_id == turn_id)
        {
            activity.by_thread.remove(&thread_id);
        }
    }

    pub(in crate::cas_projection) fn attach_hard_stop(
        self: &Arc<Self>,
        operation_id: StopOperationId,
    ) -> Result<HardStopAdmission, StopCoordinationError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        self.ensure_current()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        if let Some(slot) = state.hard.slots.get_mut(&operation_id) {
            if slot.consumed {
                return Err(StopCoordinationError::TargetUnavailable);
            }
            slot.joined_waiters = slot
                .joined_waiters
                .checked_add(1)
                .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
            return Ok(HardStopAdmission {
                attachment: HardStopAttachment::new(Arc::clone(self), operation_id),
                run_owner: None,
            });
        }
        let (target, attempt, dispatch, timeout) = {
            let local = state
                .stops
                .get(&operation_id.thread_id())
                .filter(|local| local.operation_id == operation_id)
                .ok_or(StopCoordinationError::TargetUnavailable)?;
            (
                local.target.clone(),
                local
                    .attempt
                    .ok_or(StopCoordinationError::LocalAuthorityMismatch)?,
                local.dispatch,
                local.timeout,
            )
        };
        let primary = match dispatch {
            LocalDispatchState::ClaimedNotDispatched | LocalDispatchState::Dispatching => None,
            LocalDispatchState::PrimaryAccepted => Some(PrimaryDisposition::Accepted),
            _ => return Err(StopCoordinationError::TargetUnavailable),
        };
        let snapshot = {
            let activity = self
                .hard_activity
                .lock()
                .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
            freeze_snapshot(activity.by_thread.get(&operation_id.thread_id()), &target)
        };
        let cleanup = snapshot.cleanup;
        state.hard.slots.insert(
            operation_id,
            HardStopSlot {
                target: target.clone(),
                attempt,
                snapshot,
                command: None,
                phase: primary.map_or(SlotPhase::AwaitingPrimary, SlotPhase::Running),
                joined_waiters: 1,
                consumed: false,
                terminal_successor: false,
            },
        );
        let mut command = Some(command);
        let run_owner = primary.map(|primary| HardStopRunOwner {
            coordinator: Arc::clone(self),
            operation_id,
            target,
            attempt,
            primary,
            cleanup,
            timeout,
            election: HardStopElection::FreshRequired,
            _command: command.take(),
            finished: false,
        });
        if let Some(slot) = state.hard.slots.get_mut(&operation_id) {
            slot.command = command;
        }
        Ok(HardStopAdmission {
            attachment: HardStopAttachment::new(Arc::clone(self), operation_id),
            run_owner,
        })
    }

    pub(in crate::cas_projection) fn wait_for_finalization_release(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Result<(), StopCoordinationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        while state.hard.slots.values().any(|slot| {
            slot.consumed
                && slot.target.thread_id() == thread_id
                && slot.target.turn_id() == turn_id
                && !matches!(slot.phase, SlotPhase::Finished(_))
        }) {
            state = self
                .hard_wake
                .wait(state)
                .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        }
        Ok(())
    }
}

fn freeze_snapshot(
    activity: Option<&ActiveActivity>,
    target: &StopOperationTarget,
) -> FrozenSnapshot {
    let matching = activity.filter(|activity| {
        activity.target.turn_id == target.turn_id()
            && activity.target.loaded_generation == target.loaded_generation()
            && activity.target.cas_thread_id == *target.cas_thread_id()
            && activity.target.cas_turn_id == *target.cas_turn_id()
    });
    let (commands, children, command_overflow, child_overflow) = match matching {
        Some(activity) => {
            let tracked_commands = activity
                .items
                .iter()
                .filter(|item| item.kind == PublishedHardStopActivityKind::Command)
                .count() as u32;
            let tracked_children = activity
                .items
                .iter()
                .filter(|item| item.kind == PublishedHardStopActivityKind::ChildOrSubagent)
                .count() as u32;
            let commands = tracked_commands.checked_add(activity.omitted_commands);
            let children = tracked_children.checked_add(activity.omitted_children);
            (
                commands.unwrap_or(u32::MAX),
                children.unwrap_or(u32::MAX),
                activity.command_overflow || commands.is_none(),
                activity.child_overflow || children.is_none(),
            )
        }
        None => (0, 0, false, false),
    };
    FrozenSnapshot {
        limitations: [
            HardStopLimitation {
                limitation: ExactHardStopLimitation::ChildOrSubagentInterruptionUnsupported,
                omitted_active: children,
                count_overflowed: child_overflow,
            },
            HardStopLimitation {
                limitation: ExactHardStopLimitation::IndividualTurnProcessTerminationIdentityUnsafe,
                omitted_active: commands,
                count_overflowed: command_overflow,
            },
        ],
        cleanup: commands > 0,
    }
}

impl HardStopAttachment {
    fn new(coordinator: Arc<StopCoordinator>, operation_id: StopOperationId) -> Self {
        Self {
            coordinator,
            operation_id,
            acknowledged: false,
        }
    }

    pub fn wait(mut self) -> Result<BoundedHardStopResult, StopCoordinationError> {
        let mut state = self
            .coordinator
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        loop {
            let result = state
                .hard
                .slots
                .get(&self.operation_id)
                .ok_or(StopCoordinationError::TargetUnavailable)
                .and_then(|slot| match &slot.phase {
                    SlotPhase::Finished(result) => Ok(Some(result.clone())),
                    SlotPhase::AwaitingPrimary | SlotPhase::Running(_) => Ok(None),
                })?;
            if let Some(result) = result {
                acknowledge_waiter(&mut state.hard, self.operation_id);
                self.acknowledged = true;
                return Ok(result);
            }
            state = self
                .coordinator
                .hard_wake
                .wait(state)
                .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        }
    }
}

impl Drop for HardStopAttachment {
    fn drop(&mut self) {
        if self.acknowledged {
            return;
        }
        if let Ok(mut state) = self.coordinator.state.lock() {
            acknowledge_waiter(&mut state.hard, self.operation_id);
        }
    }
}

fn acknowledge_waiter(hard: &mut HardStopCoordinatorState, operation_id: StopOperationId) {
    let remove = if let Some(slot) = hard.slots.get_mut(&operation_id) {
        slot.joined_waiters = slot.joined_waiters.saturating_sub(1);
        slot.consumed && slot.joined_waiters == 0 && matches!(slot.phase, SlotPhase::Finished(_))
    } else {
        false
    };
    if remove {
        hard.slots.remove(&operation_id);
    }
}

impl StopCoordinator {
    pub(super) fn begin_racing_proven_nondispatch_hard_run(
        self: &Arc<Self>,
        state: &mut super::StopCoordinatorState,
        operation_id: StopOperationId,
        target: &StopOperationTarget,
        attempt: StopAttemptNonce,
        timeout: std::time::Duration,
    ) -> Result<Option<HardStopRunOwner>, StopCoordinationError> {
        let Some(slot) = state.hard.slots.get_mut(&operation_id) else {
            return Ok(None);
        };
        if slot.target != *target
            || slot.attempt != attempt
            || !matches!(slot.phase, SlotPhase::AwaitingPrimary)
        {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        let local = state
            .stops
            .get_mut(&operation_id.thread_id())
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        if local.operation_id != operation_id
            || local.target != *target
            || local.attempt != Some(attempt)
            || !matches!(
                local.dispatch,
                LocalDispatchState::ClaimedNotDispatched | LocalDispatchState::Dispatching
            )
        {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        local.dispatch = LocalDispatchState::HardStopRunningProvenNondispatch;
        slot.phase = SlotPhase::Running(PrimaryDisposition::ProvenNondispatch);
        let command = slot
            .command
            .take()
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        Ok(Some(HardStopRunOwner {
            coordinator: Arc::clone(self),
            operation_id,
            target: target.clone(),
            attempt,
            primary: PrimaryDisposition::ProvenNondispatch,
            cleanup: slot.snapshot.cleanup,
            timeout,
            election: HardStopElection::FreshRequired,
            _command: Some(command),
            finished: false,
        }))
    }

    #[cfg(test)]
    pub(super) fn begin_hard_run(
        self: &Arc<Self>,
        operation_id: StopOperationId,
        target: &StopOperationTarget,
        attempt: StopAttemptNonce,
        primary_accepted: bool,
        timeout: std::time::Duration,
    ) -> Result<Option<HardStopRunOwner>, StopCoordinationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        if !state.hard.slots.contains_key(&operation_id) {
            return Ok(None);
        }
        {
            let local = state
                .stops
                .get_mut(&operation_id.thread_id())
                .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
            if local.operation_id != operation_id
                || local.target != *target
                || local.attempt != Some(attempt)
                || !matches!(
                    local.dispatch,
                    LocalDispatchState::ClaimedNotDispatched | LocalDispatchState::Dispatching
                )
            {
                return Err(StopCoordinationError::LocalAuthorityMismatch);
            }
            local.dispatch = if primary_accepted {
                LocalDispatchState::PrimaryAccepted
            } else {
                LocalDispatchState::HardStopRunningProvenNondispatch
            };
        }
        let slot = state
            .hard
            .slots
            .get_mut(&operation_id)
            .expect("checked hard-stop slot remains present");
        if slot.target != *target
            || slot.attempt != attempt
            || !matches!(slot.phase, SlotPhase::AwaitingPrimary)
        {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        let primary = if primary_accepted {
            PrimaryDisposition::Accepted
        } else {
            PrimaryDisposition::ProvenNondispatch
        };
        slot.phase = SlotPhase::Running(primary);
        let command = slot
            .command
            .take()
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        Ok(Some(HardStopRunOwner {
            coordinator: Arc::clone(self),
            operation_id,
            target: target.clone(),
            attempt,
            primary,
            cleanup: slot.snapshot.cleanup,
            timeout,
            election: HardStopElection::FreshRequired,
            _command: Some(command),
            finished: false,
        }))
    }

    pub(super) fn settle_primary_and_begin_hard_run(
        self: &Arc<Self>,
        operation_id: StopOperationId,
        target: &StopOperationTarget,
        attempt: StopAttemptNonce,
        primary_accepted: bool,
        timeout: std::time::Duration,
        inherited_election: StopElectionPermit,
    ) -> Result<Option<HardStopRunOwner>, StopCoordinationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let cleanup = match state.hard.slots.get(&operation_id) {
            Some(slot)
                if slot.target == *target
                    && slot.attempt == attempt
                    && matches!(slot.phase, SlotPhase::AwaitingPrimary) =>
            {
                Some(slot.snapshot.cleanup)
            }
            Some(_) => return Err(StopCoordinationError::LocalAuthorityMismatch),
            None => None,
        };
        let local = state
            .stops
            .get_mut(&operation_id.thread_id())
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        if local.operation_id != operation_id
            || local.target != *target
            || local.attempt != Some(attempt)
            || !matches!(
                local.dispatch,
                LocalDispatchState::ClaimedNotDispatched | LocalDispatchState::Dispatching
            )
        {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        local.dispatch = match (primary_accepted, cleanup.is_some()) {
            (true, _) => LocalDispatchState::PrimaryAccepted,
            (false, true) => LocalDispatchState::HardStopRunningProvenNondispatch,
            (false, false) => LocalDispatchState::ProvenNondispatchSettling,
        };
        let Some(cleanup) = cleanup else {
            drop(state);
            inherited_election.finish();
            return Ok(None);
        };
        let primary = if primary_accepted {
            PrimaryDisposition::Accepted
        } else {
            PrimaryDisposition::ProvenNondispatch
        };
        let slot = state
            .hard
            .slots
            .get_mut(&operation_id)
            .expect("validated hard-stop slot remains present");
        slot.phase = SlotPhase::Running(primary);
        let command = slot
            .command
            .take()
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        Ok(Some(HardStopRunOwner {
            coordinator: Arc::clone(self),
            operation_id,
            target: target.clone(),
            attempt,
            primary,
            cleanup,
            timeout,
            election: HardStopElection::Inherited(Some(inherited_election)),
            _command: Some(command),
            finished: false,
        }))
    }

    pub(super) fn finish_hard_without_run(
        &self,
        operation_id: StopOperationId,
    ) -> Result<(), StopCoordinationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let Some(slot) = state.hard.slots.get_mut(&operation_id) else {
            return Ok(());
        };
        if !matches!(slot.phase, SlotPhase::AwaitingPrimary) {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        let command = slot.command.take();
        slot.phase = SlotPhase::Finished(snapshot_result(
            operation_id,
            &slot.snapshot,
            HardStopTargetDisposition::UnavailableWithoutDispatch,
        ));
        let remove = slot.consumed && slot.joined_waiters == 0;
        if remove {
            state.hard.slots.remove(&operation_id);
        }
        drop(state);
        drop(command);
        self.hard_wake.notify_all();
        Ok(())
    }

    pub(super) fn consume_hard_slot(&self, operation_id: StopOperationId) {
        if let Ok(mut state) = self.state.lock() {
            let remove = if let Some(slot) = state.hard.slots.get_mut(&operation_id) {
                slot.consumed = true;
                slot.terminal_successor = false;
                slot.joined_waiters == 0 && matches!(slot.phase, SlotPhase::Finished(_))
            } else {
                false
            };
            if remove {
                state.hard.slots.remove(&operation_id);
            }
        }
        self.hard_wake.notify_all();
    }
}

fn snapshot_result(
    operation_id: StopOperationId,
    snapshot: &FrozenSnapshot,
    cleanup_disposition: HardStopTargetDisposition,
) -> BoundedHardStopResult {
    let mut targets = Vec::with_capacity(usize::from(snapshot.cleanup));
    if snapshot.cleanup {
        targets.push(HardStopTargetResult {
            target: HardStopTargetKind::CoarseThreadCleanup,
            disposition: cleanup_disposition,
        });
    }
    BoundedHardStopResult {
        operation_id,
        limitations: snapshot.limitations,
        targets,
    }
}

impl HardStopRunOwner {
    pub(in crate::cas_projection) const fn requires_fresh_election(&self) -> bool {
        matches!(&self.election, HardStopElection::FreshRequired)
    }

    pub(in crate::cas_projection) fn release_inherited_election_after_authorization(
        &mut self,
    ) -> Result<(), StopCoordinationError> {
        let HardStopElection::Inherited(permit) = &mut self.election else {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        };
        permit
            .take()
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?
            .finish();
        Ok(())
    }

    fn release_inherited_election_if_held(&mut self) {
        if let HardStopElection::Inherited(permit) = &mut self.election
            && let Some(permit) = permit.take()
        {
            permit.finish();
        }
    }

    pub(in crate::cas_projection) fn matches_proof(&self, proof: &StopTargetProof) -> bool {
        proof.matches(&self.target) && proof.request_timeout() == self.timeout
    }

    #[must_use]
    pub fn exact_target(&self) -> ExactForegroundTurn {
        ExactForegroundTurn::new(
            self.target.runtime_id(),
            self.target.loaded_generation(),
            self.target.cas_thread_id().clone(),
            self.target.cas_turn_id().clone(),
        )
    }

    #[must_use]
    pub fn operation_correlation(&self) -> StopOperationCorrelation {
        operation_correlation(self.operation_id)
    }

    #[must_use]
    pub fn attempt_correlation(&self) -> StopAttemptCorrelation {
        attempt_correlation(self.attempt)
    }

    #[must_use]
    pub const fn timeout(&self) -> std::time::Duration {
        self.timeout
    }

    #[must_use]
    pub const fn target(&self) -> Option<HardStopTargetKind> {
        if self.cleanup {
            Some(HardStopTargetKind::CoarseThreadCleanup)
        } else {
            None
        }
    }

    pub(in crate::cas_projection) fn finish(
        mut self,
        outcome: Option<&CoarseThreadCleanupOutcome>,
    ) -> Result<StopDispatchSettlement, StopCoordinationError> {
        let disposition = match (self.cleanup, outcome) {
            (false, None) => HardStopTargetDisposition::RequestAccepted,
            (true, Some(outcome)) => {
                if outcome.request().operation_correlation() != self.operation_correlation()
                    || outcome.request().attempt_correlation() != self.attempt_correlation()
                    || outcome.request().target() != &self.exact_target()
                {
                    return Err(StopCoordinationError::LocalAuthorityMismatch);
                }
                match outcome.disposition() {
                    CoarseThreadCleanupDisposition::RequestAccepted { .. } => {
                        HardStopTargetDisposition::RequestAccepted
                    }
                    CoarseThreadCleanupDisposition::ProvenNotDispatched { .. } => {
                        HardStopTargetDisposition::ProvenNotDispatched
                    }
                    CoarseThreadCleanupDisposition::SessionAuthorityInvalidated { .. } => {
                        HardStopTargetDisposition::SessionAuthorityLost
                    }
                    CoarseThreadCleanupDisposition::CompletionUnknown { .. } => {
                        HardStopTargetDisposition::CompletionUnknown
                    }
                }
            }
            _ => return Err(StopCoordinationError::LocalAuthorityMismatch),
        };
        self.release_inherited_election_if_held();
        let settlement = self.finish_with(disposition);
        self.finished = true;
        settlement
    }

    pub(in crate::cas_projection) fn finish_unavailable_without_dispatch(
        mut self,
    ) -> Result<StopDispatchSettlement, StopCoordinationError> {
        let disposition = if self.cleanup {
            HardStopTargetDisposition::UnavailableWithoutDispatch
        } else {
            HardStopTargetDisposition::RequestAccepted
        };
        self.release_inherited_election_if_held();
        let settlement = self.finish_with(disposition);
        self.finished = true;
        settlement
    }

    fn finish_with(
        &self,
        disposition: HardStopTargetDisposition,
    ) -> Result<StopDispatchSettlement, StopCoordinationError> {
        if self.coordinator.failure_cut_is_active() {
            return Ok(StopDispatchSettlement::Stopping(self.operation_id));
        }
        let authority_lost = matches!(
            disposition,
            HardStopTargetDisposition::SessionAuthorityLost
                | HardStopTargetDisposition::CompletionUnknown
        );
        let terminal_consumed = self
            .coordinator
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?
            .hard
            .slots
            .get(&self.operation_id)
            .is_some_and(|slot| slot.terminal_successor);
        let settlement = match self.primary {
            PrimaryDisposition::ProvenNondispatch if terminal_consumed => {
                Ok(StopDispatchSettlement::Stopping(self.operation_id))
            }
            PrimaryDisposition::ProvenNondispatch => self
                .coordinator
                .settle_proven_nondispatch(self.operation_id, Some(self.attempt)),
            PrimaryDisposition::Accepted if authority_lost && !terminal_consumed => {
                self.coordinator.abandon(
                    self.operation_id,
                    syndic_storage::StopAbandonmentReason::TargetAuthorityLost,
                )
            }
            PrimaryDisposition::Accepted => Ok(StopDispatchSettlement::Stopping(self.operation_id)),
        };
        let mut state = self
            .coordinator
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        if settlement.is_err()
            && let Some(local) = state.stops.get_mut(&self.operation_id.thread_id())
            && local.operation_id == self.operation_id
            && local.attempt == Some(self.attempt)
        {
            local.dispatch = LocalDispatchState::PossiblyDispatched;
        }
        let slot = state
            .hard
            .slots
            .get_mut(&self.operation_id)
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        if !matches!(slot.phase, SlotPhase::Running(primary) if primary == self.primary) {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        if matches!(
            &settlement,
            Ok(StopDispatchSettlement::SafelyReopened(_) | StopDispatchSettlement::Abandoned(_))
        ) {
            slot.consumed = true;
        }
        slot.phase = SlotPhase::Finished(snapshot_result(
            self.operation_id,
            &slot.snapshot,
            disposition,
        ));
        let remove = slot.consumed && slot.joined_waiters == 0;
        if remove {
            state.hard.slots.remove(&self.operation_id);
        }
        drop(state);
        self.coordinator.hard_wake.notify_all();
        settlement
    }

    fn orphan_implicit_drop(&self) {
        if self.coordinator.failure_cut_is_active() {
            return;
        }
        let Ok(mut state) = self.coordinator.state.lock() else {
            return;
        };
        if let Some(local) = state.stops.get_mut(&self.operation_id.thread_id())
            && local.operation_id == self.operation_id
            && local.attempt == Some(self.attempt)
        {
            local.dispatch = match local.dispatch {
                LocalDispatchState::AdmittedNotClaimed
                | LocalDispatchState::ClaimedNotDispatched
                | LocalDispatchState::ProvenNondispatchSettling => {
                    LocalDispatchState::ClaimUnresolved
                }
                LocalDispatchState::Dispatching
                | LocalDispatchState::HardStopRunningProvenNondispatch
                | LocalDispatchState::PrimaryAccepted => LocalDispatchState::PossiblyDispatched,
                LocalDispatchState::ClaimUnresolved
                | LocalDispatchState::PossiblyDispatched
                | LocalDispatchState::DurablyAbandoned
                | LocalDispatchState::FailureFrozenNondispatch => local.dispatch,
            };
        }
        let Some(slot) = state.hard.slots.get_mut(&self.operation_id) else {
            return;
        };
        if !matches!(slot.phase, SlotPhase::Running(primary) if primary == self.primary) {
            return;
        }
        let disposition = if self.cleanup {
            HardStopTargetDisposition::UnavailableWithoutDispatch
        } else {
            HardStopTargetDisposition::RequestAccepted
        };
        slot.phase = SlotPhase::Finished(snapshot_result(
            self.operation_id,
            &slot.snapshot,
            disposition,
        ));
        let remove = slot.consumed && slot.joined_waiters == 0;
        if remove {
            state.hard.slots.remove(&self.operation_id);
        }
        drop(state);
        self.coordinator.hard_wake.notify_all();
    }
}

impl Drop for HardStopRunOwner {
    fn drop(&mut self) {
        if !self.finished {
            self.release_inherited_election_if_held();
            self.orphan_implicit_drop();
        }
    }
}
