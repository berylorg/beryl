use std::sync::atomic::Ordering;

use beryl_backend::ResponseWorkObserver;

use super::{EventRouter, RouterState, TargetEntry, TargetTurn, state::advance_revision};
use crate::cas_projection::connection_work::{
    ConnectionRequestWorkFact, ConnectionRequestWorkKind, ConnectionRequestWorkStage,
    ConnectionTargetWorkFact, ConnectionTargetWorkState, ConnectionWorkError,
    ConnectionWorkItemKey, ConnectionWorkKey, ConnectionWorkPageBuilder, ConnectionWorkRecord,
    ConnectionWorkStamp, ConnectionWorkTargetIdentity,
};

const RESPONSE_OBSERVATION_CAPACITY: usize =
    super::LIVE_EVENT_TARGET_CAPACITY * super::TARGET_OPERATION_QUEUE_CAPACITY + 1;

#[derive(Debug)]
pub(super) struct RequestObservation {
    identity: ConnectionWorkTargetIdentity,
    turn_id: super::CasTurnId,
    kind: ConnectionRequestWorkKind,
    stage: ConnectionRequestWorkStage,
    response: ResponseWorkObserver,
}

impl RouterState {
    pub(super) fn request_work_stage(
        &mut self,
        serial: Option<u64>,
        stage: ConnectionRequestWorkStage,
    ) {
        if let Some(request) = serial.and_then(|serial| self.work_requests.get_mut(&serial))
            && request.stage != stage
        {
            request.stage = stage;
            advance_revision(self);
        }
    }

    fn prune_response_observations(&mut self) {
        let before = self.work_requests.len();
        let mut available = true;
        self.work_requests.retain(|_, request| {
            let registered = self
                .targets
                .get(&request.identity.cas_thread_id)
                .is_some_and(|target| target.registration == request.identity.registration_serial);
            if !registered && request.stage != ConnectionRequestWorkStage::ResponseAdmitted {
                return false;
            }
            match request.response.snapshot() {
                Ok(snapshot) => {
                    !snapshot.response_written() && snapshot.retained_capabilities() != 0
                }
                Err(_) => {
                    available = false;
                    true
                }
            }
        });
        if self.work_requests.len() != before {
            advance_revision(self);
        }
        if !available {
            self.work_revision = None;
        }
    }
}

impl EventRouter {
    fn work_target_identity(&self, target: &TargetEntry) -> ConnectionWorkTargetIdentity {
        ConnectionWorkTargetIdentity {
            runtime_id: self.runtime_id,
            process_generation: self.process_generation,
            connection_generation: self.connection_generation,
            registration_serial: target.registration,
            thread_id: target.owner,
            cas_thread_id: target.key.cas_thread_id.clone(),
            loaded_generation: target.loaded_generation,
            home_generation: target.home_generation,
        }
    }

    pub(super) fn observe_request(
        &self,
        state: &mut RouterState,
        thread_id: &super::CasThreadId,
        turn_id: &super::CasTurnId,
        kind: ConnectionRequestWorkKind,
        response: ResponseWorkObserver,
    ) -> Option<u64> {
        state.prune_response_observations();
        let serial = state
            .next_work_request
            .and_then(|serial| serial.checked_add(1));
        if state.work_requests.len() >= RESPONSE_OBSERVATION_CAPACITY || serial.is_none() {
            state.work_revision = None;
            return None;
        }
        let target = state.targets.get(thread_id)?;
        if response
            .register_completion_waker(self.scheduler_signal.idle_recheck_waker())
            .is_err()
        {
            state.work_revision = None;
            return None;
        }
        let observation = RequestObservation {
            identity: self.work_target_identity(target),
            turn_id: turn_id.clone(),
            kind,
            stage: ConnectionRequestWorkStage::Ingress,
            response,
        };
        let serial = serial?;
        state.next_work_request = Some(serial);
        state.work_requests.insert(serial, observation);
        advance_revision(state);
        Some(serial)
    }

    pub(in crate::cas_projection) fn work_stamp(
        &self,
    ) -> Result<ConnectionWorkStamp, ConnectionWorkError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ConnectionWorkError::Poisoned)?;
        let mut responses = 0_u64;
        for request in state.work_requests.values() {
            let snapshot = request
                .response
                .snapshot()
                .map_err(|_| ConnectionWorkError::SourceUnavailable)?;
            responses = responses
                .checked_add(snapshot.revision().change_count())
                .ok_or(ConnectionWorkError::RevisionUnavailable)?;
        }
        Ok(ConnectionWorkStamp {
            routers: state
                .work_revision
                .ok_or(ConnectionWorkError::RevisionUnavailable)?,
            responses,
            ..ConnectionWorkStamp::default()
        })
    }

    pub(in crate::cas_projection) fn collect_work_records(
        &self,
        retired: bool,
        page: &mut ConnectionWorkPageBuilder<'_>,
    ) -> Result<bool, ConnectionWorkError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ConnectionWorkError::Poisoned)?;
        state
            .work_revision
            .ok_or(ConnectionWorkError::RevisionUnavailable)?;
        let retired = retired || state.retired.is_some();
        for (thread_id, target) in &state.targets {
            let key = ConnectionWorkKey {
                connection: self.connection_generation,
                item: ConnectionWorkItemKey::Target(thread_id.clone()),
            };
            if page.after.is_some_and(|after| &key <= after) {
                continue;
            }
            let record = ConnectionWorkRecord::Target(ConnectionTargetWorkFact {
                identity: self.work_target_identity(target),
                turn_id: target.turn_id.clone(),
                state: match target.turn_state {
                    TargetTurn::AwaitingStart => ConnectionTargetWorkState::AwaitingStart,
                    TargetTurn::AwaitingCompactionTurn => {
                        ConnectionTargetWorkState::AwaitingCompactionTurn
                    }
                    TargetTurn::Exact => ConnectionTargetWorkState::Executing,
                    TargetTurn::Terminal => ConnectionTargetWorkState::Terminal,
                },
                start_dispatched: target.start_dispatched,
                activation_durable: target.activation_durable,
                publication_pending: target.publication_in_flight.is_some(),
                closing: target.publication_closing.or(state.retired),
                loss_requested: target.loss_requested,
                queued_operations: target.queued_operations.load(Ordering::Acquire),
                connection_retired: retired,
            });
            if !page.push(key, record)? {
                return Ok(false);
            }
        }
        for (serial, request) in &state.work_requests {
            let key = ConnectionWorkKey {
                connection: self.connection_generation,
                item: ConnectionWorkItemKey::Request(*serial),
            };
            if page.after.is_some_and(|after| &key <= after) {
                continue;
            }
            let record = ConnectionWorkRecord::Request(ConnectionRequestWorkFact {
                identity: request.identity.clone(),
                request_serial: *serial,
                turn_id: request.turn_id.clone(),
                kind: request.kind,
                stage: request.stage,
                response: request
                    .response
                    .snapshot()
                    .map_err(|_| ConnectionWorkError::SourceUnavailable)?,
                target_registered: state
                    .targets
                    .get(&request.identity.cas_thread_id)
                    .is_some_and(|target| {
                        target.registration == request.identity.registration_serial
                    }),
                connection_retired: retired,
            });
            if !page.push(key, record)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
