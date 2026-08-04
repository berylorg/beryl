use beryl_model::{CasItemId, CasThreadId, CasTurnId};

use crate::{ImageDetail, ItemLifecycleTimestampMs};

use super::{
    StreamedInputDescriptorKind, StreamedInputHeader, StreamedInputPass, StreamedInputSource,
};

mod slot;
mod state;
mod types;

pub(crate) use slot::{
    StreamedUserMessageVerifierGuard, StreamedUserMessageVerifierHandle,
    StreamedUserMessageVerifierSlot,
};
pub(crate) use state::StreamedUserMessageVerifier;
pub use types::{
    CheckedUserMessage, StreamedUserMessageCorrelation, StreamedUserMessageCorrelationError,
    UserMessageEchoLifecycle,
};

use state::{ActiveInput, EchoReplay, VerifierState};

impl StreamedUserMessageVerifier {
    fn new(
        request_scope: u64,
        target_thread_id: CasThreadId,
        source: Box<dyn StreamedInputSource>,
    ) -> Self {
        let header = source.header();
        Self {
            request_scope,
            target_thread_id,
            header,
            source,
            state: VerifierState::Armed,
            echo: None,
            pending_lifecycle: None,
        }
    }

    pub(crate) fn for_steering_lifecycle(
        lifecycle: UserMessageEchoLifecycle,
        target_thread_id: CasThreadId,
        target_turn_id: CasTurnId,
        item_id: CasItemId,
        source: Box<dyn StreamedInputSource>,
    ) -> Result<Self, StreamedUserMessageCorrelationError> {
        let mut verifier = Self::new(0, target_thread_id, source);
        if lifecycle == UserMessageEchoLifecycle::Completed {
            verifier.state = VerifierState::Started {
                item_id,
                turn_id: target_turn_id,
            };
        }
        verifier.begin_lifecycle(lifecycle)?;
        Ok(verifier)
    }

    pub(crate) const fn request_scope(&self) -> u64 {
        self.request_scope
    }

    pub(crate) fn source_and_header(
        &mut self,
    ) -> (StreamedInputHeader, &mut dyn StreamedInputSource) {
        (self.header, &mut *self.source)
    }

    pub(crate) const fn expected_item_count(&self) -> u64 {
        self.header.item_count()
    }

    pub(crate) fn begin_lifecycle(
        &mut self,
        lifecycle: UserMessageEchoLifecycle,
    ) -> Result<(), StreamedUserMessageCorrelationError> {
        self.prepare_lifecycle(lifecycle)?;
        if self.echo.is_some() || self.pending_lifecycle.is_some() {
            return Err(StreamedUserMessageCorrelationError::LifecycleOrdering {
                actual: lifecycle,
                state: "an unfinished lifecycle echo",
            });
        }
        let pass = StreamedInputPass::begin(self.header, &mut *self.source).map_err(|source| {
            StreamedUserMessageCorrelationError::DescriptorSource { lifecycle, source }
        })?;
        self.echo = Some(EchoReplay {
            lifecycle,
            pass,
            active: None,
        });
        Ok(())
    }

    pub(crate) fn begin_input(
        &mut self,
        item_index: u64,
    ) -> Result<&'static str, StreamedUserMessageCorrelationError> {
        let echo = self
            .echo
            .as_mut()
            .ok_or(StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        if echo.active.is_some() {
            return Err(
                StreamedUserMessageCorrelationError::UnsupportedNormalization {
                    context: "one active submitted-input descriptor",
                },
            );
        }
        if echo.pass.observed_count() != item_index {
            return Err(StreamedUserMessageCorrelationError::InputCountMismatch {
                expected: echo.pass.observed_count(),
                actual: item_index,
            });
        }
        let kind = echo
            .pass
            .next_descriptor(&mut *self.source)
            .map_err(
                |source| StreamedUserMessageCorrelationError::DescriptorSource {
                    lifecycle: echo.lifecycle,
                    source,
                },
            )?
            .ok_or(StreamedUserMessageCorrelationError::InputCountMismatch {
                expected: self.header.item_count(),
                actual: item_index.saturating_add(1),
            })?;
        let (wire_type, active) = match kind {
            StreamedInputDescriptorKind::Text(descriptor) => (
                "text",
                ActiveInput::Text {
                    item_index,
                    descriptor,
                    offset: 0,
                    page: None,
                    page_index: 0,
                    finished: false,
                },
            ),
            StreamedInputDescriptorKind::LocalImage(descriptor) => (
                "localImage",
                ActiveInput::LocalImage {
                    item_index,
                    descriptor,
                    path_offset: 0,
                },
            ),
        };
        echo.active = Some(active);
        Ok(wire_type)
    }

