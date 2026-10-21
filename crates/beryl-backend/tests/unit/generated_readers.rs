use super::*;
use beryl_wsl_supervisor::{DiagnosticTail, ExitStatus, WorkloadResult, decode_frame};
use std::sync::mpsc::{Receiver, sync_channel};

fn fixture(limit: usize) -> (Arc<ReaderRegistry>, WslGeneratedReader, Receiver<Vec<u8>>) {
    let registry = Arc::new(ReaderRegistry::default());
    registry
        .configure(GeneratedReaderAllowance::new(limit).unwrap())
        .unwrap();
    let (sender, receiver) = sync_channel(8);
    let reader = registry
        .reserve(Arc::new(Mutex::new(Some(sender))), [3; 32])
        .unwrap();
    (registry, reader, receiver)
}
fn opened(registry: &ReaderRegistry, reader: &WslGeneratedReader, length: u64) {
    assert!(
        registry
            .route(Frame::ReaderOpened {
                id: reader.slot.id,
                length
            })
            .unwrap()
            .is_none()
    );
    assert_eq!(
        reader.wait(None).unwrap(),
        Frame::ReaderOpened {
            id: reader.slot.id,
            length
        }
    );
}
fn request(receiver: &Receiver<Vec<u8>>) -> Frame {
    decode_frame(
        &[3; 32],
        &receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
    )
    .unwrap()
}
fn namespace_closed() -> Frame {
    Frame::OwnedNamespaceClosed {
        result: WorkloadResult {
            exit: ExitStatus::Exited(0),
            observation: None,
            stdout: DiagnosticTail::default(),
            stderr: DiagnosticTail::default(),
        },
    }
}

#[test]
fn reader_admission_is_bounded_immutable_and_ids_never_reused() {
    let (registry, reader, _) = fixture(1);
    assert_eq!(
        registry.configure(GeneratedReaderAllowance::new(2).unwrap()),
        Err(GeneratedOutputError::Busy)
    );
    assert!(matches!(
        registry.reserve(reader.sender.clone(), [3; 32]),
        Err(GeneratedOutputError::Busy)
    ));
    let first = reader.slot.id;
    lock(&reader.slot.state).closed = true;
    reader.close(false).unwrap();
    let next = registry.reserve(reader.sender.clone(), [3; 32]).unwrap();
    assert!(next.slot.id > first);
    assert_eq!(lock(&registry.state).readers.len(), 1);
    lock(&registry.state).next_id = u128::MAX;
    lock(&next.slot.state).closed = true;
    next.close(false).unwrap();
    assert!(matches!(
        registry.reserve(reader.sender.clone(), [3; 32]),
        Err(GeneratedOutputError::Protocol)
    ));
}

#[test]
fn unsent_open_releases_its_reservation_without_native_disposal_request() {
    let (registry, reader, receiver) = fixture(1);
    for _ in 0..8 {
        lock(&reader.sender)
            .as_ref()
            .unwrap()
            .try_send(vec![])
            .unwrap();
    }
    assert_eq!(
        reader.open("/output", &AtomicBool::new(false)),
        Err(GeneratedOutputError::Busy)
    );
    assert!(lock(&registry.state).readers.is_empty());
    reader.close(false).unwrap();
    assert_eq!(receiver.try_iter().count(), 8);
}

#[test]
fn independent_reads_reset_offset_and_probe_the_physical_eof() {
    let (registry, reader, receiver) = fixture(1);
    opened(&registry, &reader, 2);
    std::thread::scope(|scope| {
        for (sequence, offset, bytes, eof) in [
            (1, 0, vec![4, 5], false),
            (2, 2, vec![], true),
            (3, 0, vec![4, 5], false),
        ] {
            let reader = &reader;
            let task = scope.spawn(move || {
                let mut page = [0; 8192];
                let count = reader
                    .read(offset, &mut page, &AtomicBool::new(false))
                    .unwrap();
                page[..count].to_vec()
            });
            assert_eq!(
                request(&receiver),
                Frame::ReaderRead {
                    id: reader.slot.id,
                    sequence,
                    offset,
                    allowance: 8192
                }
            );
            registry
                .route(Frame::ReaderPage {
                    id: reader.slot.id,
                    sequence,
                    offset,
                    bytes: bytes.clone(),
                    eof,
                })
                .unwrap();
            assert_eq!(task.join().unwrap(), bytes);
        }
    });
}

