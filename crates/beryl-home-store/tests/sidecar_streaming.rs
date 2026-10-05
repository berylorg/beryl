#![cfg(feature = "test-faults")]

use std::{
    fs,
    io::{self, Cursor, Read},
    num::{NonZeroU64, NonZeroUsize},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use beryl_home_store::{
    HomeDomainRequirements, HomeHealthState, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, SidecarByteLimit, SidecarError, SidecarNamespace,
    test_faults::{FaultController, FaultPoint},
};

fn open(path: &Path, faults: &FaultController) -> HomeStore {
    HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap()
    .prepare_publication(HomeDomainRequirements::new())
    .unwrap()
    .publish()
    .unwrap()
}

fn namespace() -> SidecarNamespace {
    SidecarNamespace::new("images").unwrap()
}

fn limit() -> SidecarByteLimit {
    SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap())
}

struct CheckedReader {
    cursor: Cursor<Arc<[u8]>>,
    maximum: usize,
    live: Arc<AtomicUsize>,
    reads: usize,
    cancel_on_read: Option<usize>,
    cancelled: Arc<AtomicBool>,
}

impl Read for CheckedReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        assert!(bytes.len() <= self.maximum);
        self.reads += 1;
        let length = bytes.len().min(3);
        let result = self.cursor.read(&mut bytes[..length]);
        if self.cancel_on_read == Some(self.reads) {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        result
    }
}

impl Drop for CheckedReader {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

#[test]
fn short_reads_are_paged_and_replayed_without_retaining_readers() {
    for page in [1, 2, 7, 256] {
        let directory = tempfile::tempdir().unwrap();
        let store = open(directory.path(), &FaultController::new());
        let bytes: Arc<[u8]> = (0..10_001).map(|value| (value % 251) as u8).collect();
        let live = Arc::new(AtomicUsize::new(0));
        let opens = AtomicUsize::new(0);
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut factory = || {
            assert_eq!(live.fetch_add(1, Ordering::SeqCst), 0);
            opens.fetch_add(1, Ordering::SeqCst);
            Ok(CheckedReader {
                cursor: Cursor::new(Arc::clone(&bytes)),
                maximum: page,
                live: Arc::clone(&live),
                reads: 0,
                cancel_on_read: None,
                cancelled: Arc::clone(&cancelled),
            })
        };
        let admitted = store
            .admit_sidecar_stream(
                namespace(),
                bytes.len() as u64,
                limit(),
                NonZeroUsize::new(page).unwrap(),
                &mut factory,
                || false,
            )
            .unwrap();
        assert_eq!(
            fs::read(admitted.path()).unwrap().as_slice(),
            bytes.as_ref()
        );
        assert_eq!(opens.load(Ordering::SeqCst), 3);
        assert_eq!(live.load(Ordering::SeqCst), 0);
        let reused = store
            .admit_sidecar_stream(
                namespace(),
                bytes.len() as u64,
                limit(),
                NonZeroUsize::new(page).unwrap(),
                &mut factory,
                || false,
            )
            .unwrap();
        assert_eq!(admitted.address(), reused.address());
        assert_eq!(opens.load(Ordering::SeqCst), 5);
        assert_eq!(live.load(Ordering::SeqCst), 0);
        assert_eq!(
            fs::read_dir(admitted.path().parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    }
}

#[test]
fn exact_length_rejects_early_eof_and_trailing_bytes_without_poisoning_home() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path(), &FaultController::new());
    for (declared, actual) in [(2, 3), (4, 3), (0, 1)] {
        let bytes = vec![42; actual];
        let result = store.admit_sidecar_stream(
            namespace(),
            declared,
            limit(),
            NonZeroUsize::new(2).unwrap(),
            || Ok(Cursor::new(bytes.as_slice())),
            || false,
        );
        assert!(matches!(result, Err(SidecarError::LengthMismatch { .. })));
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
    }
    assert!(!directory.path().join("sidecars").exists());
    let empty = store
        .admit_sidecar_stream(
            namespace(),
            0,
            limit(),
            NonZeroUsize::new(1).unwrap(),
            || Ok(Cursor::new(&[] as &[u8])),
            || false,
        )
        .unwrap();
    assert_eq!(empty.address().length(), 0);
    assert!(fs::read(empty.path()).unwrap().is_empty());
}

#[test]
fn over_limit_and_initial_cancellation_do_not_open_a_source() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path(), &FaultController::new());
    let result = store.admit_sidecar_stream::<Cursor<&[u8]>, _, _>(
        namespace(),
        limit().get() + 1,
        limit(),
        NonZeroUsize::new(1).unwrap(),
        || panic!("out of bound input opened a source"),
        || false,
    );
    assert!(matches!(result, Err(SidecarError::BoundExceeded { .. })));
    let result = store.admit_sidecar_stream::<Cursor<&[u8]>, _, _>(
        namespace(),
        1,
        limit(),
        NonZeroUsize::new(1).unwrap(),
        || panic!("cancelled input opened a source"),
        || true,
    );
    assert!(matches!(result, Err(SidecarError::Cancelled)));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}

