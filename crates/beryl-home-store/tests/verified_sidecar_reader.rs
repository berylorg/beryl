#![cfg(feature = "test-faults")]

use std::{
    fs,
    io::{BufRead, Read, Seek, SeekFrom},
    num::{NonZeroU64, NonZeroUsize},
    path::Path,
    thread,
    time::Duration,
};

use beryl_home_store::{
    CommandCancellation, HomeDomainRequirements, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, SidecarAddress, SidecarByteLimit, SidecarError, SidecarNamespace,
    SidecarStage,
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

fn limit(bytes: u64) -> SidecarByteLimit {
    SidecarByteLimit::new(NonZeroU64::new(bytes).unwrap())
}

fn page(bytes: usize) -> NonZeroUsize {
    NonZeroUsize::new(bytes).unwrap()
}

fn seed(store: &HomeStore, bytes: &[u8]) -> SidecarAddress {
    store
        .admit_sidecar(
            SidecarNamespace::new("reader-test").unwrap(),
            bytes,
            limit(100_000),
        )
        .unwrap()
        .address()
        .clone()
}

#[test]
fn exact_limit_and_bounded_buffered_reads_preserve_seek_position() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path(), &FaultController::new());
    let bytes: Vec<u8> = (0..10_001).map(|value| (value % 251) as u8).collect();
    let address = seed(&store, &bytes);
    assert!(matches!(
        store.open_verified_sidecar_reader(
            &address,
            limit(bytes.len() as u64 - 1),
            page(7),
            CommandCancellation::new(),
        ),
        Err(SidecarError::BoundExceeded { .. })
    ));
    for size in [1, 7, 4096, 100_000] {
        let mut reader = store
            .open_verified_sidecar_reader(
                &address,
                limit(bytes.len() as u64),
                page(size),
                CommandCancellation::new(),
            )
            .unwrap();
        assert_eq!(reader.verified_sidecar().address(), &address);
        assert!(
            store
                .verified_sidecar_matches_current_scope(reader.verified_sidecar())
                .unwrap()
        );
        assert!(reader.fill_buf().unwrap().len() <= size);
        reader.consume(1);
        assert_eq!(reader.stream_position().unwrap(), 1);
        assert_eq!(reader.seek(SeekFrom::Current(3)).unwrap(), 4);
        let mut destination = [0; 8192];
        let count = reader.read(&mut destination).unwrap();
        assert!(count <= size);
        assert_eq!(&destination[..count], &bytes[4..4 + count]);
        reader.seek(SeekFrom::End(-3)).unwrap();
        let mut tail = Vec::new();
        reader.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, bytes[bytes.len() - 3..]);
        reader.rewind().unwrap();
        let mut whole = Vec::new();
        reader.read_to_end(&mut whole).unwrap();
        assert_eq!(whole, bytes);
    }
}

#[test]
fn cancellation_is_observed_before_open_during_verification_and_with_buffered_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(directory.path(), &faults);
    let address = seed(&store, b"verified source");
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        store.open_verified_sidecar_reader(&address, limit(100), page(2), cancellation),
        Err(SidecarError::Cancelled)
    ));
    let cancellation = CommandCancellation::new();
    let block = faults.block_next(FaultPoint::BeforeSidecarVerification);
    thread::scope(|scope| {
        let handle = scope.spawn(|| {
            store.open_verified_sidecar_reader(&address, limit(100), page(2), cancellation.clone())
        });
        assert!(block.wait_until_reached(Duration::from_secs(10)));
        cancellation.cancel();
        block.release();
        assert!(matches!(
            handle.join().unwrap(),
            Err(SidecarError::Cancelled)
        ));
    });
    let cancellation = CommandCancellation::new();
    let mut reader = store
        .open_verified_sidecar_reader(&address, limit(100), page(7), cancellation.clone())
        .unwrap();
    assert_eq!(reader.fill_buf().unwrap(), b"verifie");
    reader.consume(1);
    cancellation.cancel();
    for error in [
        reader.fill_buf().unwrap_err(),
        reader.read(&mut [0; 1]).unwrap_err(),
        reader.rewind().unwrap_err(),
    ] {
        assert!(matches!(
            error.get_ref().unwrap().downcast_ref::<SidecarError>(),
            Some(SidecarError::Cancelled)
        ));
    }
}

#[test]
fn digest_length_missing_and_physical_trailing_bytes_fail_closed() {
    for replacement in [
        None,
        Some(&b"changedd"[..]),
        Some(&b"original-extra"[..]),
        Some(&b"short"[..]),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let store = open(directory.path(), &FaultController::new());
        let address = seed(&store, b"original");
        let verified = store.verify_sidecar(&address, limit(100)).unwrap();
        match replacement {
            Some(bytes) => fs::write(verified.path(), bytes).unwrap(),
            None => fs::remove_file(verified.path()).unwrap(),
        }
        let result = store.open_verified_sidecar_reader(
            &address,
            limit(100),
            page(1),
            CommandCancellation::new(),
        );
        match replacement {
            Some(_) => assert!(matches!(result, Err(SidecarError::ContentMismatch))),
            None => assert!(matches!(result, Err(SidecarError::Missing))),
        }
    }
}

#[test]
fn verified_reader_keeps_the_open_file_and_scope_without_path_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let store = open(directory.path(), &FaultController::new());
    let foreign = open(foreign_directory.path(), &FaultController::new());
    let address = seed(&store, b"original");
    seed(&foreign, b"original");
    let mut reader = store
        .open_verified_sidecar_reader(&address, limit(100), page(2), CommandCancellation::new())
        .unwrap();
    assert!(
        !foreign
            .verified_sidecar_matches_current_scope(reader.verified_sidecar())
            .unwrap()
    );
    let path = reader.verified_sidecar().path().to_path_buf();
    #[cfg(windows)]
    assert_eq!(fs::remove_file(&path).unwrap_err().raw_os_error(), Some(32));
    #[cfg(not(windows))]
    {
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"replaced").unwrap();
    }
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"original");
    let error = reader.seek(SeekFrom::Current(-100)).unwrap_err();
    assert!(matches!(
        error.get_ref().unwrap().downcast_ref::<SidecarError>(),
        Some(SidecarError::Storage {
            stage: SidecarStage::SeekFinal,
            ..
        })
    ));
    store.close().unwrap();
    let reopened = open(directory.path(), &FaultController::new());
    assert!(
        !reopened
            .verified_sidecar_matches_current_scope(reader.verified_sidecar())
            .unwrap()
    );
    drop(reader);
    fs::remove_file(path).unwrap();
    assert!(matches!(
        reopened.open_verified_sidecar_reader(
            &address,
            limit(100),
            page(2),
            CommandCancellation::new()
        ),
        Err(SidecarError::Missing)
    ));
}
