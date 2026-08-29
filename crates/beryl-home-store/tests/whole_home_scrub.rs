mod support;

use std::{
    convert::Infallible,
    sync::{
        Arc, Condvar, LazyLock, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use beryl_home_store::{
    DomainReader, DomainSchemaVersion, KeyspaceSchemaVersion, RecordFamily, StorageDomain,
    WholeHomeScrubTrigger,
};
use tempfile::tempdir;

use support::BytesRecord;

static VALIDATIONS: AtomicUsize = AtomicUsize::new(0);
static FIRST_RUN: LazyLock<(Mutex<bool>, Condvar)> =
    LazyLock::new(|| (Mutex::new(false), Condvar::new()));
static SERIAL: Mutex<()> = Mutex::new(());

struct ScrubDomain;

impl StorageDomain for ScrubDomain {
    const NAME: &'static str = "scrub-flight";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<BytesRecord<Self>>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        if VALIDATIONS.fetch_add(1, Ordering::SeqCst) == 0 {
            let (released, changed) = &*FIRST_RUN;
            let mut released = released.lock().unwrap();
            changed.notify_all();
            while !*released {
                released = changed.wait(released).unwrap();
            }
        }
        Ok(())
    }
}

#[test]
fn requests_join_and_corruption_evidence_coalesces_one_released_rerun() {
    let _serial = SERIAL.lock().unwrap();
    VALIDATIONS.store(0, Ordering::SeqCst);
    *FIRST_RUN.0.lock().unwrap() = false;
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    store.register_domain::<ScrubDomain>().unwrap();
    let store = Arc::new(store);

    let leader_store = Arc::clone(&store);
    let leader =
        thread::spawn(move || leader_store.scrub_whole_home(WholeHomeScrubTrigger::Explicit));
    let mut released = FIRST_RUN.0.lock().unwrap();
    while VALIDATIONS.load(Ordering::SeqCst) == 0 {
        released = FIRST_RUN.1.wait(released).unwrap();
    }
    drop(released);

    let triggers = [
        WholeHomeScrubTrigger::Background,
        WholeHomeScrubTrigger::CorruptionEvidence,
        WholeHomeScrubTrigger::CorruptionEvidence,
    ];
    let joiners: Vec<_> = triggers
        .into_iter()
        .map(|trigger| {
            let store = Arc::clone(&store);
            thread::spawn(move || store.scrub_whole_home(trigger))
        })
        .collect();
    for _ in 0..100_000 {
        if store.scrub_test_snapshot().joined == 3 {
            break;
        }
        thread::yield_now();
    }
    let active = store.scrub_test_snapshot();
    assert_eq!(active.joined, 3);
    assert_eq!(active.coalesced_reruns, 1);

    *FIRST_RUN.0.lock().unwrap() = true;
    FIRST_RUN.1.notify_all();
    leader.join().unwrap().unwrap();
    for joiner in joiners {
        joiner.join().unwrap().unwrap();
    }
    assert_eq!(VALIDATIONS.load(Ordering::SeqCst), 2);
    assert!(!store.scrub_test_snapshot().active);

    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(VALIDATIONS.load(Ordering::SeqCst), 3);
    assert_eq!(store.scrub_test_snapshot().worker_runs, 3);
}

#[test]
fn corruption_request_at_terminal_decision_is_not_lost() {
    let _serial = SERIAL.lock().unwrap();
    VALIDATIONS.store(0, Ordering::SeqCst);
    *FIRST_RUN.0.lock().unwrap() = true;
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    store.register_domain::<ScrubDomain>().unwrap();
    let store = Arc::new(store);
    let terminal = store.block_next_scrub_terminal_decision();

    let leader_store = Arc::clone(&store);
    let leader =
        thread::spawn(move || leader_store.scrub_whole_home(WholeHomeScrubTrigger::Explicit));
    assert!(terminal.wait_until_reached(Duration::from_secs(10)));
    assert_eq!(VALIDATIONS.load(Ordering::SeqCst), 1);

    let corruption_store = Arc::clone(&store);
    let corruption = thread::spawn(move || {
        corruption_store.scrub_whole_home(WholeHomeScrubTrigger::CorruptionEvidence)
    });
    for _ in 0..100_000 {
        if store.scrub_test_requests_entered() == 2 {
            break;
        }
        thread::yield_now();
    }
    assert_eq!(store.scrub_test_requests_entered(), 2);

    terminal.release();
    leader.join().unwrap().unwrap();
    corruption.join().unwrap().unwrap();

    assert_eq!(VALIDATIONS.load(Ordering::SeqCst), 2);
    let completed = store.scrub_test_snapshot();
    assert!(!completed.active);
    assert_eq!(completed.joined, 0);
    assert_eq!(completed.coalesced_reruns, 0);
    assert_eq!(completed.worker_runs, 2);
}
