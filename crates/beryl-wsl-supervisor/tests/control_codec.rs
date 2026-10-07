use beryl_wsl_supervisor::*;
use std::io::Cursor;

fn frames() -> Vec<Frame> {
    vec![
        Frame::Initialize,
        Frame::LaunchServer {
            executable: "/opt/codex".into(),
            execution_root: "/home/operator/work".into(),
            arguments: vec![
                "app-server".into(),
                "--listen".into(),
                "ws://127.0.0.1:9876".into(),
            ],
        },
        Frame::Observe {
            kind: ObservationKind::Executable,
            path: Some("/opt/codex".into()),
            timeout_ms: 30_000,
        },
        Frame::Observe {
            kind: ObservationKind::Home,
            path: None,
            timeout_ms: 1,
        },
        Frame::Stop,
        Frame::BrokerClose,
        Frame::Ready {
            role: Role::ContextBroker,
        },
        Frame::Ready {
            role: Role::Supervisor,
        },
        Frame::WorkloadStarted,
        Frame::ShutdownPending,
        Frame::OwnedNamespaceClosed {
            result: WorkloadResult {
                exit: ExitStatus::Signaled(9),
                observation: Some("/home/operator".into()),
                stdout: DiagnosticTail {
                    bytes: vec![0, 255, b'\n'],
                    truncated: true,
                },
                stderr: DiagnosticTail::default(),
            },
        },
        Frame::LinuxCompanionsClosed,
        Frame::Failure {
            kind: FailureKind::Credentials,
            errno: Some(13),
        },
    ]
}
#[test]
fn closed_payloads_roundtrip_and_fragment_at_every_boundary() {
    let nonce = [73; 32];
    for frame in frames() {
        let encoded = encode_frame(&nonce, &frame).unwrap();
        assert_eq!(decode_frame(&nonce, &encoded).unwrap(), frame);
        let (initial_nonce, initial) = read_initial_frame(Cursor::new(&encoded)).unwrap();
        assert_eq!(initial_nonce, nonce);
        assert_eq!(initial, frame);
        for split in 0..=encoded.len() {
            let mut reader = FrameReader::new(nonce);
            let mut output = reader.feed(&encoded[..split]).unwrap();
            output.extend(reader.feed(&encoded[split..]).unwrap());
            reader.finish().unwrap();
            assert_eq!(output, vec![frame.clone()]);
        }
    }
}
#[test]
fn stream_reader_leaves_the_next_frame_for_the_next_read() {
    let nonce = [8; 32];
    let expected = [Frame::Stop, Frame::LinuxCompanionsClosed];
    let bytes: Vec<_> = expected
        .iter()
        .flat_map(|frame| encode_frame(&nonce, frame).unwrap())
        .collect();
    let mut input = Cursor::new(bytes);
    let mut reader = FrameReader::new(nonce);
    for frame in expected {
        assert_eq!(reader.read_frame(&mut input).unwrap(), Some(frame));
    }
    assert_eq!(reader.read_frame(&mut input).unwrap(), None);
}
#[test]
fn rejects_nonce_tags_versions_lengths_trailing_and_partial_output() {
    let nonce = [4; 32];
    let encoded = encode_frame(&nonce, &Frame::Stop).unwrap();
    for (offset, replacement) in [(0, 0), (8, 2), (10, 99), (16, 9)] {
        let mut bytes = encoded.clone();
        bytes[offset] = replacement;
        assert!(decode_frame(&nonce, &bytes).is_err());
    }
    let mut bytes = encoded.clone();
    bytes[12..16].copy_from_slice(&((MAX_PAYLOAD_LEN + 1) as u32).to_le_bytes());
    assert!(FrameReader::new(nonce).feed(&bytes).is_err());
    let mut bytes = encoded.clone();
    bytes.push(0);
    assert!(decode_frame(&nonce, &bytes).is_err());
    for count in 1..encoded.len() {
        let mut reader = FrameReader::new(nonce);
        reader.feed(&encoded[..count]).unwrap();
        assert!(reader.finish().is_err());
    }
    let mut reader = FrameReader::new(nonce);
    let mut bytes = encoded.clone();
    bytes[8] = 2;
    assert!(reader.feed(&bytes).is_err());
    assert!(reader.feed(&encoded).is_err());
}
#[test]
fn rejects_oversized_or_untyped_launch_observation_and_result_fields() {
    let nonce = [7; 32];
    for frame in [
        Frame::LaunchServer {
            executable: "relative".into(),
            execution_root: "/".into(),
            arguments: vec![],
        },
        Frame::LaunchServer {
            executable: "/x".into(),
            execution_root: "/".into(),
            arguments: vec!["x".repeat(8193)],
        },
        Frame::LaunchServer {
            executable: "/x".into(),
            execution_root: "/".into(),
            arguments: vec!["".into(); 65],
        },
        Frame::Observe {
            kind: ObservationKind::Home,
            path: Some("/".into()),
            timeout_ms: 1,
        },
        Frame::Observe {
            kind: ObservationKind::Directory,
            path: Some("/".into()),
            timeout_ms: 0,
        },
        Frame::Observe {
            kind: ObservationKind::Directory,
            path: Some("/".into()),
            timeout_ms: 30_001,
        },
        Frame::Failure {
            kind: FailureKind::Exec,
            errno: Some(-1),
        },
    ] {
        assert!(encode_frame(&nonce, &frame).is_err());
    }
    let frame = Frame::LaunchServer {
        executable: "/x".into(),
        execution_root: "/".into(),
        arguments: vec!["x".repeat(8192); 64],
    };
    assert!(encode_frame(&nonce, &frame).is_err());
    let bytes: Vec<_> = (0..9)
        .flat_map(|_| encode_frame(&nonce, &Frame::Stop).unwrap())
        .collect();
    assert!(FrameReader::new(nonce).feed(&bytes).is_err());
}
#[test]
fn progress_is_monotonic_and_setup_failure_can_close_without_starting() {
    let mut state = ControlState::new(Role::Supervisor);
    assert!(state.accept(&Frame::WorkloadStarted).is_err());
    assert!(state.accept(&Frame::LinuxCompanionsClosed).is_err());
    state
        .accept(&Frame::Ready {
            role: Role::Supervisor,
        })
        .unwrap();
    state
        .accept(&Frame::Failure {
            kind: FailureKind::Exec,
            errno: Some(2),
        })
        .unwrap();
    state
        .accept(&Frame::OwnedNamespaceClosed {
            result: WorkloadResult {
                exit: ExitStatus::Exited(1),
                observation: None,
                stdout: DiagnosticTail::default(),
                stderr: DiagnosticTail::default(),
            },
        })
        .unwrap();
    state.accept(&Frame::ShutdownPending).unwrap();
    assert!(state.accept(&Frame::WorkloadStarted).is_err());
    state.accept(&Frame::LinuxCompanionsClosed).unwrap();
    assert!(state.accept(&Frame::Stop).is_err());
}
