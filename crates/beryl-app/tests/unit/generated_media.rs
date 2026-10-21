use super::{header::{GeneratedPngHeader, HeaderError}, scheduler::{Scheduler, Work}};
use std::io::{Cursor, Read};

fn png_header(width: u32, height: u32, depth: u8, colour: u8, interlace: u8) -> [u8; 33] {
    let mut bytes = [0; 33];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    bytes[8..12].copy_from_slice(&13u32.to_be_bytes());
    bytes[12..16].copy_from_slice(b"IHDR");
    bytes[16..20].copy_from_slice(&width.to_be_bytes());
    bytes[20..24].copy_from_slice(&height.to_be_bytes());
    bytes[24] = depth;
    bytes[25] = colour;
    bytes[28] = interlace;
    let crc = crc32fast::hash(&bytes[12..29]).to_be_bytes();
    bytes[29..].copy_from_slice(&crc);
    bytes
}

#[test]
fn generated_original_header_accepts_wide_interlaced_sixteen_bit_originals() {
    let bytes = png_header(0x7fff_ffff, 0x7fff_ffff, 16, 6, 1);
    let header = GeneratedPngHeader::read(Cursor::new(bytes)).unwrap();
    assert_eq!(header.dimensions(), [0x7fff_ffff, 0x7fff_ffff]);
    let mut reader = header.bind(Cursor::new(bytes));
    let mut actual = Vec::new();
    reader.read_to_end(&mut actual).unwrap();
    assert_eq!(actual, bytes);
}

#[test]
fn generated_original_every_cursor_checks_returned_header_from_first_digest_byte() {
    let bytes = png_header(512, 1, 8, 6, 0);
    let header = GeneratedPngHeader::read(Cursor::new(bytes)).unwrap();
    for position in 0..33 {
        let mut changed = bytes;
        changed[position] ^= 1;
        for quantum in [1, 7, 33, 64] {
            let mut reader = header.bind(Cursor::new(changed));
            let mut output = [0; 64];
            let failed = loop {
                match reader.read(&mut output[..quantum]) {
                    Err(_) => break true,
                    Ok(0) => break false,
                    Ok(_) => {}
                }
            };
            assert!(failed, "position={position}, quantum={quantum}");
            assert!(reader.read(&mut output).is_err());
        }
    }
}

#[test]
fn generated_original_short_returned_header_refuses_every_physical_eof() {
    let bytes = png_header(1, 1, 8, 6, 0);
    let header = GeneratedPngHeader::read(Cursor::new(bytes)).unwrap();
    for length in 0..33 {
        let mut reader = header.bind(Cursor::new(&bytes[..length]));
        assert!(reader.read_to_end(&mut Vec::new()).is_err());
    }
}

#[test]
fn generated_original_header_rejects_illegal_fields_and_crc() {
    for (width, height, depth, colour, interlace) in [
        (0, 1, 8, 6, 0), (1, 0, 8, 6, 0), (0x8000_0000, 1, 8, 6, 0),
        (1, 1, 4, 6, 0), (1, 1, 16, 3, 0), (1, 1, 8, 1, 0), (1, 1, 8, 6, 2),
    ] {
        assert!(matches!(GeneratedPngHeader::read(Cursor::new(png_header(width, height, depth, colour, interlace))), Err(HeaderError::Malformed)));
    }
    let mut bytes = png_header(1, 1, 8, 6, 0);
    bytes[29] ^= 1;
    assert!(matches!(GeneratedPngHeader::read(Cursor::new(bytes)), Err(HeaderError::Malformed)));
    assert!(matches!(GeneratedPngHeader::read(Cursor::new(*b"GIF89axx")), Err(HeaderError::Unsupported)));
}

