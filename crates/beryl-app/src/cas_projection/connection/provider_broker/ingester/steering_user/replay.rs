use std::sync::{Arc, atomic::AtomicBool};

use beryl_backend::{
    StreamedInputDescriptor, StreamedInputHeader, StreamedInputSource, StreamedInputSourceError,
    StreamedTextPage, StreamedTextSourceId,
};
use beryl_home_store::HomeStore;

use crate::cas_projection::{ProjectionCancellationToken, input_replay::AcceptedInputReplaySource};

pub(super) struct AcceptedSteeringReplaySource {
    home: Arc<HomeStore>,
    replay_cancellation: ProjectionCancellationToken,
    source: AcceptedInputReplaySource,
}

impl AcceptedSteeringReplaySource {
    pub(super) fn new(
        home: Arc<HomeStore>,
        broker_cancelled: Arc<AtomicBool>,
        source: AcceptedInputReplaySource,
    ) -> Self {
        Self {
            home,
            replay_cancellation: ProjectionCancellationToken::from_shared_flag(broker_cancelled),
            source,
        }
    }
}

impl StreamedInputSource for AcceptedSteeringReplaySource {
    fn header(&self) -> StreamedInputHeader {
        self.source.header()
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        self.source
            .begin_pass(&self.home, &self.replay_cancellation)
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        self.source
            .next_descriptor(&self.home, &self.replay_cancellation)
    }

    fn read_text_page(
        &mut self,
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        self.source.read_text_page(
            &self.home,
            &self.replay_cancellation,
            source_id,
            start,
            max_utf8_bytes,
        )
    }
}