    pub(crate) fn expected_image_detail(
        &self,
        item_index: u64,
    ) -> Result<Option<ImageDetail>, StreamedUserMessageCorrelationError> {
        match self.active_input(item_index)? {
            ActiveInput::LocalImage { descriptor, .. } => Ok(descriptor.detail()),
            ActiveInput::Text { .. } => Err(
                StreamedUserMessageCorrelationError::UnsupportedNormalization {
                    context: "a current local-image descriptor",
                },
            ),
        }
    }

    pub(crate) fn compare_text_bytes(
        &mut self,
        item_index: u64,
        actual: &[u8],
    ) -> Result<(), StreamedUserMessageCorrelationError> {
        let echo = self
            .echo
            .as_mut()
            .ok_or(StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        let ActiveInput::Text {
            item_index: active_index,
            descriptor,
            offset,
            page,
            page_index,
            finished,
        } = echo
            .active
            .as_mut()
            .ok_or(StreamedUserMessageCorrelationError::VerifierUnavailable)?
        else {
            return Err(
                StreamedUserMessageCorrelationError::UnsupportedNormalization {
                    context: "a current text descriptor",
                },
            );
        };
        if *active_index != item_index || *finished {
            return Err(StreamedUserMessageCorrelationError::VerifierScopeDisagreement);
        }

        for actual_byte in actual {
            if *offset == descriptor.utf8_len() {
                return Err(StreamedUserMessageCorrelationError::TextLengthMismatch {
                    item_index,
                    expected: descriptor.utf8_len(),
                    actual: offset.saturating_add(1),
                });
            }
            if page
                .as_ref()
                .is_none_or(|current| *page_index == current.text().len())
            {
                drop(page.take());
                let next_page = echo
                    .pass
                    .read_text_page(&mut *self.source, item_index + 1, descriptor, *offset)
                    .map_err(|source| StreamedUserMessageCorrelationError::TextSource {
                        item_index,
                        source,
                    })?;
                *page = Some(Box::new(next_page));
                *page_index = 0;
            }
            let expected = page
                .as_ref()
                .expect("nonterminal text owns a page")
                .text()
                .as_bytes()[*page_index];
            if *actual_byte != expected {
                return Err(StreamedUserMessageCorrelationError::TextMismatch {
                    item_index,
                    byte_offset: *offset,
                });
            }
            *page_index += 1;
            *offset += 1;
        }
        Ok(())
    }

    pub(crate) fn finish_text(
        &mut self,
        item_index: u64,
    ) -> Result<(), StreamedUserMessageCorrelationError> {
        let ActiveInput::Text {
            item_index: active_index,
            descriptor,
            offset,
            page,
            finished,
            ..
        } = self.active_input_mut(item_index)?
        else {
            return Err(
                StreamedUserMessageCorrelationError::UnsupportedNormalization {
                    context: "a current text descriptor",
                },
            );
        };
        if *active_index != item_index {
            return Err(StreamedUserMessageCorrelationError::VerifierScopeDisagreement);
        }
        if *offset != descriptor.utf8_len() {
            return Err(StreamedUserMessageCorrelationError::TextLengthMismatch {
                item_index,
                expected: descriptor.utf8_len(),
                actual: *offset,
            });
        }
        drop(page.take());
        *finished = true;
        Ok(())
    }

    pub(crate) fn compare_image_path_bytes(
        &mut self,
        item_index: u64,
        actual: &[u8],
    ) -> Result<(), StreamedUserMessageCorrelationError> {
        let ActiveInput::LocalImage {
            item_index: active_index,
            descriptor,
            path_offset,
        } = self.active_input_mut(item_index)?
        else {
            return Err(
                StreamedUserMessageCorrelationError::UnsupportedNormalization {
                    context: "a current local-image descriptor",
                },
            );
        };
        if *active_index != item_index {
            return Err(StreamedUserMessageCorrelationError::VerifierScopeDisagreement);
        }
        let expected = descriptor.path().as_bytes();
        let end = path_offset
            .checked_add(actual.len())
            .ok_or(StreamedUserMessageCorrelationError::ImagePathLengthMismatch { item_index })?;
        if end > expected.len() {
            return Err(
                StreamedUserMessageCorrelationError::ImagePathLengthMismatch { item_index },
            );
        }
        if actual != &expected[*path_offset..end] {
            let mismatch = actual
                .iter()
                .zip(&expected[*path_offset..end])
                .position(|(actual, expected)| actual != expected)
                .unwrap_or(0);
            return Err(StreamedUserMessageCorrelationError::ImagePathMismatch {
                item_index,
                byte_offset: (*path_offset + mismatch) as u64,
            });
        }
        *path_offset = end;
        Ok(())
    }

    pub(crate) fn finish_image_path(
        &mut self,
        item_index: u64,
    ) -> Result<(), StreamedUserMessageCorrelationError> {
        let ActiveInput::LocalImage {
            descriptor,
            path_offset,
            ..
        } = self.active_input_mut(item_index)?
        else {
            return Err(
                StreamedUserMessageCorrelationError::UnsupportedNormalization {
                    context: "a current local-image descriptor",
                },
            );
        };
        if *path_offset != descriptor.path().len() {
            return Err(
                StreamedUserMessageCorrelationError::ImagePathLengthMismatch { item_index },
            );
        }
        Ok(())
    }

    pub(crate) fn finish_input(
        &mut self,
        item_index: u64,
    ) -> Result<(), StreamedUserMessageCorrelationError> {
        let echo = self
            .echo
            .as_mut()
            .ok_or(StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        let active = echo
            .active
            .take()
            .ok_or(StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        match active {
            ActiveInput::Text {
                item_index: active_index,
                finished: true,
                ..
            }
            | ActiveInput::LocalImage {
                item_index: active_index,
                ..
            } if active_index == item_index => Ok(()),
            _ => Err(StreamedUserMessageCorrelationError::VerifierScopeDisagreement),
        }
    }

    pub(crate) fn finish_lifecycle_content(
        &mut self,
        actual_count: u64,
    ) -> Result<(), StreamedUserMessageCorrelationError> {
        let mut echo = self
            .echo
            .take()
            .ok_or(StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        if echo.active.is_some() {
            return Err(StreamedUserMessageCorrelationError::VerifierScopeDisagreement);
        }
        if actual_count != self.header.item_count() {
            return Err(StreamedUserMessageCorrelationError::InputCountMismatch {
                expected: self.header.item_count(),
                actual: actual_count,
            });
        }
        echo.pass.finish(&mut *self.source).map_err(|source| {
            StreamedUserMessageCorrelationError::DescriptorSource {
                lifecycle: echo.lifecycle,
                source,
            }
        })?;
        self.pending_lifecycle = Some(echo.lifecycle);
        Ok(())
    }

    pub(crate) fn prepare_lifecycle(
        &self,
        lifecycle: UserMessageEchoLifecycle,
    ) -> Result<(), StreamedUserMessageCorrelationError> {
        match (&self.state, lifecycle) {
            (VerifierState::Armed, UserMessageEchoLifecycle::Started)
            | (VerifierState::Started { .. }, UserMessageEchoLifecycle::Completed) => Ok(()),
            (VerifierState::Armed, UserMessageEchoLifecycle::Completed) => {
                Err(StreamedUserMessageCorrelationError::LifecycleOrdering {
                    actual: lifecycle,
                    state: "no started echo",
                })
            }
            (VerifierState::Started { .. }, UserMessageEchoLifecycle::Started) => {
                Err(StreamedUserMessageCorrelationError::LifecycleOrdering {
                    actual: lifecycle,
                    state: "one started echo",
                })
            }
            (VerifierState::Completed { .. }, _) => {
                Err(StreamedUserMessageCorrelationError::LifecycleOrdering {
                    actual: lifecycle,
                    state: "completed echo",
                })
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_lifecycle(
        &mut self,
        lifecycle: UserMessageEchoLifecycle,
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        item_id: CasItemId,
        timestamp: ItemLifecycleTimestampMs,
    ) -> Result<CheckedUserMessage, StreamedUserMessageCorrelationError> {
        if thread_id != self.target_thread_id {
            return Err(StreamedUserMessageCorrelationError::ThreadMismatch);
        }
        self.prepare_lifecycle(lifecycle)?;
        if self.pending_lifecycle != Some(lifecycle) {
            return Err(StreamedUserMessageCorrelationError::VerifierScopeDisagreement);
        }
        self.pending_lifecycle = None;
        match lifecycle {
            UserMessageEchoLifecycle::Started => {
                self.state = VerifierState::Started {
                    item_id: item_id.clone(),
                    turn_id: turn_id.clone(),
                };
            }
            UserMessageEchoLifecycle::Completed => {
                let VerifierState::Started {
                    item_id: started_item,
                    turn_id: started_turn,
                } = &self.state
                else {
                    unreachable!("prepare_lifecycle admitted only a started verifier")
                };
                if started_turn != &turn_id {
                    return Err(StreamedUserMessageCorrelationError::TurnMismatch);
                }
                if started_item != &item_id {
                    return Err(StreamedUserMessageCorrelationError::ItemMismatch);
                }
                self.state = VerifierState::Completed {
                    turn_id: turn_id.clone(),
                };
            }
        }
        Ok(CheckedUserMessage {
            lifecycle,
            thread_id,
            turn_id,
            timestamp,
            correlation: StreamedUserMessageCorrelation {
                item_id,
                checked_input_items: self.header.item_count(),
            },
        })
    }

    pub(crate) fn verify_successful_response(
        &self,
        response_turn_id: &CasTurnId,
    ) -> Result<(), StreamedUserMessageCorrelationError> {
        let VerifierState::Completed { turn_id } = &self.state else {
            return Err(StreamedUserMessageCorrelationError::SuccessfulResponseBeforeBothEchoes);
        };
        if turn_id != response_turn_id {
            return Err(StreamedUserMessageCorrelationError::ResponseTurnMismatch);
        }
        Ok(())
    }

    pub(crate) fn verify_rejection(&self) -> Result<(), StreamedUserMessageCorrelationError> {
        if matches!(self.state, VerifierState::Armed)
            && self.echo.is_none()
            && self.pending_lifecycle.is_none()
        {
            Ok(())
        } else {
            Err(StreamedUserMessageCorrelationError::RejectionAfterEcho)
        }
    }

    fn active_input(
        &self,
        item_index: u64,
    ) -> Result<&ActiveInput, StreamedUserMessageCorrelationError> {
        let active = self
            .echo
            .as_ref()
            .and_then(|echo| echo.active.as_ref())
            .ok_or(StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        if active.item_index() != item_index {
            return Err(StreamedUserMessageCorrelationError::VerifierScopeDisagreement);
        }
        Ok(active)
    }

    fn active_input_mut(
        &mut self,
        item_index: u64,
    ) -> Result<&mut ActiveInput, StreamedUserMessageCorrelationError> {
        let active = self
            .echo
            .as_mut()
            .and_then(|echo| echo.active.as_mut())
            .ok_or(StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        if active.item_index() != item_index {
            return Err(StreamedUserMessageCorrelationError::VerifierScopeDisagreement);
        }
        Ok(active)
    }
}
