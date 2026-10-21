#![cfg(target_os = "linux")]
#[path = "support/generated_reader.rs"]
mod readers;
#[path = "support/native_role.rs"]
mod support;
use beryl_wsl_supervisor::*;
use readers::*;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use support::retire;

#[test]
fn positional_passes_keep_one_open_file_through_path_replacement_and_physical_eof() {
    let fixture = Fixture::new();
    let bytes: Vec<_> = (0..MAX_READER_PAGE * 2 + 73)
        .map(|index| (index % 251) as u8)
        .collect();
    let path = fixture.file("original", &bytes);
    let nonce = [121; 32];
    let (broker, mut root) = server(nonce, 1);
    open(&mut root, nonce, 1, &path, bytes.len() as u64);
    let (reader, _, init) = reader_process(&root);
    assert_eq!(
        std::fs::read_link(format!("/proc/{reader}/ns/pid")).unwrap(),
        std::fs::read_link(format!("/proc/{init}/ns/pid")).unwrap()
    );
    assert_ne!(
        std::fs::read_link(format!("/proc/{reader}/ns/pid")).unwrap(),
        std::fs::read_link("/proc/self/ns/pid").unwrap()
    );
    let descriptors: Vec<_> = std::fs::read_dir(format!("/proc/{reader}/fd"))
        .unwrap()
        .map(|entry| std::fs::read_link(entry.unwrap().path()).unwrap())
        .collect();
    assert_eq!(descriptors.len(), 2, "{descriptors:?}");
    assert_eq!(
        descriptors
            .iter()
            .filter(|path| path.to_string_lossy().starts_with("socket:"))
            .count(),
        1
    );
    assert!(
        !descriptors
            .iter()
            .any(|path| path.to_string_lossy().contains("pidfd"))
    );
    std::fs::rename(&path, fixture.root.join("held")).unwrap();
    fixture.file("original", b"replacement must never be adopted");
    let mut sequence = 0;
    for _ in 0..3 {
        let mut actual = Vec::new();
        while actual.len() < bytes.len() {
            sequence += 1;
            let offset = actual.len() as u64;
            let allowance = (bytes.len() - actual.len()).min(MAX_READER_PAGE) as u32;
            root.send(
                &nonce,
                &Frame::ReaderRead {
                    id: id(1),
                    sequence,
                    offset,
                    allowance,
                },
            );
            let Frame::ReaderPage {
                id: actual_id,
                sequence: actual_sequence,
                offset: actual_offset,
                bytes: page,
                eof,
            } = root.next()
            else {
                panic!("positional data required")
            };
            assert_eq!(
                (actual_id, actual_sequence, actual_offset, eof),
                (id(1), sequence, offset, false)
            );
            assert!(page.len() <= allowance as usize);
            actual.extend(page);
        }
        assert_eq!(actual, bytes);
        sequence += 1;
        root.send(
            &nonce,
            &Frame::ReaderRead {
                id: id(1),
                sequence,
                offset: bytes.len() as u64,
                allowance: 1,
            },
        );
        assert_eq!(
            root.next(),
            Frame::ReaderPage {
                id: id(1),
                sequence,
                offset: bytes.len() as u64,
                bytes: vec![],
                eof: true
            }
        );
    }
    close(&mut root, nonce, 1, sequence + 1, false);
    root.send(&nonce, &Frame::Stop);
    assert!(retire(broker, root).1.is_empty());
}

#[test]
fn length_growth_and_shrinkage_fail_instead_of_synthesizing_eof() {
    for (nonce_byte, changed_length) in [(122, 65), (123, 16)] {
        let fixture = Fixture::new();
        let path = fixture.file("source", &[7; 32]);
        let nonce = [nonce_byte; 32];
        let (broker, mut root) = server(nonce, 1);
        open(&mut root, nonce, 1, &path, 32);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(changed_length)
            .unwrap();
        root.send(
            &nonce,
            &Frame::ReaderRead {
                id: id(1),
                sequence: 1,
                offset: 32,
                allowance: 1,
            },
        );
        failed(&mut root, 1, 1, ReaderFailure::Changed);
        root.send(&nonce, &Frame::Stop);
        assert!(retire(broker, root).1.is_empty());
    }
}

#[test]
fn reader_capacity_refuses_without_a_path_queue_and_reuses_only_fresh_ids() {
    let fixture = Fixture::new();
    let path = fixture.file("source", b"original");
    let nonce = [124; 32];
    let (broker, mut root) = server(nonce, 1);
    open(&mut root, nonce, 1, &path, 8);
    root.send(
        &nonce,
        &Frame::ReaderOpen {
            id: id(2),
            path: path.to_str().unwrap().into(),
        },
    );
    failed(&mut root, 2, 0, ReaderFailure::Refused);
    close(&mut root, nonce, 1, 1, false);
    open(&mut root, nonce, 3, &path, 8);
    close(&mut root, nonce, 3, 1, false);
    root.send(
        &nonce,
        &Frame::ReaderOpen {
            id: id(1),
            path: path.to_str().unwrap().into(),
        },
    );
    assert!(matches!(
        root.next(),
        Frame::Failure {
            kind: FailureKind::Protocol,
            ..
        }
    ));
    assert!(retire(broker, root).1.is_empty());
}

