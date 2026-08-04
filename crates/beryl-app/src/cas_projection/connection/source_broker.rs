use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use beryl_backend::{
    StreamedInputDescriptor, StreamedInputHeader, StreamedInputSource, StreamedInputSourceError,
    StreamedTextPage, StreamedTextSourceId,
};

use super::ConnectionCommandOutcome;
use crate::cas_projection::ProjectionCoordinatorError;

pub(in crate::cas_projection) trait StreamedInputBrokerService {
    fn header(&self) -> StreamedInputHeader;

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError>;

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError>;

    fn read_text_page(
        &mut self,
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError>;

    #[cfg(feature = "test-faults")]
    fn pause_text_page_handoff_for_lifecycle_test(&mut self) {}
}

pub(super) enum SourceBrokerEvent<T, E> {
    BeginPass {
        reply: SyncSender<Result<StreamedInputHeader, StreamedInputSourceError>>,
    },
    NextDescriptor {
        reply: SyncSender<Result<Option<StreamedInputDescriptor>, StreamedInputSourceError>>,
    },
    ReadTextPage {
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
        reply: SyncSender<Result<StreamedTextPage, StreamedInputSourceError>>,
    },
    Finished(Result<ConnectionCommandOutcome<T>, E>),
}

/// Sole nonclone descriptor-source endpoint moved into one queued driver command.
pub(super) struct RemoteStreamedInputSource<T, E> {
    header: StreamedInputHeader,
    events: SyncSender<SourceBrokerEvent<T, E>>,
}

impl<T, E> StreamedInputSource for RemoteStreamedInputSource<T, E>
where
    T: Send + 'static,
    E: Send + 'static,
{
    fn header(&self) -> StreamedInputHeader {
        self.header
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        request(&self.events, |reply| SourceBrokerEvent::BeginPass { reply })
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        request(&self.events, |reply| SourceBrokerEvent::NextDescriptor {
            reply,
        })
    }

    fn read_text_page(
        &mut self,
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        request(&self.events, |reply| SourceBrokerEvent::ReadTextPage {
            source_id,
            start,
            max_utf8_bytes,
            reply,
        })
    }
}

fn request<T, E, R>(
    events: &SyncSender<SourceBrokerEvent<T, E>>,
    event: impl FnOnce(SyncSender<Result<R, StreamedInputSourceError>>) -> SourceBrokerEvent<T, E>,
) -> Result<R, StreamedInputSourceError> {
    let (reply, response) = sync_channel(1);
    events
        .send(event(reply))
        .map_err(|_| StreamedInputSourceError::BrokerLost)?;
    response
        .recv()
        .map_err(|_| StreamedInputSourceError::BrokerLost)?
}

#[allow(
    clippy::type_complexity,
    reason = "the source, completion sender, and receiver form one request-scoped broker authority"
)]
pub(super) fn channel<T, E>(
    header: StreamedInputHeader,
) -> (
    RemoteStreamedInputSource<T, E>,
    SyncSender<SourceBrokerEvent<T, E>>,
    Receiver<SourceBrokerEvent<T, E>>,
) {
    let (events, receiver) = sync_channel(1);
    (
        RemoteStreamedInputSource {
            header,
            events: events.clone(),
        },
        events,
        receiver,
    )
}

pub(super) fn service_until_finished<T, E>(
    receiver: Receiver<SourceBrokerEvent<T, E>>,
    mut service: impl StreamedInputBrokerService,
) -> Result<Result<ConnectionCommandOutcome<T>, E>, ProjectionCoordinatorError> {
    loop {
        match receiver.recv() {
            Ok(SourceBrokerEvent::BeginPass { reply }) => {
                send_reply(reply, service.begin_pass())?;
            }
            Ok(SourceBrokerEvent::NextDescriptor { reply }) => {
                send_reply(reply, service.next_descriptor())?;
            }
            Ok(SourceBrokerEvent::ReadTextPage {
                source_id,
                start,
                max_utf8_bytes,
                reply,
            }) => {
                let result = service.read_text_page(source_id, start, max_utf8_bytes);
                #[cfg(feature = "test-faults")]
                if result.is_ok() {
                    service.pause_text_page_handoff_for_lifecycle_test();
                }
                send_reply(reply, result)?;
            }
            Ok(SourceBrokerEvent::Finished(result)) => return Ok(result),
            Err(_) => return Err(ProjectionCoordinatorError::ProjectionWorkerStopped),
        }
    }
}

fn send_reply<T>(
    reply: SyncSender<Result<T, StreamedInputSourceError>>,
    result: Result<T, StreamedInputSourceError>,
) -> Result<(), ProjectionCoordinatorError> {
    reply
        .send(result)
        .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/source_broker.rs"
    ));
}
