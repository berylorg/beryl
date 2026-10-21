#![cfg(all(feature = "test-faults", target_os = "windows"))]

#[path = "generated_output_sources/support.rs"]
mod support;
#[path = "generated_output_sources/wsl.rs"]
mod wsl;

use beryl_home_store::{HomeHealthState, SidecarByteLimit, SidecarError, SidecarNamespace};
use std::{
    cell::Cell,
    fs,
    io::{Read, Write},
    num::{NonZeroU64, NonZeroUsize},
    sync::atomic::Ordering,
};
use support::Fixture;

struct CancellingReader<'a> {
    cursor: beryl_backend::GeneratedOutputCursor<'a>,
    cancellation: &'a std::sync::atomic::AtomicBool,
    cancel_after_read: bool,
}

impl Read for CancellingReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let length = self.cursor.read(bytes)?;
        if self.cancel_after_read && length != 0 {
            self.cancellation.store(true, Ordering::Release);
        }
        Ok(length)
    }
}

fn namespace() -> SidecarNamespace {
    SidecarNamespace::new("images").unwrap()
}
fn limit() -> SidecarByteLimit {
    SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap())
}

#[test]
fn original_file_cursors_support_home_admission_and_exact_reuse() {
    let mut fixture = Fixture::new();
    let bytes: Vec<u8> = (0..100_001).map(|index| (index % 251) as u8).collect();
    fs::write(fixture.path(), &bytes).unwrap();
    let source = fixture.open();
    let mut first = source.cursor();
    let mut second = source.cursor();
    let mut a = [0; 13];
    let mut b = [0; 7];
    first.read_exact(&mut a).unwrap();
    second.read_exact(&mut b).unwrap();
    assert_eq!(a, bytes[..13]);
    assert_eq!(b, bytes[..7]);
    drop((first, second));
    let passes = Cell::new(0);
    let mut factory = || {
        passes.set(passes.get() + 1);
        Ok(source.cursor())
    };
    let admitted = fixture
        .home
        .admit_sidecar_stream(
            namespace(),
            source.length(),
            limit(),
            NonZeroUsize::new(257).unwrap(),
            &mut factory,
            || false,
        )
        .unwrap();
    assert_eq!(passes.get(), 3);
    assert_eq!(fs::read(admitted.path()).unwrap(), bytes);
    let reused = fixture
        .home
        .admit_sidecar_stream(
            namespace(),
            source.length(),
            limit(),
            NonZeroUsize::new(8192).unwrap(),
            &mut factory,
            || false,
        )
        .unwrap();
    assert_eq!(passes.get(), 5);
    assert_eq!(reused.address(), admitted.address());
    assert_eq!(fs::read(reused.path()).unwrap(), bytes);
    drop(factory);
    assert!(source.close().is_ok());
}

#[test]
fn source_mutation_between_digest_and_write_is_refused() {
    let mut fixture = Fixture::new();
    fs::write(fixture.path(), [31; 8193]).unwrap();
    let source = fixture.open();
    let mut pass = 0;
    let result = fixture.home.admit_sidecar_stream(
        namespace(),
        source.length(),
        limit(),
        NonZeroUsize::new(257).unwrap(),
        || {
            pass += 1;
            if pass == 2 {
                fs::write(fixture.path(), [32; 8193])?;
            }
            Ok(source.cursor())
        },
        || false,
    );
    assert!(matches!(result, Err(SidecarError::ContentMismatch)));
    assert!(source.close().is_ok());
}

#[test]
fn physical_growth_and_shrinkage_fail_home_admission() {
    for grow in [false, true] {
        let mut fixture = Fixture::new();
        fs::write(fixture.path(), [41; 8192]).unwrap();
        let source = fixture.open();
        if grow {
            fs::OpenOptions::new()
                .append(true)
                .open(fixture.path())
                .unwrap()
                .write_all(&[42])
                .unwrap();
        } else {
            fs::OpenOptions::new()
                .write(true)
                .open(fixture.path())
                .unwrap()
                .set_len(8191)
                .unwrap();
        }
        let result = fixture.home.admit_sidecar_stream(
            namespace(),
            source.length(),
            limit(),
            NonZeroUsize::new(8192).unwrap(),
            || Ok(source.cursor()),
            || false,
        );
        assert!(matches!(result, Err(SidecarError::Source { .. })));
        assert_eq!(fixture.home.health().state(), HomeHealthState::Healthy);
        assert!(source.close().is_ok());
    }
}

#[test]
fn cancellation_refuses_admission_and_closes_original_source() {
    let mut fixture = Fixture::new();
    fs::write(fixture.path(), [51; 8193]).unwrap();
    let source = fixture.open();
    fixture.cancelled.store(true, Ordering::Release);
    let result = fixture.home.admit_sidecar_stream(
        namespace(),
        source.length(),
        limit(),
        NonZeroUsize::new(257).unwrap(),
        || Ok(source.cursor()),
        || fixture.cancelled.load(Ordering::Acquire),
    );
    assert!(matches!(result, Err(SidecarError::Cancelled)));
    assert!(source.cursor().read(&mut [0; 1]).is_err());
    assert!(source.close().is_ok());
}

