use beryl_wsl_supervisor::*;
use std::io::{self, Read};

struct RecordBuffer {
    encoded: Vec<u8>,
    position: usize,
    body_allocation: Option<usize>,
}
impl Read for RecordBuffer {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = output.len().min(self.encoded.len() - self.position);
        if self.position >= HEADER_LEN && count != 0 {
            self.body_allocation = Some(output.as_ptr() as usize - self.position);
        }
        output[..count].copy_from_slice(&self.encoded[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

fn frames() -> Vec<Frame> {
    let id = 1u128.to_be_bytes();
    let mut frames = vec![
        Frame::ConfigureReaders { limit: 2 },
        Frame::ReaderOpen {
            id,
            path: "/home/operator/output.png".into(),
        },
        Frame::ReaderRead {
            id,
            sequence: 1,
            offset: 0,
            allowance: MAX_READER_PAGE as u32,
        },
        Frame::ReaderRead {
            id,
            sequence: 2,
            offset: MAX_READER_PAGE as u64,
            allowance: 1,
        },
        Frame::ReaderCancel { id, sequence: 3 },
        Frame::ReaderClose { id, sequence: 3 },
        Frame::ReaderOpened {
            id,
            length: MAX_READER_PAGE as u64,
        },
        Frame::ReaderPage {
            id,
            sequence: 1,
            offset: 0,
            bytes: vec![17; MAX_READER_PAGE],
            eof: false,
        },
        Frame::ReaderPage {
            id,
            sequence: 2,
            offset: MAX_READER_PAGE as u64,
            bytes: vec![],
            eof: true,
        },
        Frame::ReaderClosed {
            id,
            sequence: 0,
            cancelled: true,
        },
        Frame::ReaderClosed {
            id,
            sequence: 3,
            cancelled: false,
        },
    ];
    for kind in [
        ReaderFailure::Refused,
        ReaderFailure::Missing,
        ReaderFailure::Unreadable,
        ReaderFailure::Changed,
        ReaderFailure::Protocol,
        ReaderFailure::Cancelled,
    ] {
        frames.push(Frame::ReaderFailed {
            id,
            sequence: 0,
            kind,
        });
    }
    frames
}

#[test]
fn generated_reader_frames_roundtrip_with_bounded_fragmented_pages() {
    let nonce = [93; 32];
    for frame in frames() {
        let bytes = encode_frame(&nonce, &frame).unwrap();
        assert_eq!(decode_frame(&nonce, &bytes).unwrap(), frame);
        for split in [
            0,
            1,
            HEADER_LEN - 1,
            HEADER_LEN,
            bytes.len() - 1,
            bytes.len(),
        ] {
            let mut reader = FrameReader::new(nonce);
            let mut decoded = reader.feed(&bytes[..split]).unwrap();
            decoded.extend(reader.feed(&bytes[split..]).unwrap());
            reader.finish().unwrap();
            assert_eq!(decoded, vec![frame.clone()]);
        }
    }
}

#[test]
fn streaming_decode_transfers_the_single_bounded_wire_allocation_to_the_page() {
    let nonce = [39; 32];
    let mut input = RecordBuffer {
        encoded: encode_frame(
            &nonce,
            &Frame::ReaderPage {
                id: 1u128.to_be_bytes(),
                sequence: 1,
                offset: 0,
                bytes: vec![71; MAX_READER_PAGE],
                eof: false,
            },
        )
        .unwrap(),
        position: 0,
        body_allocation: None,
    };
    let mut reader = FrameReader::new(nonce);
    let Some(Frame::ReaderPage { bytes, .. }) = reader.read_frame(&mut input).unwrap() else {
        panic!("expected reader page")
    };
    assert_eq!(Some(bytes.as_ptr() as usize), input.body_allocation);
    assert_eq!(bytes, vec![71; MAX_READER_PAGE]);
    assert!(bytes.capacity() <= MAX_READER_PAGE + 85);
    reader.finish().unwrap();
}

#[test]
fn reader_encoding_refuses_unbounded_pages_zero_ids_and_sequence_overflow() {
    let id = 1u128.to_be_bytes();
    for frame in [
        Frame::ConfigureReaders { limit: 0 },
        Frame::ReaderOpen {
            id: [0; 16],
            path: "/output".into(),
        },
        Frame::ReaderOpen {
            id,
            path: "relative".into(),
        },
        Frame::ReaderOpen {
            id,
            path: format!("/{}", "x".repeat(MAX_PATH_LEN)),
        },
        Frame::ReaderRead {
            id,
            sequence: 0,
            offset: 0,
            allowance: 1,
        },
        Frame::ReaderRead {
            id,
            sequence: 1,
            offset: 0,
            allowance: 0,
        },
        Frame::ReaderRead {
            id,
            sequence: 1,
            offset: 0,
            allowance: MAX_READER_PAGE as u32 + 1,
        },
        Frame::ReaderRead {
            id,
            sequence: 1,
            offset: u64::MAX,
            allowance: 1,
        },
        Frame::ReaderClose { id, sequence: 0 },
        Frame::ReaderCancel { id, sequence: 0 },
        Frame::ReaderOpened { id, length: 0 },
        Frame::ReaderPage {
            id,
            sequence: 1,
            offset: 0,
            bytes: vec![1; MAX_READER_PAGE + 1],
            eof: false,
        },
        Frame::ReaderPage {
            id,
            sequence: 1,
            offset: u64::MAX,
            bytes: vec![1],
            eof: false,
        },
    ] {
        assert!(encode_frame(&[3; 32], &frame).is_err());
    }
}

#[test]
fn reader_decoding_rechecks_id_sequence_allowance_and_closed_discriminants() {
    let nonce = [2; 32];
    let request = encode_frame(
        &nonce,
        &Frame::ReaderRead {
            id: 1u128.to_be_bytes(),
            sequence: 1,
            offset: 0,
            allowance: 1,
        },
    )
    .unwrap();
    for (start, count) in [(HEADER_LEN, 16), (HEADER_LEN + 16, 8), (HEADER_LEN + 32, 4)] {
        let mut malformed = request.clone();
        malformed[start..start + count].fill(0);
        assert!(decode_frame(&nonce, &malformed).is_err());
    }
    let mut request = request;
    request[HEADER_LEN + 32..HEADER_LEN + 36]
        .copy_from_slice(&((MAX_READER_PAGE + 1) as u32).to_le_bytes());
    assert!(decode_frame(&nonce, &request).is_err());
    for frame in [
        Frame::ReaderClosed {
            id: 1u128.to_be_bytes(),
            sequence: 1,
            cancelled: false,
        },
        Frame::ReaderFailed {
            id: 1u128.to_be_bytes(),
            sequence: 1,
            kind: ReaderFailure::Protocol,
        },
    ] {
        let mut malformed = encode_frame(&nonce, &frame).unwrap();
        *malformed.last_mut().unwrap() = 255;
        assert!(decode_frame(&nonce, &malformed).is_err());
    }
}

#[test]
fn borrowed_and_streaming_page_decoders_reject_truncation_counts_eof_flags_and_overflow() {
    let nonce = [27; 32];
    let valid = encode_frame(
        &nonce,
        &Frame::ReaderPage {
            id: 1u128.to_be_bytes(),
            sequence: 1,
            offset: 0,
            bytes: vec![4, 5],
            eof: false,
        },
    )
    .unwrap();
    let mut malformed = Vec::new();
    for length in 1..valid.len() {
        malformed.push(valid[..length].to_vec());
    }
    for count in [0u32, 1, 3, MAX_READER_PAGE as u32 + 1] {
        let mut bytes = valid.clone();
        bytes[HEADER_LEN + 32..HEADER_LEN + 36].copy_from_slice(&count.to_le_bytes());
        malformed.push(bytes);
    }
    for (start, count) in [(HEADER_LEN, 16), (HEADER_LEN + 16, 8)] {
        let mut bytes = valid.clone();
        bytes[start..start + count].fill(0);
        malformed.push(bytes);
    }
    let mut bytes = valid.clone();
    bytes[HEADER_LEN + 24..HEADER_LEN + 32].copy_from_slice(&u64::MAX.to_le_bytes());
    malformed.push(bytes);
    let mut bytes = valid;
    *bytes.last_mut().unwrap() = 2;
    malformed.push(bytes);
    for bytes in malformed {
        assert!(decode_frame(&nonce, &bytes).is_err());
        assert!(
            FrameReader::new(nonce)
                .read_frame(std::io::Cursor::new(&bytes))
                .is_err()
        );
        let mut reader = FrameReader::new(nonce);
        assert!(reader.feed(&bytes).and_then(|_| reader.finish()).is_err());
    }
}

#[test]
fn reader_replies_require_original_supervisor_started_and_not_closed() {
    let reply = Frame::ReaderOpened {
        id: 1u128.to_be_bytes(),
        length: 1,
    };
    let mut state = ControlState::new(Role::Supervisor);
    assert!(state.accept(&reply).is_err());
    state
        .accept(&Frame::Ready {
            role: Role::Supervisor,
        })
        .unwrap();
    assert!(state.accept(&reply).is_err());
    state.accept(&Frame::WorkloadStarted).unwrap();
    state.accept(&reply).unwrap();
    state.accept(&Frame::ShutdownPending).unwrap();
    state.accept(&reply).unwrap();
    state
        .accept(&Frame::OwnedNamespaceClosed {
            result: WorkloadResult {
                exit: ExitStatus::Exited(0),
                observation: None,
                stdout: DiagnosticTail::default(),
                stderr: DiagnosticTail::default(),
            },
        })
        .unwrap();
    assert!(state.accept(&reply).is_err());
    let mut broker = ControlState::new(Role::ContextBroker);
    broker
        .accept(&Frame::Ready {
            role: Role::ContextBroker,
        })
        .unwrap();
    assert!(broker.accept(&reply).is_err());
}

#[test]
fn reader_header_bounds_are_rejected_before_reading_or_allocating_a_body() {
    struct HeaderOnly(std::io::Cursor<Vec<u8>>);
    impl Read for HeaderOnly {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            assert_eq!(
                self.0.position(),
                0,
                "body read after invalid reader header"
            );
            self.0.read(output)
        }
    }
    let nonce = [53; 32];
    for tag in [6u16, 7, 8, 9, 10, 107, 108, 109, 110] {
        let mut header = encode_frame(
            &nonce,
            &Frame::ReaderClosed {
                id: 1u128.to_be_bytes(),
                sequence: 1,
                cancelled: false,
            },
        )
        .unwrap();
        header.truncate(HEADER_LEN);
        header[10..12].copy_from_slice(&tag.to_le_bytes());
        header[12..16].copy_from_slice(&(MAX_PAYLOAD_LEN as u32).to_le_bytes());
        assert!(FrameReader::new(nonce).feed(&header).is_err());
        assert!(
            FrameReader::new(nonce)
                .read_frame(HeaderOnly(std::io::Cursor::new(header)))
                .is_err()
        );
    }
}