#[test]
fn request_queue_refusal_does_not_consume_sequence_or_leave_outstanding_read() {
    let (registry, reader, receiver) = fixture(1);
    opened(&registry, &reader, 1);
    for _ in 0..8 {
        lock(&reader.sender)
            .as_ref()
            .unwrap()
            .try_send(vec![])
            .unwrap();
    }
    assert_eq!(
        reader.read(0, &mut [0], &AtomicBool::new(false)),
        Err(GeneratedOutputError::Busy)
    );
    let state = lock(&reader.slot.state);
    assert_eq!(state.next_sequence, 1);
    assert!(!state.request_in_flight);
    drop(state);
    assert_eq!(receiver.try_iter().count(), 8);
}

#[test]
fn wrong_id_sequence_offset_allowance_and_eof_fail_closed() {
    for mutation in 0..6 {
        let (registry, reader, _) = fixture(1);
        opened(&registry, &reader, 2);
        lock(&reader.slot.state).pending = Pending::Read {
            sequence: 1,
            offset: 0,
            allowance: 2,
        };
        let mut id = reader.slot.id;
        let mut sequence = 1;
        let mut offset = 0;
        let mut bytes = vec![1, 2];
        let mut eof = false;
        match mutation {
            0 => id = 9u128.to_be_bytes(),
            1 => sequence = 2,
            2 => offset = 1,
            3 => bytes.push(3),
            4 => eof = true,
            5 => bytes.clear(),
            _ => unreachable!(),
        }
        assert_eq!(
            registry.route(Frame::ReaderPage {
                id,
                sequence,
                offset,
                bytes,
                eof
            }),
            Err(GeneratedOutputError::Protocol)
        );
    }
}

#[test]
fn unread_page_does_not_block_control_progress_and_cancel_waits_for_matching_close() {
    let (registry, reader, receiver) = fixture(1);
    opened(&registry, &reader, 2);
    let cancelled = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let read = scope.spawn(|| reader.read(0, &mut [0; 2], &cancelled));
        assert!(matches!(
            request(&receiver),
            Frame::ReaderRead { sequence: 1, .. }
        ));
        cancelled.store(true, Ordering::Release);
        assert_eq!(read.join().unwrap(), Err(GeneratedOutputError::Cancelled));
        assert_eq!(
            registry.route(Frame::ShutdownPending).unwrap(),
            Some(Frame::ShutdownPending)
        );
        let close = scope.spawn(|| reader.close(true));
        assert_eq!(
            request(&receiver),
            Frame::ReaderCancel {
                id: reader.slot.id,
                sequence: 2
            }
        );
        registry
            .route(Frame::ReaderPage {
                id: reader.slot.id,
                sequence: 1,
                offset: 0,
                bytes: vec![1, 2],
                eof: false,
            })
            .unwrap();
        assert!(lock(&reader.slot.state).reply.is_none());
        assert_eq!(lock(&registry.state).readers.len(), 1);
        registry
            .route(Frame::ReaderClosed {
                id: reader.slot.id,
                sequence: 2,
                cancelled: true,
            })
            .unwrap();
        close.join().unwrap().unwrap();
    });
    assert!(lock(&registry.state).readers.is_empty());
}