#[test]
fn original_process_retirement_revokes_an_open_source() {
    let mut fixture = Fixture::new();
    fs::write(fixture.path(), [61; 100]).unwrap();
    let source = fixture.open();
    fixture.server.shutdown().unwrap();
    assert!(source.cursor().read(&mut [0; 1]).is_err());
    assert!(source.close().is_ok());
}

#[test]
fn path_replacement_cannot_replace_the_opened_source_between_passes() {
    let mut fixture = Fixture::new();
    let original = [71; 8193];
    fs::write(fixture.path(), original).unwrap();
    let source = fixture.open();
    let mut pass = 0;
    let admitted = fixture
        .home
        .admit_sidecar_stream(
            namespace(),
            source.length(),
            limit(),
            NonZeroUsize::new(257).unwrap(),
            || {
                pass += 1;
                if pass == 2 {
                    fs::rename(
                        fixture.path(),
                        fixture.directory.path().join("original.bin"),
                    )?;
                    fs::write(fixture.path(), [72; 8193])?;
                }
                Ok(source.cursor())
            },
            || false,
        )
        .unwrap();
    assert_eq!(fs::read(admitted.path()).unwrap(), original);
    assert_eq!(fs::read(fixture.path()).unwrap(), [72; 8193]);
    assert!(source.close().is_ok());
}

#[test]
fn cancellation_during_digest_or_write_releases_the_original_reader() {
    for cancelled_pass in [1, 2] {
        let mut fixture = Fixture::new();
        fs::write(fixture.path(), [81; 8193]).unwrap();
        let source = fixture.open();
        let mut pass = 0;
        let result = fixture.home.admit_sidecar_stream(
            namespace(),
            source.length(),
            limit(),
            NonZeroUsize::new(257).unwrap(),
            || {
                pass += 1;
                Ok(CancellingReader {
                    cursor: source.cursor(),
                    cancellation: &fixture.cancelled,
                    cancel_after_read: pass == cancelled_pass,
                })
            },
            || fixture.cancelled.load(Ordering::Acquire),
        );
        assert!(matches!(result, Err(SidecarError::Cancelled)));
        assert_eq!(fixture.home.health().state(), HomeHealthState::Healthy);
        assert!(source.close().is_ok());
    }
}

#[test]
fn public_source_authorization_refuses_foreign_and_retired_loaded_authority() {
    use beryl_backend::{ForegroundSessionConfig, GeneratedOutputError, GeneratedOutputTarget};
    use beryl_model::{CasItemId, CasTurnId};
    use std::time::Duration;

    let mut fixture = Fixture::new();
    let target = || {
        GeneratedOutputTarget::new(
            [19; 16],
            CasTurnId::new("authority-turn").unwrap(),
            CasItemId::new("authority-item").unwrap(),
        )
    };
    assert!(
        fixture
            .session
            .authorize_generated_output(&fixture.release, &fixture.loaded, target())
            .is_ok()
    );
    let root = fixture.directory.path().join("root");
    let mut other_session = fixture
        .server
        .client_connector()
        .connect_foreground_candidate(
            ForegroundSessionConfig::new(NonZeroUsize::new(8).unwrap()),
            Duration::from_secs(10),
        )
        .unwrap();
    other_session
        .initialize_foreground(Duration::from_secs(10))
        .unwrap();
    let other_release = other_session
        .admit_release(&root, Duration::from_secs(10))
        .unwrap();
    let other_loaded = other_session
        .start_thread(&root, Duration::from_secs(10))
        .unwrap()
        .into_loaded();
    assert_eq!(other_loaded.thread_id(), fixture.loaded.thread_id());
    assert_eq!(
        fixture
            .session
            .authorize_generated_output(&fixture.release, &other_loaded, target())
            .err(),
        Some(GeneratedOutputError::LostSourceAuthority)
    );
    assert_eq!(
        other_session
            .authorize_generated_output(&other_release, &fixture.loaded, target())
            .err(),
        Some(GeneratedOutputError::LostSourceAuthority)
    );
    {
        let foreign_process = Fixture::new();
        assert_eq!(
            fixture
                .session
                .authorize_generated_output(&foreign_process.release, &fixture.loaded, target())
                .err(),
            Some(GeneratedOutputError::LostSourceAuthority)
        );
    }
    let old_loaded = fixture.loaded.clone();
    fixture
        .session
        .unsubscribe_thread(old_loaded.thread_id(), Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        fixture
            .session
            .authorize_generated_output(&fixture.release, &old_loaded, target())
            .err(),
        Some(GeneratedOutputError::LostSourceAuthority)
    );
    other_session.shutdown().unwrap();
}
