use super::*;
use crate::cas_projection::{
    CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    ProjectionCancellationToken, RuntimeFailureSnapshot,
};
use syndic_storage::{
    NativeProjectionRecoveryBasis, NativeProjectionRecoveryPlan, NativeProjectionRequest,
    SelectedPathProof, SyndicTimestamp,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub(crate) enum SelectedProjectionRecoveryError {
    #[error("selected projection recovery is unavailable")]
    Unavailable,
    #[error("selected projection recovery was cancelled")]
    Cancelled,
    #[error("the exact selected projection operation failed")]
    Failed,
}

impl From<RuntimeSessionPreparationError> for SelectedProjectionRecoveryError {
    fn from(_: RuntimeSessionPreparationError) -> Self {
        Self::Unavailable
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RetainedSelectedProjection {
    failure: RuntimeFailureSnapshot,
    owner: Weak<Mutex<SessionState>>,
    registration: ScheduledSessionRegistration,
    generation: beryl_model::CasLoadedSessionGeneration,
    binding_revision: beryl_model::BindingRevision,
    cas_thread_id: beryl_model::CasThreadId,
    lineage: syndic_storage::CasLineageProof,
    basis: NativeProjectionRecoveryBasis,
}

struct RecoveryReservation {
    active: bool,
    sessions: ScheduledExecutionSessions,
    thread_id: SyndicThreadId,
    registration: Option<ScheduledSessionRegistration>,
    resources: Option<SessionResources>,
}

impl Drop for RecoveryReservation {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        {
            let mut state = self.sessions.lock();
            state.recovering.remove(&self.thread_id);
            state.work_changed();
        }
        if let Some(registration) = self.registration.take()
            && let Some(resources) = self.resources.take()
        {
            self.sessions.settle_return(registration, resources);
        }
    }
}

impl ScheduledExecutionSessions {
    pub(crate) fn downgrade(&self) -> WeakScheduledExecutionSessions {
        WeakScheduledExecutionSessions {
            state: Arc::downgrade(&self.state),
            work_identity: Arc::downgrade(&self.work_identity),
        }
    }

    pub(crate) fn selected_projection_recovery_eligible(
        &self,
        snapshot: RuntimeFailureSnapshot,
        thread_id: SyndicThreadId,
        binding: &ExecutionBinding,
    ) -> bool {
        let (context, has_session) = {
            let state = self.lock();
            if state.closed
                || state.recovering.contains_key(&thread_id)
                || state.preparing.contains_key(&thread_id)
                || state
                    .slots
                    .get(&thread_id)
                    .is_some_and(|slot| slot.checked_out || slot.retiring)
                || (!state.slots.contains_key(&thread_id)
                    && state.context.as_ref().is_none_or(|provider| {
                        state.slots.len() + state.recovering.len() >= provider.capacity
                    }))
            {
                return false;
            }
            let Some(context) = state.preparation.clone() else {
                return false;
            };
            (context, state.slots.contains_key(&thread_id))
        };
        if context.launch_spec_for(thread_id, binding).is_err()
            || !context.owner.selected_retry_eligible(snapshot, binding)
            || context.commands.execution_candidate().is_err()
        {
            return false;
        }
        if !has_session
            && !context.workers.preparation_capacity_available(
                context
                    .owner
                    .failure_snapshot(binding.runtime_id())
                    .is_some(),
            )
        {
            return false;
        }
        let Ok(request) = recovery_request(&context, thread_id, binding) else {
            return false;
        };
        matches!(
            context.storage.prepare_native_projection_recovery(
                &context.home,
                &NativeProjectionRequest::new(
                    thread_id,
                    request.selected_path(),
                    binding.clone(),
                    crate::conversation_tools::ConversationToolRegistry::canonical().profile()
                ),
                crate::cas_projection::input_replay::point_limit(),
            ),
            Ok(NativeProjectionRecoveryPlan::Ready { .. })
        )
    }

    pub(crate) fn recover_selected_projection(
        &self,
        snapshot: RuntimeFailureSnapshot,
        thread_id: SyndicThreadId,
        binding: &ExecutionBinding,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<RetainedSelectedProjection, SelectedProjectionRecoveryError> {
        self.reap();
        let unavailable = SelectedProjectionRecoveryError::Unavailable;
        if cancellation.is_cancelled() {
            return Err(SelectedProjectionRecoveryError::Cancelled);
        }
        let (context, mut reservation) = {
            let mut state = self.lock();
            let context = state.preparation.clone().ok_or(unavailable)?;
            let provider = state.context.as_ref().ok_or(unavailable)?;
            if state.closed
                || !context.commands.is_open()
                || state.preparing.contains_key(&thread_id)
                || state.recovering.contains_key(&thread_id)
                || cancellation.is_cancelled()
                || (!state.slots.contains_key(&thread_id)
                    && state.slots.len() + state.recovering.len() >= provider.capacity)
            {
                return Err(unavailable);
            }
            let (registration, resources) = if let Some(slot) = state.slots.get_mut(&thread_id) {
                if slot.checked_out || slot.retiring || &slot.binding != binding {
                    return Err(unavailable);
                }
                let resources = slot.resources.take().ok_or(unavailable)?;
                slot.checked_out = true;
                (Some(slot.registration), Some(resources))
            } else {
                (None, None)
            };
            state.recovering.insert(thread_id, binding.clone());
            state.work_changed();
            (
                context,
                RecoveryReservation {
                    active: true,
                    sessions: self.clone(),
                    thread_id,
                    registration,
                    resources,
                },
            )
        };
        if !context.owner.selected_retry_eligible(snapshot, binding) {
            return Err(unavailable);
        }
        let acquisition =
            crate::cas_projection::acquisition::ProjectionAcquisition::admit(&context.commands)
                .map_err(|_| unavailable)?;
        let coordinator =
            CasProjectionCoordinator::for_healthy_home(&context.home).map_err(|_| unavailable)?;
        let flight = coordinator
            .begin_projection(thread_id)
            .map_err(|_| unavailable)?
            .with_acquisition(acquisition.clone());
        let request = recovery_request(&context, thread_id, binding)?;
        let NativeProjectionRecoveryPlan::Ready { plan, basis } = context
            .storage
            .prepare_native_projection_recovery(
                &context.home,
                &NativeProjectionRequest::new(
                    thread_id,
                    request.selected_path(),
                    binding.clone(),
                    crate::conversation_tools::ConversationToolRegistry::canonical().profile(),
                ),
                crate::cas_projection::input_replay::point_limit(),
            )
            .map_err(|_| unavailable)?
        else {
            return Err(unavailable);
        };
        if reservation.resources.is_none() {
            let session = context.prepare_recovery_session(
                self,
                snapshot,
                thread_id,
                binding,
                cancellation,
                &acquisition,
            )?;
            reservation.resources = Some(SessionResources {
                session,
                tools: Box::new(context.tools.clone()),
                projection: None,
            });
        }
        let resources = reservation.resources.as_mut().ok_or(unavailable)?;
        if !context
            .storage
            .validate_native_projection_recovery_basis(
                &context.home,
                &basis,
                crate::cas_projection::input_replay::point_limit(),
            )
            .map_err(|_| unavailable)?
            || cancellation.is_cancelled()
        {
            return Err(unavailable);
        }
        let retained = resources.projection.take();
        let projection = if retained.as_ref().is_some_and(|projection| {
            projection_matches(projection, &basis) && projection.is_live().unwrap_or(false)
        }) {
            retained.expect("matching retained projection")
        } else {
            drop(retained);
            coordinator
                .obtain_selected_recovery_projection(
                    &context.home,
                    &context.storage,
                    &mut resources.session,
                    &request,
                    cancellation,
                    &flight,
                    plan,
                )
                .map_err(|error| match error {
                    crate::cas_projection::ProjectionExecutionError::Cancelled => {
                        SelectedProjectionRecoveryError::Cancelled
                    }
                    _ => SelectedProjectionRecoveryError::Failed,
                })?
        };
        let successor = context
            .storage
            .native_projection_recovery_successor_basis(
                &context.home,
                &basis,
                projection.binding_revision(),
                crate::cas_projection::input_replay::point_limit(),
            )
            .map_err(|_| unavailable)?
            .ok_or(unavailable)?;
        if !projection_matches(&projection, &successor)
            || !projection.is_live().unwrap_or(false)
            || cancellation.is_cancelled()
            || !context.owner.selected_retry_eligible(snapshot, binding)
        {
            return Err(unavailable);
        }
        let mut state = self.lock();
        let provider = state.context.as_ref().ok_or(unavailable)?;
        if state.closed || !context.commands.is_open() || !provider.owns(&resources.session) {
            return Err(unavailable);
        }
        let registration = if let Some(registration) = reservation.registration {
            let slot = state.slots.get(&thread_id).ok_or(unavailable)?;
            if slot.registration != registration || slot.retiring || !slot.checked_out {
                return Err(unavailable);
            }
            registration
        } else {
            let serial = state.next_serial.checked_add(1).ok_or(unavailable)?;
            let registration = ScheduledSessionRegistration {
                service_generation: provider.service_generation,
                thread_id,
                serial,
            };
            state.next_serial = serial;
            state.slots.insert(
                thread_id,
                SessionSlot {
                    registration,
                    binding: binding.clone(),
                    policy: context.config.policy.clone(),
                    assets: context.config.assets.clone(),
                    connection: Arc::clone(resources.session.connection()),
                    resources: None,
                    checked_out: true,
                    retiring: false,
                },
            );
            state.high_water = state.high_water.max(state.slots.len());
            registration
        };
        let proof = RetainedSelectedProjection {
            failure: snapshot,
            owner: Arc::downgrade(&self.state),
            registration,
            generation: projection.loaded_session_generation(),
            binding_revision: projection.binding_revision(),
            cas_thread_id: projection.cas_thread_id().clone(),
            lineage: projection.lineage_proof(),
            basis: successor,
        };
        resources.projection = Some(projection);
        let slot = state.slots.get_mut(&thread_id).ok_or(unavailable)?;
        slot.resources = reservation.resources.take();
        slot.checked_out = false;
        state.recovering.remove(&thread_id);
        state.work_changed();
        reservation.registration = None;
        reservation.active = false;
        let ready = state.context.as_ref().map(|context| context.ready.clone());
        drop(state);
        if let Some(ready) = ready {
            ready.notify();
        }
        Ok(proof)
    }

    pub(crate) fn retained_selected_projection_matches(
        &self,
        proof: &RetainedSelectedProjection,
    ) -> bool {
        let context = {
            let state = self.lock();
            if state.closed || !Weak::ptr_eq(&proof.owner, &Arc::downgrade(&self.state)) {
                return false;
            }
            let Some(context) = state.preparation.clone() else {
                return false;
            };
            context
        };
        context
            .storage
            .validate_native_projection_recovery_basis(
                &context.home,
                &proof.basis,
                crate::cas_projection::input_replay::point_limit(),
            )
            .unwrap_or(false)
            && self
                .with_retained_selected_projection(proof, || ())
                .is_some()
    }

    pub(crate) fn with_retained_selected_projection<T>(
        &self,
        proof: &RetainedSelectedProjection,
        publish: impl FnOnce() -> T,
    ) -> Option<T> {
        let state = self.state.try_lock().ok()?;
        if state.closed || !Weak::ptr_eq(&proof.owner, &Arc::downgrade(&self.state)) {
            return None;
        }
        let slot = state.slots.get(&proof.registration.thread_id)?;
        if slot.registration != proof.registration || slot.retiring || slot.checked_out {
            return None;
        }
        let projection = slot.resources.as_ref()?.projection.as_ref()?;
        if projection.loaded_session_generation() != proof.generation
            || projection.binding_revision() != proof.binding_revision
            || projection.cas_thread_id() != &proof.cas_thread_id
            || projection.lineage_proof() != proof.lineage
            || !projection_matches(projection, &proof.basis)
        {
            return None;
        }
        state
            .preparation
            .as_ref()?
            .owner
            .try_with_selected_retry_ready(proof.failure, proof.generation.process(), || {
                projection.try_with_live_authority(publish)
            })
            .flatten()
    }
}

fn projection_matches(
    projection: &LoadedCasProjection,
    basis: &NativeProjectionRecoveryBasis,
) -> bool {
    basis.source().is_some_and(|source| {
        source.thread_id() == projection.syndic_thread_id()
            && source.binding_revision() == projection.binding_revision()
            && source.binding().cas_thread_id() == projection.cas_thread_id()
            && source.binding().lineage() == projection.lineage_proof()
            && source.binding().execution() == projection.execution_binding()
    })
}

fn recovery_request(
    context: &preparation::PreparationContext,
    thread_id: SyndicThreadId,
    binding: &ExecutionBinding,
) -> Result<CasProjectionRequest, RuntimeSessionPreparationError> {
    let unavailable = RuntimeSessionPreparationError::RetryUnavailable;
    let thread = context
        .storage
        .thread(
            &context.home,
            thread_id,
            crate::cas_projection::input_replay::point_limit(),
        )
        .map_err(|_| unavailable)?
        .ok_or(unavailable)?;
    Ok(CasProjectionRequest::new(
        thread_id,
        SelectedPathProof::new(
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
        ),
        binding.clone(),
        context.config.policy.thread_options().clone(),
        context.config.policy.model_context_window_tokens(),
        SyndicTimestamp::from_unix_millis(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
                .ok_or(unavailable)?,
        ),
        context.config.policy.projection_timeout(),
    ))
}