#[test]
fn missing_empty_directory_and_fifo_sources_close_the_exact_failed_child() {
    let fixture = Fixture::new();
    let empty = fixture.file("empty", b"");
    let fifo = fixture.root.join("fifo");
    let fifo_c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
    let nonce = [125; 32];
    let (broker, mut root) = server(nonce, 1);
    for (number, path, kind) in [
        (1, fixture.root.join("missing"), ReaderFailure::Missing),
        (2, empty, ReaderFailure::Unreadable),
        (3, fixture.root.clone(), ReaderFailure::Unreadable),
        (4, fifo, ReaderFailure::Unreadable),
    ] {
        root.send(
            &nonce,
            &Frame::ReaderOpen {
                id: id(number),
                path: path.to_str().unwrap().into(),
            },
        );
        failed(&mut root, number, 0, kind);
        close(&mut root, nonce, number, 1, true);
    }
    root.send(&nonce, &Frame::Stop);
    assert!(retire(broker, root).1.is_empty());
}

#[test]
fn reader_installs_broker_uid_gid_and_supplementary_groups_before_open() {
    let fixture = Fixture::new();
    let nonce = [126; 32];
    let (broker, mut root) = credential_server(nonce);
    for (number, uid, gid, mode, allowed) in [
        (1, 0, 0, 0o600, false),
        (2, 62001, 0, 0o400, true),
        (3, 0, 62002, 0o040, true),
        (4, 0, 62003, 0o040, true),
        (5, 0, 62004, 0o040, false),
    ] {
        let path = fixture.file(&format!("credentials-{number}"), b"credential-qualified");
        let path_c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::chown(path_c.as_ptr(), uid, gid) }, 0);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        if allowed {
            open(&mut root, nonce, number, &path, 20);
            root.send(
                &nonce,
                &Frame::ReaderRead {
                    id: id(number),
                    sequence: 1,
                    offset: 0,
                    allowance: 20,
                },
            );
            assert_eq!(
                root.next(),
                Frame::ReaderPage {
                    id: id(number),
                    sequence: 1,
                    offset: 0,
                    bytes: b"credential-qualified".to_vec(),
                    eof: false
                }
            );
            close(&mut root, nonce, number, 2, false);
        } else {
            root.send(
                &nonce,
                &Frame::ReaderOpen {
                    id: id(number),
                    path: path.to_str().unwrap().into(),
                },
            );
            failed(&mut root, number, 0, ReaderFailure::Unreadable);
            close(&mut root, nonce, number, 1, true);
        }
    }
    root.send(&nonce, &Frame::Stop);
    assert!(retire(broker, root).1.is_empty());
}

#[test]
fn repeated_or_skipped_sequence_fails_the_original_reader_and_joins_it() {
    for (nonce_byte, replay) in [(127, true), (128, false)] {
        let fixture = Fixture::new();
        let path = fixture.file("source", b"sequence");
        let nonce = [nonce_byte; 32];
        let (broker, mut root) = server(nonce, 1);
        open(&mut root, nonce, 1, &path, 8);
        let sequence = if replay {
            root.send(
                &nonce,
                &Frame::ReaderRead {
                    id: id(1),
                    sequence: 1,
                    offset: 0,
                    allowance: 8,
                },
            );
            assert!(matches!(root.next(), Frame::ReaderPage { sequence: 1, .. }));
            1
        } else {
            2
        };
        root.send(
            &nonce,
            &Frame::ReaderRead {
                id: id(1),
                sequence,
                offset: 0,
                allowance: 8,
            },
        );
        failed(&mut root, 1, u64::from(replay), ReaderFailure::Protocol);
        root.send(&nonce, &Frame::Stop);
        assert!(retire(broker, root).1.is_empty());
    }
}

#[test]
fn cancellation_reaps_and_drains_a_stopped_reader_with_an_outstanding_request() {
    let fixture = Fixture::new();
    let path = fixture.file("source", b"cancel");
    let nonce = [129; 32];
    let (broker, mut root) = server(nonce, 1);
    open(&mut root, nonce, 1, &path, 6);
    let (_, reader, _) = reader_process(&root);
    signal(&reader, libc::SIGSTOP);
    root.send(
        &nonce,
        &Frame::ReaderRead {
            id: id(1),
            sequence: 1,
            offset: 0,
            allowance: 6,
        },
    );
    close(&mut root, nonce, 1, 2, true);
    assert!(exited(&reader, Duration::ZERO));
    root.send(&nonce, &Frame::Stop);
    assert!(retire(broker, root).1.is_empty());
}

