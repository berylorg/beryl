use std::sync::Arc;

use beryl_backend::{
    CheckedSteeringUserMessage, OrderedTurnStreamRejection, OrderedTurnStreamSubmitCause,
    SteeringUserMessageAbandonReason, SteeringUserMessageSelection, SteeringUserMessageSource,
};
use beryl_state::{AssetOwner, BerylState};

use super::{ActiveSteeringLifecycle, BrokerReply, Ingester};
use crate::cas_projection::connection::provider_broker::steering_result::{
    CheckedSteeringLifecycle, SteeringSelectionReservationError,
};
use crate::cas_projection::{
    ProjectionCancellationToken,
    connection::router::{
        DelayedSteeringLifecycleFinishError, DelayedSteeringLifecyclePermit,
        DelayedSteeringLifecyclePermitError,
    },
    input_replay::{
        AcceptedInputReplayContext, AcceptedInputReplayFactory,
        decode_accepted_input_steering_correlation, point_limit,
    },
};

mod replay;

use replay::AcceptedSteeringReplaySource;

impl Ingester {
    pub(super) fn select_steering_user_message(
        &mut self,
        selection: SteeringUserMessageSelection,
    ) -> (BrokerReply, bool) {
        if self.active.is_some() {
            return self.reject_steering_selection(
                selection,
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl),
            );
        }
        let input_id =
            match decode_accepted_input_steering_correlation(selection.client_user_message_id()) {
                Ok(input_id) => input_id,
                Err(_) => {
                    return self.reject_steering_selection(
                        selection,
                        OrderedTurnStreamSubmitCause::Rejected(
                            OrderedTurnStreamRejection::SchemaMismatch,
                        ),
                    );
                }
            };
        let Some((home_generation, storage)) = self.current_observation_authority() else {
            return self.reject_steering_selection(
                selection,
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::StagingConflict),
            );
        };
        let route = match storage.delivering_steering_input(&self.home, input_id, point_limit()) {
            Ok(Some(route)) => route,
            Ok(None) => {
                return self.reject_steering_selection(
                    selection,
                    OrderedTurnStreamSubmitCause::Rejected(
                        OrderedTurnStreamRejection::InvalidControl,
                    ),
                );
            }
            Err(_) => {
                return self.reject_steering_selection(
                    selection,
                    OrderedTurnStreamSubmitCause::Rejected(
                        OrderedTurnStreamRejection::StagingConflict,
                    ),
                );
            }
        };
        let proof = match self
            .steering_results
            .reserve(&route, home_generation, &selection)
        {
            Ok(proof) => proof,
            Err(error) => {
                let cause = match error {
                    SteeringSelectionReservationError::Closed => {
                        OrderedTurnStreamSubmitCause::Cancelled
                    }
                    SteeringSelectionReservationError::CapacityFull => {
                        OrderedTurnStreamSubmitCause::CapacityFull
                    }
                    SteeringSelectionReservationError::InvalidSequence => {
                        OrderedTurnStreamSubmitCause::Rejected(
                            OrderedTurnStreamRejection::InvalidControl,
                        )
                    }
                };
                return self.reject_steering_selection(selection, cause);
            }
        };

        let permit = match self.router.acquire_delayed_steering_lifecycle(
            self.live_command(),
            route.target().pending().cas_thread_id(),
            route.target().cas_turn_id(),
        ) {
            Ok(permit) => permit,
            Err(error) => return self.failed_steering_selection(error, selection),
        };
        if !permit.matches_target(
            route.input().thread_id(),
            route.target(),
            route.loaded_generation(),
        ) || permit.home_generation(&self.home) != Some(home_generation)
        {
            return self.failed_selected_permit(
                permit,
                selection,
                OrderedTurnStreamRejection::InvalidControl,
            );
        }

        let Some((confirmed_generation, confirmed_storage)) = self.current_observation_authority()
        else {
            return self.failed_selected_permit(
                permit,
                selection,
                OrderedTurnStreamRejection::StagingConflict,
            );
        };
        let confirmed = match confirmed_storage.delivering_steering_input(
            &self.home,
            input_id,
            point_limit(),
        ) {
            Ok(Some(confirmed))
                if confirmed_generation == home_generation && confirmed == route =>
            {
                confirmed
            }
            _ => {
                return self.failed_selected_permit(
                    permit,
                    selection,
                    OrderedTurnStreamRejection::StagingConflict,
                );
            }
        };
        let state = match BerylState::reacquire(&self.home) {
            Ok(state) => state,
            Err(_) => {
                return self.failed_selected_permit(
                    permit,
                    selection,
                    OrderedTurnStreamRejection::StagingConflict,
                );
            }
        };
        let owner_head = match state
            .assets()
            .owner_head(&self.home, AssetOwner::AcceptedInput(input_id))
        {
            Ok(owner_head) => owner_head,
            Err(_) => {
                return self.failed_selected_permit(
                    permit,
                    selection,
                    OrderedTurnStreamRejection::StagingConflict,
                );
            }
        };
        let replay_cancellation =
            ProjectionCancellationToken::from_shared_flag(Arc::clone(&self.cancelled));
        let replay = match AcceptedInputReplayFactory::prepare(
            &self.home,
            confirmed_storage,
            state.assets(),
            AcceptedInputReplayContext::new(
                self.home_id,
                home_generation,
                confirmed.execution().root_path().mode().clone(),
            ),
            confirmed.input().clone(),
            owner_head,
            &replay_cancellation,
        ) {
            Ok(replay) => replay,
            Err(_) => {
                return self.failed_selected_permit(
                    permit,
                    selection,
                    OrderedTurnStreamRejection::StagingConflict,
                );
            }
        };
        let header = replay.header();
        let source = SteeringUserMessageSource::new(
            confirmed.target().pending().cas_thread_id().clone(),
            confirmed.target().cas_turn_id().clone(),
            Box::new(AcceptedSteeringReplaySource::new(
                Arc::clone(&self.home),
                Arc::clone(&self.cancelled),
                replay.fresh_source(),
            )),
        );
        #[cfg(all(test, feature = "test-faults"))]
        let selected_lifecycle = selection.lifecycle();
        self.put_steering(ActiveSteeringLifecycle {
            selection,
            header,
            proof,
            permit,
        });
        #[cfg(all(test, feature = "test-faults"))]
        crate::cas_projection::test_faults::pause_steering_after_selection(
            crate::cas_projection::test_faults::ProviderTestKey::new(
                self.home_id,
                Arc::as_ptr(&self.cancelled) as usize,
            ),
            selected_lifecycle,
        );
        (BrokerReply::SteeringSelected(source), false)
    }

    fn reject_steering_selection(
        &mut self,
        selection: SteeringUserMessageSelection,
        cause: OrderedTurnStreamSubmitCause,
    ) -> (BrokerReply, bool) {
        if self.exact_persistent_failure() {
            return (
                BrokerReply::SteeringSelectionRejected(selection, cause),
                true,
            );
        }
        self.steering_results.close();
        self.retire();
        (
            BrokerReply::SteeringSelectionRejected(selection, cause),
            true,
        )
    }

    fn failed_steering_selection(
        &mut self,
        error: DelayedSteeringLifecyclePermitError,
        selection: SteeringUserMessageSelection,
    ) -> (BrokerReply, bool) {
        if self.exact_persistent_failure() {
            return (
                BrokerReply::SteeringSelectionRejected(
                    selection,
                    OrderedTurnStreamSubmitCause::Rejected(
                        OrderedTurnStreamRejection::InvalidControl,
                    ),
                ),
                true,
            );
        }
        self.steering_results.abandon_selection();
        if let DelayedSteeringLifecyclePermitError::Target(target) = error {
            let _ = self.invalidate_target(target);
        }
        self.retire();
        (
            BrokerReply::SteeringSelectionRejected(
                selection,
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl),
            ),
            true,
        )
    }

    fn failed_selected_permit(
        &mut self,
        permit: DelayedSteeringLifecyclePermit,
        selection: SteeringUserMessageSelection,
        rejection: OrderedTurnStreamRejection,
    ) -> (BrokerReply, bool) {
        if self.exact_persistent_failure() {
            drop(permit);
            return (
                BrokerReply::SteeringSelectionRejected(
                    selection,
                    OrderedTurnStreamSubmitCause::Rejected(rejection),
                ),
                true,
            );
        }
        self.steering_results.abandon_selection();
        match permit.fail() {
            Ok(target) | Err(DelayedSteeringLifecycleFinishError::Target(target)) => {
                let _ = self.invalidate_target(target);
            }
            Err(DelayedSteeringLifecycleFinishError::Router) => {}
        }
        self.retire();
        (
            BrokerReply::SteeringSelectionRejected(
                selection,
                OrderedTurnStreamSubmitCause::Rejected(rejection),
            ),
            true,
        )
    }

    pub(super) fn checked_steering_user_message(
        &mut self,
        message: CheckedSteeringUserMessage,
    ) -> (BrokerReply, bool) {
        let Some(active) = self.take_steering() else {
            return self.reject_checked_steering(
                message,
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl),
            );
        };
        let selected = &active.selection;
        if message.lifecycle() != selected.lifecycle()
            || message.item_id() != selected.item_id()
            || message.client_user_message_id() != selected.client_user_message_id()
            || message.thread_id() != active.proof.route().target().pending().cas_thread_id()
            || message.turn_id() != active.proof.route().target().cas_turn_id()
            || message.checked_input_items() != active.header.item_count()
        {
            return self.fail_active_checked(
                active,
                message,
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let Some((home_generation, storage)) = self.current_observation_authority() else {
            return self.fail_active_checked(
                active,
                message,
                OrderedTurnStreamRejection::StagingConflict,
            );
        };
        let confirmed = match storage.delivering_steering_input(
            &self.home,
            active.proof.route().input().id(),
            point_limit(),
        ) {
            Ok(Some(confirmed))
                if home_generation == active.proof.home_generation()
                    && &confirmed == active.proof.route() =>
            {
                confirmed
            }
            _ => {
                return self.fail_active_checked(
                    active,
                    message,
                    OrderedTurnStreamRejection::StagingConflict,
                );
            }
        };
        if !active.permit.matches_target(
            confirmed.input().thread_id(),
            confirmed.target(),
            confirmed.loaded_generation(),
        ) || active.permit.home_generation(&self.home) != Some(home_generation)
        {
            return self.fail_active_checked(
                active,
                message,
                OrderedTurnStreamRejection::InvalidControl,
            );
        }

        let proof = active.proof;
        let permit = active.permit;
        let mut result = Some(CheckedSteeringLifecycle::new(
            confirmed,
            home_generation,
            message,
        ));
        let results = Arc::clone(&self.steering_results);
        let publish_proof = proof.clone();
        match permit.finish_with(|| {
            results.publish(
                result
                    .take()
                    .expect("router callback publishes the checked lifecycle once"),
                publish_proof,
            );
        }) {
            Ok(()) => (BrokerReply::SteeringChecked, false),
            Err(error) => {
                let message = result
                    .take()
                    .expect("failed router finish leaves checked lifecycle ownership")
                    .into_message();
                if self.exact_persistent_failure() {
                    return (
                        BrokerReply::SteeringCheckedRejected(
                            message,
                            OrderedTurnStreamSubmitCause::Rejected(
                                OrderedTurnStreamRejection::InvalidControl,
                            ),
                        ),
                        true,
                    );
                }
                self.steering_results.abandon_selection();
                if let DelayedSteeringLifecycleFinishError::Target(target) = error {
                    let _ = self.invalidate_target(target);
                }
                self.retire();
                (
                    BrokerReply::SteeringCheckedRejected(
                        message,
                        OrderedTurnStreamSubmitCause::Rejected(
                            OrderedTurnStreamRejection::InvalidControl,
                        ),
                    ),
                    true,
                )
            }
        }
    }

    fn fail_active_checked(
        &mut self,
        active: ActiveSteeringLifecycle,
        message: CheckedSteeringUserMessage,
        rejection: OrderedTurnStreamRejection,
    ) -> (BrokerReply, bool) {
        if self.exact_persistent_failure() {
            drop(active);
            return (
                BrokerReply::SteeringCheckedRejected(
                    message,
                    OrderedTurnStreamSubmitCause::Rejected(rejection),
                ),
                true,
            );
        }
        self.steering_results.abandon_selection();
        match active.permit.fail() {
            Ok(target) | Err(DelayedSteeringLifecycleFinishError::Target(target)) => {
                let _ = self.invalidate_target(target);
            }
            Err(DelayedSteeringLifecycleFinishError::Router) => {}
        }
        self.retire();
        (
            BrokerReply::SteeringCheckedRejected(
                message,
                OrderedTurnStreamSubmitCause::Rejected(rejection),
            ),
            true,
        )
    }

    fn reject_checked_steering(
        &mut self,
        message: CheckedSteeringUserMessage,
        cause: OrderedTurnStreamSubmitCause,
    ) -> (BrokerReply, bool) {
        if self.exact_persistent_failure() {
            return (BrokerReply::SteeringCheckedRejected(message, cause), true);
        }
        self.steering_results.close();
        self.retire();
        (BrokerReply::SteeringCheckedRejected(message, cause), true)
    }

    pub(super) fn abandon_steering_user_message(
        &mut self,
        _reason: SteeringUserMessageAbandonReason,
    ) -> (BrokerReply, bool) {
        if self.exact_persistent_failure() {
            return (BrokerReply::SteeringAbandoned, true);
        }
        let Some(active) = self.take_steering() else {
            self.steering_results.close();
            self.retire();
            return (
                BrokerReply::SteeringAbandonRejected(OrderedTurnStreamSubmitCause::Rejected(
                    OrderedTurnStreamRejection::InvalidControl,
                )),
                true,
            );
        };
        self.steering_results.abandon_selection();
        match active.permit.fail() {
            Ok(target) | Err(DelayedSteeringLifecycleFinishError::Target(target)) => {
                let _ = self.invalidate_target(target);
            }
            Err(DelayedSteeringLifecycleFinishError::Router) => {}
        }
        self.retire();
        (BrokerReply::SteeringAbandoned, true)
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests;