#[test]
fn cancellation_between_source_pages_and_write_pages_closes_readers() {
    for cancel_pass in [1, 2] {
        let directory = tempfile::tempdir().unwrap();
        let store = open(directory.path(), &FaultController::new());
        let bytes: Arc<[u8]> = vec![17; 80].into();
        let live = Arc::new(AtomicUsize::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut pass = 0;
        let result = store.admit_sidecar_stream(
            namespace(),
            bytes.len() as u64,
            limit(),
            NonZeroUsize::new(7).unwrap(),
            || {
                pass += 1;
                live.fetch_add(1, Ordering::SeqCst);
                Ok(CheckedReader {
                    cursor: Cursor::new(Arc::clone(&bytes)),
                    maximum: 7,
                    live: Arc::clone(&live),
                    reads: 0,
                    cancel_on_read: (pass == cancel_pass).then_some(2),
                    cancelled: Arc::clone(&cancelled),
                })
            },
            || cancelled.load(Ordering::SeqCst),
        );
        assert!(matches!(result, Err(SidecarError::Cancelled)));
        assert_eq!(live.load(Ordering::SeqCst), 0);
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
        store
            .admit_sidecar(namespace(), b"next independent source", limit())
            .unwrap();
    }
}

#[test]
fn changed_replay_fails_closed_and_preserves_existing_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path(), &FaultController::new());
    let original = store
        .admit_sidecar(namespace(), b"original", limit())
        .unwrap();
    let mut pass = 0;
    let result = store.admit_sidecar_stream(
        namespace(),
        8,
        limit(),
        NonZeroUsize::new(5).unwrap(),
        || {
            pass += 1;
            Ok(Cursor::new(if pass == 1 {
                b"original"
            } else {
                b"changed!"
            }))
        },
        || false,
    );
    assert!(matches!(result, Err(SidecarError::ContentMismatch)));
    assert_eq!(fs::read(original.path()).unwrap(), b"original");
    assert_eq!(store.health().state(), HomeHealthState::Failed);
}

#[test]
fn source_errors_release_admission_and_allow_independent_work() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path(), &FaultController::new());
    let result = store.admit_sidecar_stream::<Cursor<&[u8]>, _, _>(
        namespace(),
        1,
        limit(),
        NonZeroUsize::new(7).unwrap(),
        || Err(io::Error::other("source cannot replay")),
        || false,
    );
    assert!(matches!(result, Err(SidecarError::Source { .. })));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    store
        .admit_sidecar(namespace(), b"independent", limit())
        .unwrap();
}

#[test]
fn retirement_refuses_streaming_work_before_source_acquisition() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path(), &FaultController::new());
    let retired = store.service_reference();
    store.close().unwrap();
    let result = retired.admit_sidecar_stream::<Cursor<&[u8]>, _, _>(
        namespace(),
        1,
        limit(),
        NonZeroUsize::new(7).unwrap(),
        || panic!("retired generation opened a source"),
        || false,
    );
    assert!(matches!(result, Err(SidecarError::HealthGate(_))));
}

#[test]
fn cancellation_before_publication_wins_and_after_publication_drains_token() {
    for (point, expected_cancel) in [
        (FaultPoint::BeforeSidecarFileSync, true),
        (FaultPoint::AfterSidecarRename, false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let store = open(directory.path(), &faults);
        let reference = store.service_reference();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let block = faults.block_next(point);
        let worker = thread::spawn(move || {
            reference.admit_sidecar_stream(
                namespace(),
                20,
                limit(),
                NonZeroUsize::new(7).unwrap(),
                || Ok(Cursor::new(b"publication boundary")),
                || worker_cancelled.load(Ordering::SeqCst),
            )
        });
        assert!(block.wait_until_reached(Duration::from_secs(10)));
        cancelled.store(true, Ordering::SeqCst);
        block.release();
        let result = worker.join().unwrap();
        if expected_cancel {
            assert!(matches!(result, Err(SidecarError::Cancelled)));
        } else {
            let token = result.unwrap();
            assert_eq!(fs::read(token.path()).unwrap(), b"publication boundary");
        }
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
        store.close().unwrap();
    }
}

#[test]
fn physical_fault_releases_stream_reader_and_requires_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(directory.path(), &faults);
    let live = Arc::new(AtomicUsize::new(0));
    faults.fail_next(FaultPoint::BeforeSidecarFileSync);
    let result = store.admit_sidecar_stream(
        namespace(),
        8,
        limit(),
        NonZeroUsize::new(4).unwrap(),
        || {
            live.fetch_add(1, Ordering::SeqCst);
            Ok(CheckedReader {
                cursor: Cursor::new(Arc::from(b"original".as_slice())),
                maximum: 4,
                live: Arc::clone(&live),
                reads: 0,
                cancel_on_read: None,
                cancelled: Arc::new(AtomicBool::new(false)),
            })
        },
        || false,
    );
    assert!(matches!(result, Err(SidecarError::Storage { .. })));
    assert_eq!(live.load(Ordering::SeqCst), 0);
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    let recovered = store.recover_same_home().unwrap().publish().unwrap();
    recovered
        .admit_sidecar(namespace(), b"original", limit())
        .unwrap();
}
