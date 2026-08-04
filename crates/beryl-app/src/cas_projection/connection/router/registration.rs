use std::{
    sync::{Arc, Mutex, atomic::AtomicUsize, mpsc},
    time::Duration,
};

use beryl_model::{CasLoadedSessionGeneration, SyndicThreadId};

use super::{
    EventRouter, LiveEventTargetRegistrationError, TargetEntry, TargetRegistration,
    TargetTerminalSignal, TargetTurn, TargetTurnRegistration, state::advance_revision,
};

impl EventRouter {
    pub(in crate::cas_projection) fn register(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
        key: super::LoadedThreadKey,
        owner: SyndicThreadId,
        loaded_generation: CasLoadedSessionGeneration,
        home_generation: u64,
        request_timeout: Duration,
        turn: TargetTurnRegistration,
    ) -> Result<TargetRegistration, LiveEventTargetRegistrationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| LiveEventTargetRegistrationError::RouterPoisoned)?;
        let committed = command.commit_if_current(|| {
            if state.retired.is_some() || state.persistent_failure.is_some() {
                return Err(LiveEventTargetRegistrationError::ConnectionRetired);
            }
            if state.targets.contains_key(&key.cas_thread_id) {
                return Err(LiveEventTargetRegistrationError::TargetAlreadyRegistered {
                    thread_id: key.cas_thread_id.clone(),
                });
            }
            if state.retired_thread_lanes.contains(&key.cas_thread_id) {
                return Err(LiveEventTargetRegistrationError::TargetGenerationRetired {
                    thread_id: key.cas_thread_id.clone(),
                });
            }
            if let TargetTurnRegistration::Pending(activation) = &turn
                && activation.thread_id() != owner
            {
                return Err(LiveEventTargetRegistrationError::ActivationAuthorityMismatch);
            }
            if state.targets.len() >= super::LIVE_EVENT_TARGET_CAPACITY {
                return Err(LiveEventTargetRegistrationError::TargetCapacityFull {
                    capacity: super::LIVE_EVENT_TARGET_CAPACITY,
                });
            }
            let (
                turn_state,
                turn_id,
                start_dispatched,
                activation_durable,
                pending_activation,
                compaction,
            ) = match turn {
                TargetTurnRegistration::Pending(activation) => (
                    TargetTurn::AwaitingStart,
                    None,
                    false,
                    false,
                    Some(activation),
                    None,
                ),
                TargetTurnRegistration::Active(turn_id) => {
                    (TargetTurn::Exact, Some(turn_id), true, true, None, None)
                }
                TargetTurnRegistration::ContextCompaction(authority) => (
                    TargetTurn::AwaitingCompactionTurn,
                    None,
                    false,
                    true,
                    None,
                    Some(authority),
                ),
            };
            let target_ready = turn_state == TargetTurn::Exact && activation_durable;
            state.next_registration = state
                .next_registration
                .checked_add(1)
                .ok_or(LiveEventTargetRegistrationError::GenerationExhausted)?;
            let registration = state.next_registration;
            let (sender, receiver) = mpsc::sync_channel(super::TARGET_OPERATION_QUEUE_CAPACITY);
            let queued_operations = Arc::new(AtomicUsize::new(0));
            let terminal = Arc::new(Mutex::new(TargetTerminalSignal::Open));
            let loss_receipt = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let bound_turn = Arc::new(Mutex::new(turn_id.clone()));
            state.targets.insert(
                key.cas_thread_id.clone(),
                TargetEntry {
                    registration,
                    key: key.clone(),
                    owner,
                    loaded_generation,
                    home_generation,
                    request_timeout,
                    turn_state,
                    turn_id,
                    start_dispatched,
                    activation_durable,
                    pending_activation: pending_activation.clone(),
                    compaction,
                    dynamic_tool_responses: std::collections::HashMap::new(),
                    sender: Some(sender),
                    queued_operations: Arc::clone(&queued_operations),
                    terminal: Arc::clone(&terminal),
                    bound_turn: Arc::clone(&bound_turn),
                    publication_in_flight: None,
                    publication_closing: None,
                    loss_requested: false,
                    loss_receipt: Arc::clone(&loss_receipt),
                    persistent_failure_projection: None,
                },
            );
            advance_revision(&mut state);
            Ok((
                TargetRegistration {
                    registration,
                    key,
                    owner,
                    loaded_generation,
                    receiver,
                    queued_operations,
                    terminal,
                    loss_receipt,
                    compaction,
                    #[cfg(test)]
                    bound_turn,
                },
                target_ready,
            ))
        });
        let (registration, target_ready) =
            committed.map_err(|_| LiveEventTargetRegistrationError::ConnectionRetired)??;
        drop(state);
        if target_ready {
            self.scheduler_signal.wake(
                crate::cas_projection::accepted_input_scheduler::AcceptedInputWakeReason::TargetReady,
            );
        }
        Ok(registration)
    }
}
