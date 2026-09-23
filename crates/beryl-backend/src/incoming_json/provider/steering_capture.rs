use crate::{
    CheckedSteeringUserMessage, ClientUserMessageId, ImageDetail, ItemLifecycleTimestampMs,
    OrderedTurnStreamSink, SteeringUserMessageAbandonReason, SteeringUserMessageCaptureMode,
    SteeringUserMessageError, SteeringUserMessageSelection, StreamedUserMessageCorrelationError,
    UnverifiedSteeringUserMessage, UserMessageEchoLifecycle, turn::StreamedUserMessageVerifier,
};
use beryl_model::{CasItemId, CasThreadId, CasTurnId};

pub(super) struct SteeringUserMessageCapture<'a> {
    sink: &'a mut dyn OrderedTurnStreamSink,
    verifier: Option<StreamedUserMessageVerifier>,
    lifecycle: UserMessageEchoLifecycle,
    item_id: CasItemId,
    client_user_message_id: ClientUserMessageId,
    expected_turn_id: Option<CasTurnId>,
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
        let mut capture = Self {
            sink,
            verifier: None,
            lifecycle,
            item_id,
            client_user_message_id,
            expected_turn_id: None,
            active: true,
            abandon: SteeringUserMessageAbandonReason::SchemaFailure,
        };
        if capture.mode()? == SteeringUserMessageCaptureMode::Passive {
            return Ok(capture);
        }
        let selection = SteeringUserMessageSelection::new(
            lifecycle,
            capture.item_id.clone(),
            capture.client_user_message_id.clone(),
        );
        let selected = match capture.sink.select_steering_user_message(selection) {
            Ok(source) => source,
            Err(error) => {
                if capture.mode()? == SteeringUserMessageCaptureMode::Passive {
                    return Ok(capture);
                }
                capture.active = false;
                return Err(SteeringUserMessageError::Selection(error.cause()));
            }
        };
        let (thread, turn, source) = selected.into_parts();
        if capture.mode()? == SteeringUserMessageCaptureMode::Passive {
            return Ok(capture);
        }
        let verifier = StreamedUserMessageVerifier::for_steering_lifecycle(
            lifecycle,
            thread,
            turn.clone(),
            capture.item_id.clone(),
            source,
        );
        match verifier {
            Ok(verifier) => {
                capture.verifier = Some(verifier);
                capture.expected_turn_id = Some(turn);
            }
            Err(error) => {
                if capture.mode()? != SteeringUserMessageCaptureMode::Passive {
                    capture.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
                    return Err(error.into());
                }
            }
        }
        Ok(capture)
    }

    fn mode(&mut self) -> Result<SteeringUserMessageCaptureMode, SteeringUserMessageError> {
        self.sink
            .steering_user_message_capture_mode()
            .map_err(|cause| {
                self.abandon = crate::turn::steering_abandon_reason(cause);
                SteeringUserMessageError::Selection(cause)
            })
    }

    fn verify<T>(
        &mut self,
        passive: T,
        operation: impl FnOnce(
            &mut StreamedUserMessageVerifier,
        ) -> Result<T, StreamedUserMessageCorrelationError>,
    ) -> Result<T, SteeringUserMessageError> {
        let mode = self.mode()?;
        if self.verifier.is_none() {
            return Ok(passive);
        }
        if mode == SteeringUserMessageCaptureMode::Passive {
            self.verifier = None;
            self.expected_turn_id = None;
            return Ok(passive);
        }
        let result = operation(self.verifier.as_mut().expect("verification remains active"));
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                if self.mode()? == SteeringUserMessageCaptureMode::Passive {
                    self.verifier = None;
                    self.expected_turn_id = None;
                    Ok(passive)
                } else {
                    self.abandon = SteeringUserMessageAbandonReason::CorrelationFailure;
                    Err(error.into())
                }
            }
        }
    }

    pub(super) fn check_item_count(
        &mut self,
        actual: u64,
        complete: bool,
    ) -> Result<(), SteeringUserMessageError> {
        self.verify((), |verifier| {
            let expected = verifier.expected_item_count();
            if actual > expected || (complete && actual != expected) {
                return Err(StreamedUserMessageCorrelationError::InputCountMismatch {
                    expected,
                    actual,
                });
            }
            Ok(())
        })
    }

    pub(super) fn begin_input(
        &mut self,
        index: u64,
        actual: &'static str,
    ) -> Result<(), SteeringUserMessageError> {
        self.verify((), |verifier| {
            let expected = verifier.begin_input(index)?;
            if actual != expected {
                return Err(StreamedUserMessageCorrelationError::InputVariantMismatch {
                    item_index: index,
                    expected,
                    actual,
                });
            }
            Ok(())
        })
    }

    pub(super) fn check_image_detail(
        &mut self,
        index: u64,
        actual: Option<ImageDetail>,
    ) -> Result<(), SteeringUserMessageError> {
        self.verify((), |verifier| {
            if verifier.expected_image_detail(index)? != actual {
                return Err(StreamedUserMessageCorrelationError::ImageDetailMismatch {
                    item_index: index,
                });
            }
            Ok(())
        })
    }

    pub(super) fn compare_text_bytes(
        &mut self,
        index: u64,
        bytes: &[u8],
    ) -> Result<(), SteeringUserMessageError> {
        self.verify((), |verifier| verifier.compare_text_bytes(index, bytes))
    }

    pub(super) fn finish_text(&mut self, index: u64) -> Result<(), SteeringUserMessageError> {
        self.verify((), |verifier| verifier.finish_text(index))
    }

    pub(super) fn compare_image_path_bytes(
        &mut self,
        index: u64,
        bytes: &[u8],
    ) -> Result<(), SteeringUserMessageError> {
        self.verify((), |verifier| {
            verifier.compare_image_path_bytes(index, bytes)
        })
    }

    pub(super) fn finish_image_path(&mut self, index: u64) -> Result<(), SteeringUserMessageError> {
        self.verify((), |verifier| verifier.finish_image_path(index))
    }

    pub(super) fn finish_input(&mut self, index: u64) -> Result<(), SteeringUserMessageError> {
        self.verify((), |verifier| verifier.finish_input(index))
    }

    pub(super) fn finish_content(&mut self, count: u64) -> Result<(), SteeringUserMessageError> {
        self.verify((), |verifier| verifier.finish_lifecycle_content(count))
    }

    pub(super) fn seal(
        mut self,
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        timestamp: ItemLifecycleTimestampMs,
    ) -> Result<(), SteeringUserMessageError> {
        let lifecycle = self.lifecycle;
        let item = self.item_id.clone();
        let expected = self.expected_turn_id.clone();
        let checked = self
            .verify(None, |verifier| {
                if expected.as_ref() != Some(&turn_id) {
                    return Err(StreamedUserMessageCorrelationError::TurnMismatch);
                }
                verifier
                    .commit_lifecycle(
                        lifecycle,
                        thread_id.clone(),
                        turn_id.clone(),
                        item,
                        timestamp,
                    )
                    .map(Some)
            })
            .map_err(|error| match error {
                SteeringUserMessageError::Correlation {
                    source: StreamedUserMessageCorrelationError::TurnMismatch,
                } => SteeringUserMessageError::TurnMismatch,
                other => other,
            })?;
        let result = if let Some(checked) = checked {
            let (lifecycle, thread_id, turn_id, timestamp, correlation) = checked.into_parts();
            self.sink
                .submit_checked_steering_user_message(CheckedSteeringUserMessage::new(
                    lifecycle,
                    thread_id,
                    turn_id,
                    correlation.item_id().clone(),
                    timestamp,
                    self.client_user_message_id.clone(),
                    correlation.checked_input_items(),
                ))
                .map_err(|error| error.cause())
        } else {
            self.sink
                .submit_unverified_steering_user_message(UnverifiedSteeringUserMessage {
                    lifecycle,
                    thread_id,
                    turn_id,
                    item_id: self.item_id.clone(),
                    timestamp,
                    client_user_message_id: self.client_user_message_id.clone(),
                })
        };
        match result {
            Ok(()) => {
                self.active = false;
                Ok(())
            }
            Err(cause) => {
                self.abandon = crate::turn::steering_abandon_reason(cause);
                Err(SteeringUserMessageError::Commit(cause))
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
