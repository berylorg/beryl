use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use super::*;
use beryl_backend::{
    STREAMED_TEXT_MAX_PAGE_BYTES,
    StreamedInputDescriptorKind, StreamedInputSequenceDigestAccumulator,
    StreamedInputSourceIdentity, StreamedInputSourceRevision, StreamedTextDescriptor,
    TextSourceProof,
};

const TEXT: &str = "abc";

fn source_identity() -> StreamedInputSourceIdentity {
    StreamedInputSourceIdentity::new([1; 32])
}

fn source_revision() -> StreamedInputSourceRevision {
    StreamedInputSourceRevision::new(7)
}

fn source_id() -> StreamedTextSourceId {
    StreamedTextSourceId::new([2; 32])
}

fn proof() -> TextSourceProof {
    TextSourceProof::new([3; 32])
}

fn header() -> StreamedInputHeader {
    let mut digest = StreamedInputSequenceDigestAccumulator::new(1);
    digest.push_text(1, proof(), TEXT.len() as u64).unwrap();
    StreamedInputHeader::new(
        source_identity(),
        source_revision(),
        1,
        digest.finish().unwrap(),
    )
}

struct TestService {
    pass_count: Arc<AtomicUsize>,
    page_count: Arc<AtomicUsize>,
    release_count: Arc<AtomicUsize>,
    pass_open: bool,
    descriptor_emitted: bool,
}

impl TestService {
    fn new(
        pass_count: Arc<AtomicUsize>,
        page_count: Arc<AtomicUsize>,
        release_count: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            pass_count,
            page_count,
            release_count,
            pass_open: false,
            descriptor_emitted: false,
        }
    }
}

impl Drop for TestService {
    fn drop(&mut self) {
        self.release_count.fetch_add(1, Ordering::AcqRel);
    }
}

impl StreamedInputBrokerService for TestService {
    fn header(&self) -> StreamedInputHeader {
        header()
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        self.pass_count.fetch_add(1, Ordering::AcqRel);
        self.pass_open = true;
        self.descriptor_emitted = false;
        Ok(header())
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        if !self.pass_open {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        if self.descriptor_emitted {
            self.pass_open = false;
            return Ok(None);
        }
        self.descriptor_emitted = true;
        Ok(Some(StreamedInputDescriptor::new(
            source_identity(),
            source_revision(),
            1,
            StreamedInputDescriptorKind::Text(StreamedTextDescriptor::new(
                source_id(),
                proof(),
                TEXT.len() as u64,
            )),
        )))
    }

    fn read_text_page(
        &mut self,
        requested_source: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        assert_eq!(requested_source, source_id());
        assert_eq!(start, 0);
        assert!(max_utf8_bytes >= TEXT.len());
        self.page_count.fetch_add(1, Ordering::AcqRel);
        Ok(StreamedTextPage::new(
            source_identity(),
            source_revision(),
            source_id(),
            proof(),
            0,
            TEXT,
            None,
        ))
    }
}

#[test]
fn one_nonclone_source_replays_all_three_passes_through_capacity_one_broker() {
    let pass_count = Arc::new(AtomicUsize::new(0));
    let page_count = Arc::new(AtomicUsize::new(0));
    let release_count = Arc::new(AtomicUsize::new(0));
    let service = TestService::new(
        Arc::clone(&pass_count),
        Arc::clone(&page_count),
        Arc::clone(&release_count),
    );
    let (mut source, events, receiver) = channel::<(), u8>(header());
    let worker = std::thread::spawn(move || {
        for _ in 0..3 {
            assert_eq!(source.begin_pass().unwrap(), header());
            let descriptor = source.next_descriptor().unwrap().unwrap();
            assert_eq!(descriptor.item_ordinal(), 1);
            let StreamedInputDescriptorKind::Text(text) = descriptor.kind() else {
                panic!("test source must emit text")
            };
            let page = source
                .read_text_page(text.source_id(), 0, STREAMED_TEXT_MAX_PAGE_BYTES)
                .unwrap();
            assert_eq!(page.text(), TEXT);
            assert!(source.next_descriptor().unwrap().is_none());
        }
        events.send(SourceBrokerEvent::Finished(Err(9))).unwrap();
    });

    let result = service_until_finished(receiver, service).unwrap();
    assert!(matches!(result, Err(9)));
    worker.join().unwrap();
    assert_eq!(pass_count.load(Ordering::Acquire), 3);
    assert_eq!(page_count.load(Ordering::Acquire), 3);
    assert_eq!(release_count.load(Ordering::Acquire), 1);
}

#[test]
fn dropping_either_broker_end_wakes_the_other_side() {
    let (mut source, _events, receiver) = channel::<(), ()>(header());
    drop(receiver);
    assert_eq!(
        source.begin_pass(),
        Err(StreamedInputSourceError::BrokerLost)
    );

    let (mut source, _events, receiver) = channel::<(), ()>(header());
    let worker = std::thread::spawn(move || source.begin_pass());
    let event = receiver.recv().unwrap();
    let SourceBrokerEvent::BeginPass { reply } = event else {
        panic!("source emitted an unexpected event")
    };
    drop(reply);
    assert_eq!(
        worker.join().unwrap(),
        Err(StreamedInputSourceError::BrokerLost)
    );
}

#[test]
fn abandoned_reply_is_reported_instead_of_ignored() {
    let pass_count = Arc::new(AtomicUsize::new(0));
    let page_count = Arc::new(AtomicUsize::new(0));
    let release_count = Arc::new(AtomicUsize::new(0));
    let service = TestService::new(pass_count, page_count, Arc::clone(&release_count));
    let (_source, events, receiver) = channel::<(), ()>(header());
    let (reply, response) = sync_channel(1);
    drop(response);
    events
        .send(SourceBrokerEvent::BeginPass { reply })
        .unwrap();
    let failure = service_until_finished(receiver, service).unwrap_err();
    assert!(matches!(
        failure,
        ProjectionCoordinatorError::ProjectionWorkerStopped
    ));
    assert_eq!(release_count.load(Ordering::Acquire), 1);
}

struct FailingService {
    failure: StreamedInputSourceError,
}

impl StreamedInputBrokerService for FailingService {
    fn header(&self) -> StreamedInputHeader {
        header()
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        Err(self.failure.clone())
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        Err(StreamedInputSourceError::InvalidSource)
    }

    fn read_text_page(
        &mut self,
        _source_id: StreamedTextSourceId,
        _start: u64,
        _max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        Err(StreamedInputSourceError::InvalidSource)
    }
}

#[test]
fn typed_source_failures_cross_the_broker_without_collapsing() {
    let failures = [
        StreamedInputSourceError::Cancelled,
        StreamedInputSourceError::RevisionDrift {
            expected: source_revision(),
            actual: StreamedInputSourceRevision::new(8),
        },
        StreamedInputSourceError::ReadFailed,
        StreamedInputSourceError::MalformedTextSegmentation { item_ordinal: 1 },
    ];
    for expected in failures {
        let (mut source, events, receiver) = channel::<(), ()>(header());
        let worker = std::thread::spawn(move || {
            let failure = source.begin_pass().unwrap_err();
            events.send(SourceBrokerEvent::Finished(Ok(
                ConnectionCommandOutcome::new((), None),
            )))
            .unwrap();
            failure
        });
        let service = FailingService {
            failure: expected.clone(),
        };
        service_until_finished(receiver, service)
            .unwrap()
            .unwrap();
        assert_eq!(worker.join().unwrap(), expected);
    }
}