fn discover(scheduler: &mut Scheduler<u64, u64>) -> (Option<u64>, usize) {
    match scheduler.next() {
        Work::Discover { sweep, available } => (sweep, available),
        _ => panic!("expected discovery"),
    }
}
fn resource(scheduler: &mut Scheduler<u64, u64>) -> u64 {
    match scheduler.next() {
        Work::Resource(resource) => resource,
        _ => panic!("expected resource"),
    }
}

#[test]
fn generated_scheduler_preserves_finite_continuation_under_repeated_notifications() {
    let mut scheduler = Scheduler::new(2, 1);
    assert_eq!(discover(&mut scheduler), (None, 2));
    scheduler.notify();
    scheduler.discovered([1, 2], Some(2));
    assert_eq!(resource(&mut scheduler), 1);
    assert_eq!(discover(&mut scheduler), (Some(2), 1));
    scheduler.notify();
    scheduler.discovered([3], Some(3));
    scheduler.finished(1);
    assert_eq!(resource(&mut scheduler), 2);
    assert_eq!(discover(&mut scheduler), (Some(3), 1));
    scheduler.discovered([4], None);
    scheduler.finished(2);
    assert_eq!(resource(&mut scheduler), 3);
    assert_eq!(discover(&mut scheduler), (None, 1));
}

#[test]
fn generated_scheduler_bounds_queue_workers_and_joins_same_identity() {
    let mut scheduler = Scheduler::new(2, 2);
    discover(&mut scheduler);
    scheduler.discovered([1, 2], Some(2));
    assert_eq!(resource(&mut scheduler), 1);
    assert_eq!(resource(&mut scheduler), 2);
    assert_eq!(discover(&mut scheduler), (Some(2), 2));
    scheduler.discovered([1, 2], Some(4));
    assert_eq!(scheduler.counts(), (0, 2));
    assert_eq!(discover(&mut scheduler), (Some(4), 2));
    scheduler.discovered([3, 4], Some(6));
    assert!(matches!(scheduler.next(), Work::Wait));
    assert_eq!(scheduler.counts(), (2, 2));
    scheduler.finished(1);
    assert_eq!(resource(&mut scheduler), 3);
    assert_eq!(scheduler.counts(), (1, 2));
}

#[test]
fn generated_scheduler_idle_pending_peer_does_not_restart_a_completed_sweep() {
    let mut scheduler = Scheduler::new(1, 1);
    discover(&mut scheduler);
    scheduler.discovered([1], Some(1));
    assert_eq!(resource(&mut scheduler), 1);
    scheduler.finished(1);
    assert_eq!(discover(&mut scheduler), (Some(1), 1));
    scheduler.discovered([2], None);
    assert_eq!(resource(&mut scheduler), 2);
    scheduler.finished(2);
    assert!(matches!(scheduler.next(), Work::Wait));
    scheduler.notify();
    assert_eq!(discover(&mut scheduler), (None, 1));
}

#[test]
fn generated_scheduler_notification_survives_peer_scan_skipping_active_pending_resource() {
    let mut scheduler = Scheduler::new(2, 2);
    discover(&mut scheduler);
    scheduler.discovered([1], None);
    assert_eq!(resource(&mut scheduler), 1);
    for _ in 0..8 { scheduler.notify(); }
    assert_eq!(discover(&mut scheduler), (None, 2));
    scheduler.discovered([1], None);
    assert_eq!(scheduler.counts(), (0, 1));
    assert!(matches!(scheduler.next(), Work::Wait));
    scheduler.finished(1);
    assert_eq!(discover(&mut scheduler), (None, 2));
    scheduler.discovered([1], None);
    assert_eq!(resource(&mut scheduler), 1);
    scheduler.finished(1);
    assert_eq!(scheduler.counts(), (0, 0));
    for _ in 0..8 { assert!(matches!(scheduler.next(), Work::Wait)); }
}

