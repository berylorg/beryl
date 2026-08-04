use beryl_model::{CasItemId, CasThreadId, CasTurnId};

use crate::{
    CheckedSteeringUserMessage, ClientUserMessageId, ImageDetail, ItemLifecycleTimestampMs,
    OrderedTurnStreamSink, SteeringUserMessageAbandonReason, SteeringUserMessageError,
    SteeringUserMessageSelection, UserMessageEchoLifecycle, turn::StreamedUserMessageVerifier,
};

pub(super) struct SteeringUserMessageCapture<'a> {
    sink: &'a mut dyn OrderedTurnStreamSink,
    verifier: StreamedUserMessageVerifier,
    lifecycle: UserMessageEchoLifecycle,
    item_id: CasItemId,
    client_user_message_id: ClientUserMessageId,
    expected_turn_id: CasTurnId,
    active: bool,
    abandon: SteeringUserMessageAbandonReason,
}

impl<'a> SteeringUserMessageCapture<'a> {
    pub(super) fn begin(
        sink: &'a mut dyn OrderedTurnStreamSink,
        lifecycle: UserMessageEchoLifecycle,
        item_id: CasItemId,
        client_user_message_id: ClientUserMessageId,
    ) -> Result<Self, SteeringUserMessageError> {
        let selection = SteeringUserMessageSelection::new(
            lifecycle,
            item_id.clone(),
            client_user_message_id.clone(),
        );
        let selected = sink
            .select_steering_user_message(selection)
            .map_err(|error| SteeringUserMessageError::Selection(error.cause()))?;
        let (expected_thread_id, expected_turn_id, source) = selected.into_parts();
        let verifier = match StreamedUserMessageVerifier::for_steering_lifecycle(
            lifecycle,
            expected_thread_id,
            expected_turn_id.clone(),
            item_id.clone(),
            source,
        ) {
            Ok(verifier) => verifier,
            Err(source) => {
                let _ = sink.abandon_steering_user_message(
                    SteeringUserMessageAbandonReason::CorrelationFailure,
                );
                return Err(source.into());
            }
        };
        Ok(Self {
            sink,
            verifier,
            lifecycle,
            item_id,
            client_user_message_id,
            expected_turn_id,
            active: true,
            abandon: SteeringUserMessageAbandonReason::SchemaFailure,
        })
    }

    pub(super) const fn expected_item_count(&self) -> u64 {
        self.verifier.expected_item_count()
    }

    pub(super) fn begin_input(
        &mut self,
        item_index: u64,
    ) -> Result<&'static str, SteeringUserMessageError> {
        let result = self.verifier.begin_input(item_index);
        if result.is_err() {
            self.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
        }
        result.map_err(Into::into)
    }

    pub(super) fn expected_image_detail(
        &mut self,
        item_index: u64,
    ) -> Result<Option<ImageDetail>, SteeringUserMessageError> {
        let result = self.verifier.expected_image_detail(item_index);
        if result.is_err() {
            self.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
        }
        result.map_err(Into::into)
    }

    pub(super) fn compare_text_bytes(
        &mut self,
        item_index: u64,
        bytes: &[u8],
    ) -> Result<(), SteeringUserMessageError> {
        let result = self.verifier.compare_text_bytes(item_index, bytes);
        if result.is_err() {
            self.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
        }
        result.map_err(Into::into)
    }

    pub(super) fn finish_text(&mut self, item_index: u64) -> Result<(), SteeringUserMessageError> {
        let result = self.verifier.finish_text(item_index);
        if result.is_err() {
            self.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
        }
        result.map_err(Into::into)
    }

    pub(super) fn compare_image_path_bytes(
        &mut self,
        item_index: u64,
        bytes: &[u8],
    ) -> Result<(), SteeringUserMessageError> {
        let result = self.verifier.compare_image_path_bytes(item_index, bytes);
        if result.is_err() {
            self.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
        }
        result.map_err(Into::into)
    }

    pub(super) fn finish_image_path(
        &mut self,
        item_index: u64,
    ) -> Result<(), SteeringUserMessageError> {
        let result = self.verifier.finish_image_path(item_index);
        if result.is_err() {
            self.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
        }
        result.map_err(Into::into)
    }

    pub(super) fn finish_input(&mut self, item_index: u64) -> Result<(), SteeringUserMessageError> {
        let result = self.verifier.finish_input(item_index);
        if result.is_err() {
            self.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
        }
        result.map_err(Into::into)
    }

    pub(super) fn finish_content(
        &mut self,
        item_count: u64,
    ) -> Result<(), SteeringUserMessageError> {
        let result = self.verifier.finish_lifecycle_content(item_count);
        if result.is_err() {
            self.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
        }
        result.map_err(Into::into)
    }

    pub(super) fn seal(
        mut self,
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        timestamp: ItemLifecycleTimestampMs,
    ) -> Result<(), SteeringUserMessageError> {
        if turn_id != self.expected_turn_id {
            self.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
            return Err(SteeringUserMessageError::TurnMismatch);
        }
        let checked = self.verifier.commit_lifecycle(
            self.lifecycle,
            thread_id,
            turn_id,
            self.item_id.clone(),
            timestamp,
        );
        if checked.is_err() {
            self.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
        }
        let checked = checked?;
        let (lifecycle, thread_id, turn_id, timestamp, correlation) = checked.into_parts();
        let message = CheckedSteeringUserMessage::new(
            lifecycle,
            thread_id,
            turn_id,
            correlation.item_id().clone(),
            timestamp,
            self.client_user_message_id.clone(),
            correlation.checked_input_items(),
        );
        match self.sink.submit_checked_steering_user_message(message) {
            Ok(()) => {
                self.active = false;
                Ok(())
            }
            Err(error) => {
                self.abandon = crate::turn::steering_abandon_reason(error.cause());
                Err(SteeringUserMessageError::Commit(error.cause()))
            }
        }
    }

    pub(super) fn mark_route_failure(&mut self) {
        self.abandon = SteeringUserMessageAbandonReason::MissingOrMalformedRoute;
    }

    pub(super) fn mark_transport_lost(&mut self) {
        self.abandon = SteeringUserMessageAbandonReason::TransportLost;
    }
}

impl Drop for SteeringUserMessageCapture<'_> {
    fn drop(&mut self) {
        if self.active {
            let _ = self.sink.abandon_steering_user_message(self.abandon);
        }
    }
}