#[test]
fn automatic_failure_receipt_cannot_acknowledge_a_later_emitted_close() {
    let (registry, reader, receiver) = fixture(1);
    opened(&registry, &reader, 2);
    let cancelled = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let read = scope.spawn(|| reader.read(0, &mut [0; 2], &cancelled));
        request(&receiver);
        cancelled.store(true, Ordering::Release);
        assert_eq!(read.join().unwrap(), Err(GeneratedOutputError::Cancelled));
        let close = scope.spawn(|| reader.close(true));
        assert_eq!(
            request(&receiver),
            Frame::ReaderCancel {
                id: reader.slot.id,
                sequence: 2
            }
        );
        registry
            .route(Frame::ReaderFailed {
                id: reader.slot.id,
                sequence: 1,
                kind: ReaderFailure::Changed,
            })
            .unwrap();
        registry
            .route(Frame::ReaderClosed {
                id: reader.slot.id,
                sequence: 1,
                cancelled: true,
            })
            .unwrap();
        assert!(!reader.closed());
        assert_eq!(lock(&registry.state).readers.len(), 1);
        registry
            .route(Frame::ReaderClosed {
                id: reader.slot.id,
                sequence: 2,
                cancelled: true,
            })
            .unwrap();
        close.join().unwrap().unwrap();
    });
}

#[test]
fn namespace_closure_requires_original_channel_join_before_source_release() {
    let (registry, reader, _) = fixture(1);
    assert_eq!(
        registry.route(namespace_closed()).unwrap(),
        Some(namespace_closed())
    );
    assert!(!reader.current());
    assert!(!reader.closed());
    assert_eq!(lock(&registry.state).readers.len(), 1);
    registry.joined();
    assert!(reader.closed());
    reader.close(true).unwrap();
    assert!(lock(&registry.state).readers.is_empty());
}

#[test]
fn failure_or_channel_join_alone_never_fabricates_namespace_closure() {
    let (registry, reader, _) = fixture(1);
    registry.fail();
    registry.joined();
    assert!(!reader.closed());
    assert!(!reader.current());
    assert_eq!(lock(&registry.state).readers.len(), 1);
}

#[test]
fn automatic_failure_still_holds_admission_until_explicit_final_close() {
    let (registry, reader, receiver) = fixture(1);
    registry
        .route(Frame::ReaderFailed {
            id: reader.slot.id,
            sequence: 0,
            kind: ReaderFailure::Missing,
        })
        .unwrap();
    registry
        .route(Frame::ReaderClosed {
            id: reader.slot.id,
            sequence: 0,
            cancelled: true,
        })
        .unwrap();
    assert!(!reader.closed());
    assert!(lock(&reader.slot.state).automatic_closed);
    assert!(matches!(
        registry.reserve(reader.sender.clone(), [3; 32]),
        Err(GeneratedOutputError::Busy)
    ));
    std::thread::scope(|scope| {
        let close = scope.spawn(|| reader.close(false));
        assert_eq!(
            request(&receiver),
            Frame::ReaderClose {
                id: reader.slot.id,
                sequence: 1
            }
        );
        registry
            .route(Frame::ReaderClosed {
                id: reader.slot.id,
                sequence: 1,
                cancelled: false,
            })
            .unwrap();
        close.join().unwrap().unwrap();
    });
    assert!(lock(&registry.state).readers.is_empty());
}

#[test]
fn original_reader_diagnostics_expose_lifecycle_counts_without_pending_image_bytes() {
    let (registry, reader, _) = fixture(1);
    lock(&reader.slot.state).reply = Some(Frame::ReaderPage {
        id: reader.slot.id,
        sequence: 1,
        offset: 0,
        bytes: b"sensitive-original-page".to_vec(),
        eof: false,
    });
    let diagnostic = format!("{registry:?} {reader:?}");
    assert!(diagnostic.contains("readers: 1"));
    assert!(diagnostic.contains("reply_pending: true"));
    assert!(!diagnostic.contains("sensitive"));
    assert!(!diagnostic.contains("115, 101, 110"));
    assert!(!diagnostic.contains("ReaderPage"));
    assert!(diagnostic.len() < 1000);
}