#[test]
fn generated_scheduler_active_skip_without_notification_creates_no_self_wake() {
    let mut scheduler = Scheduler::new(2, 2);
    discover(&mut scheduler);
    scheduler.discovered([1], Some(1));
    assert_eq!(resource(&mut scheduler), 1);
    assert_eq!(discover(&mut scheduler), (Some(1), 2));
    scheduler.discovered([1], None);
    scheduler.finished(1);
    for _ in 0..8 { assert!(matches!(scheduler.next(), Work::Wait)); }
    scheduler.notify();
    assert_eq!(discover(&mut scheduler), (None, 2));
    scheduler.discovered([1], None);
    assert_eq!(resource(&mut scheduler), 1);
    scheduler.notify();
    scheduler.stop();
    scheduler.finished(1);
    scheduler.notify();
    assert_eq!(scheduler.counts(), (0, 0));
    assert!(matches!(scheduler.next(), Work::Stop));
}

#[test]
fn generated_scheduler_stop_discards_queued_and_discovery_but_retains_active_drain() {
    let mut scheduler = Scheduler::new(2, 1);
    discover(&mut scheduler);
    scheduler.discovered([1, 2], Some(2));
    assert_eq!(resource(&mut scheduler), 1);
    discover(&mut scheduler);
    scheduler.stop();
    scheduler.discovered([3], Some(3));
    assert_eq!(scheduler.counts(), (0, 1));
    assert!(matches!(scheduler.next(), Work::Stop));
    scheduler.finished(1);
    assert_eq!(scheduler.counts(), (0, 0));
}

#[test]
fn generated_scheduler_saturated_waiting_batch_reaches_later_ready_peer() {
    let mut scheduler = Scheduler::new(2, 1);
    let mut cursor = 0u64;
    let mut ready_seen = false;
    loop {
        match scheduler.next() {
            Work::Discover { sweep, available } => {
                let after = sweep.unwrap_or(0);
                assert_eq!(after, cursor);
                assert!(available <= 2);
                let end = (after + available as u64).min(17);
                // The pending producer entries are checked and skipped during bounded discovery.
                scheduler.discovered((after + 1..=end).filter(|id| *id == 17), (end < 17).then_some(end));
                cursor = end;
                scheduler.notify();
            }
            Work::Resource(17) => { ready_seen = true; scheduler.finished(17); break; }
            _ => panic!("ready peer must be reached within the original finite sweep"),
        }
    }
    assert!(ready_seen);
    assert_eq!(cursor, 17);
    assert_eq!(scheduler.counts(), (0, 0));
}

#[test]
fn generated_backend_busy_and_cleanup_never_claim_source_loss() {
    use beryl_backend::GeneratedOutputError as Error;
    use beryl_model::GeneratedMediaUnavailableReason as Reason;
    for error in [Error::Busy, Error::Cancelled, Error::CleanupPending] {
        assert_eq!(super::source::classify(error), None);
    }
    assert_eq!(super::source::classify(Error::LostSourceAuthority), Some(Reason::LostSourceAuthority));
    assert_eq!(super::source::classify(Error::Missing), Some(Reason::Missing));
    assert_eq!(super::source::classify(Error::Changed), Some(Reason::Changed));
}

#[test]
fn generated_read_revision_drift_defers_without_retiring_workers_or_hiding_invalid_facts() {
    use super::coordinator::{GeneratedMediaCoordinatorError as Error, admission_read_failure, settle_resource, source_read_failure};
    let source = source_read_failure(syndic_storage::SyndicReadError::ConcurrentChange { operation: "generated-media source proof" });
    assert!(matches!(source, Error::Deferred));
    assert!(!settle_resource(Err(source)).unwrap());
    let assets = admission_read_failure(beryl_state::GeneratedMediaError::ConcurrentChange);
    assert!(!settle_resource(Err(assets)).unwrap());
    assert!(settle_resource(Err(admission_read_failure(beryl_state::GeneratedMediaError::Rejected))).is_err());
    assert!(settle_resource(Err(source_read_failure(syndic_storage::SyndicReadError::Invariant("malformed source evidence")))).is_err());
    assert!(settle_resource(Ok(true)).unwrap());
    let mut scheduler = Scheduler::new(2, 2);
    discover(&mut scheduler);
    scheduler.discovered([1, 2], Some(2));
    assert_eq!(resource(&mut scheduler), 1);
    scheduler.finished(1);
    assert_eq!(resource(&mut scheduler), 2);
    scheduler.finished(2);
    assert_eq!(scheduler.counts(), (0, 0));
}