#[test]
fn automatic_open_failure_keeps_positive_terminal_proof_for_a_racing_cancel() {
    let fixture = Fixture::new();
    let nonce = [131; 32];
    let (broker, mut root) = server(nonce, 1);
    root.send(
        &nonce,
        &Frame::ReaderOpen {
            id: id(1),
            path: fixture.root.join("missing").to_str().unwrap().into(),
        },
    );
    assert_eq!(
        root.next(),
        Frame::ReaderFailed {
            id: id(1),
            sequence: 0,
            kind: ReaderFailure::Missing
        }
    );
    root.send(
        &nonce,
        &Frame::ReaderCancel {
            id: id(1),
            sequence: 1,
        },
    );
    let mut matched = false;
    for _ in 0..2 {
        match root.next() {
            Frame::ReaderClosed {
                id: actual,
                sequence: 0,
                cancelled: true,
            } => assert_eq!(actual, id(1)),
            Frame::ReaderClosed {
                id: actual,
                sequence: 1,
                cancelled: true,
            } => {
                assert_eq!(actual, id(1));
                matched = true;
                break;
            }
            frame => panic!("unexpected cancellation race response: {frame:?}"),
        }
    }
    assert!(matched);
    let path = fixture.file("next", b"fresh");
    open(&mut root, nonce, 2, &path, 5);
    close(&mut root, nonce, 2, 1, false);
    root.send(&nonce, &Frame::Stop);
    assert!(retire(broker, root).1.is_empty());
}

#[test]
fn cancellation_during_open_reaps_the_child_before_a_matching_receipt() {
    let fixture = Fixture::new();
    let path = fixture.file("source", b"opening");
    let nonce = [132; 32];
    let (broker, mut root) = server(nonce, 1);
    let original_init = children(root.child.id());
    assert_eq!(original_init.len(), 1);
    let init = pidfd(original_init[0]);
    signal(&init, libc::SIGSTOP);
    root.send(
        &nonce,
        &Frame::ReaderOpen {
            id: id(1),
            path: path.to_str().unwrap().into(),
        },
    );
    root.send(
        &nonce,
        &Frame::ReaderCancel {
            id: id(1),
            sequence: 1,
        },
    );
    signal(&init, libc::SIGCONT);
    loop {
        match root.next() {
            Frame::ReaderOpened {
                id: actual,
                length: 7,
            } => assert_eq!(actual, id(1)),
            Frame::ReaderClosed {
                id: actual,
                sequence: 1,
                cancelled: true,
            } => {
                assert_eq!(actual, id(1));
                break;
            }
            frame => panic!("unexpected open cancellation response: {frame:?}"),
        }
    }
    root.send(&nonce, &Frame::Stop);
    assert!(retire(broker, root).1.is_empty());
}

#[test]
fn another_slot_can_cycle_without_evicting_an_unacknowledged_automatic_close() {
    let fixture = Fixture::new();
    let path = fixture.file("source", b"slot");
    let nonce = [133; 32];
    let (broker, mut root) = server(nonce, 2);
    root.send(
        &nonce,
        &Frame::ReaderOpen {
            id: id(1),
            path: fixture.root.join("missing").to_str().unwrap().into(),
        },
    );
    failed(&mut root, 1, 0, ReaderFailure::Missing);
    for number in 2..=5 {
        open(&mut root, nonce, number, &path, 4);
        close(&mut root, nonce, number, 1, false);
    }
    close(&mut root, nonce, 1, 1, true);
    root.send(&nonce, &Frame::Stop);
    assert!(retire(broker, root).1.is_empty());
}

#[test]
fn a_full_maximum_page_pipe_does_not_block_original_namespace_retirement() {
    let fixture = Fixture::new();
    let path = fixture.file("source", &vec![42; MAX_READER_PAGE]);
    let nonce = [130; 32];
    let (broker, mut root) = server(nonce, 1);
    open(&mut root, nonce, 1, &path, MAX_READER_PAGE as u64);
    let (_, reader, init) = reader_process(&root);
    let namespace = pidfd(init);
    assert_eq!(
        unsafe { libc::fcntl(root.stdout.as_raw_fd(), libc::F_SETPIPE_SZ, 4096) },
        4096
    );
    root.send(
        &nonce,
        &Frame::ReaderRead {
            id: id(1),
            sequence: 1,
            offset: 0,
            allowance: MAX_READER_PAGE as u32,
        },
    );
    let mut poll = libc::pollfd {
        fd: root.stdout.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(unsafe { libc::poll(&mut poll, 1, 5_000) }, 1);
    let mut available: libc::c_int = 0;
    assert_eq!(
        unsafe { libc::ioctl(root.stdout.as_raw_fd(), libc::FIONREAD, &mut available) },
        0
    );
    assert_eq!(available, 4096);
    root.send(&nonce, &Frame::Stop);
    assert!(
        exited(&namespace, Duration::from_secs(4)),
        "control stalled behind a held page"
    );
    assert!(exited(&reader, Duration::ZERO));
    assert_eq!(
        root.next(),
        Frame::ReaderPage {
            id: id(1),
            sequence: 1,
            offset: 0,
            bytes: vec![42; MAX_READER_PAGE],
            eof: false
        }
    );
    assert!(retire(broker, root).1.is_empty());
}