#[test]
fn generated_discovery_revision_drift_retries_exact_sweep_without_resetting_high_water() {
    let mut scheduler = Scheduler::new(2, 1);
    discover(&mut scheduler);
    scheduler.discovered([], Some(19));
    assert_eq!(discover(&mut scheduler), (Some(19), 2));
    scheduler.notify();
    scheduler.discovery_deferred(Some(19));
    assert_eq!(discover(&mut scheduler), (Some(19), 2));
    scheduler.discovered([23], None);
    assert_eq!(resource(&mut scheduler), 23);
    scheduler.finished(23);
    assert_eq!(discover(&mut scheduler), (None, 2));
    scheduler.discovered([], None);
    assert!(matches!(scheduler.next(), Work::Wait));
}

#[test]
fn generated_home_first_digest_pass_refuses_header_changed_after_preflight() {
    use beryl_home_store::{HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, SidecarByteLimit, SidecarError, SidecarNamespace};
    use std::num::{NonZeroU64, NonZeroUsize};
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT)).unwrap();
    let state = beryl_state::BerylState::register(&mut candidate).unwrap();
    let home = candidate.prepare_publication(beryl_state::BerylState::required_domains().unwrap()).unwrap().publish().unwrap();
    let original = png_header(17, 19, 8, 6, 0);
    let header = GeneratedPngHeader::read(Cursor::new(original)).unwrap();
    let changed = png_header(23, 29, 8, 6, 0);
    let opens = std::cell::Cell::new(0);
    let result = home.admit_sidecar_stream(
        SidecarNamespace::new("images").unwrap(), 33, SidecarByteLimit::new(NonZeroU64::MAX), NonZeroUsize::new(13).unwrap(),
        || { opens.set(opens.get() + 1); Ok(header.bind(Cursor::new(changed))) }, || false,
    );
    assert!(matches!(result, Err(SidecarError::Source { .. })));
    assert_eq!(opens.get(), 1, "the first hashing cursor itself must refuse changed facts");
    drop(state);
    home.close().unwrap();
}

#[test]
fn generated_home_all_passes_use_fresh_zero_offset_header_bound_cursors() {
    use beryl_home_store::{HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, SidecarByteLimit, SidecarNamespace};
    use std::num::{NonZeroU64, NonZeroUsize};
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT)).unwrap();
    let state = beryl_state::BerylState::register(&mut candidate).unwrap();
    let home = candidate.prepare_publication(beryl_state::BerylState::required_domains().unwrap()).unwrap().publish().unwrap();
    let header_bytes = png_header(0x7fff_ffff, 0x7fff_ffff, 16, 6, 1);
    let header = GeneratedPngHeader::read(Cursor::new(header_bytes)).unwrap();
    let mut bytes = Vec::from(header_bytes);
    bytes.resize(100_000, 97);
    let opens = std::cell::Cell::new(0);
    for _ in 0..2 {
        let admitted = home.admit_sidecar_stream(
            SidecarNamespace::new("images").unwrap(), bytes.len() as u64,
            SidecarByteLimit::new(NonZeroU64::MAX), NonZeroUsize::new(4096).unwrap(),
            || { opens.set(opens.get() + 1); Ok(header.bind(Cursor::new(&bytes))) }, || false,
        ).unwrap();
        assert_eq!(admitted.address().length(), 100_000);
    }
    assert!(opens.get() >= 5, "digest, write/reuse and comparison passes obtain independent cursors");
    drop(state);
    home.close().unwrap();
}
