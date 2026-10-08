#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, SyndicStorage, THREAD_DISCOVERY_PAGE_MAX_BYTES,
    THREAD_DISCOVERY_PAGE_MAX_ITEMS,
};

use support::{TestHome, draft_id, id, open, timestamp};

struct Fixture {
    store: HomeStore,
    storage: SyndicStorage,
    _home: TestHome,
}

impl Fixture {
    fn new() -> Self {
        let home = TestHome::new("thread-discovery");
        let mut candidate = open(home.path());
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        Self {
            store,
            storage,
            _home: home,
        }
    }

    fn create(&self, seed: u8) {
        let execution = ExecutionBinding::new(
            RuntimeId::from_bytes([1; 16]),
            RootId::from_bytes([2; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                r"C:\Work\Beryl".to_owned(),
            )
            .unwrap(),
        );
        let creation = CreateThread::ordinary(
            id(seed),
            draft_id(seed),
            execution,
            timestamp(u64::from(seed)),
            DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        );
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command
            .add(
                self.storage
                    .create_thread(self.storage.revision(&self.store).unwrap(), creation),
            )
            .unwrap();
        assert!(matches!(
            self.store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    }
}

fn limits(items: usize) -> CursorReadLimits {
    CursorReadLimits::new(items, THREAD_DISCOVERY_PAGE_MAX_BYTES).unwrap()
}

#[test]
fn empty_source_is_complete_and_consumes_no_bytes() {
    let fixture = Fixture::new();
    let page = fixture
        .storage
        .threads_page(&fixture.store, None, limits(1))
        .unwrap();
    assert!(page.records().is_empty());
    assert!(!page.has_more());
    assert_eq!(page.stored_bytes(), 0);
    assert_eq!(page.decoded_bytes(), 0);
}

#[test]
fn identity_order_and_exclusive_continuation_cover_both_boundary_identities() {
    let fixture = Fixture::new();
    for seed in [255, 7, 0, 3] {
        fixture.create(seed);
    }
    let first = fixture
        .storage
        .threads_page(&fixture.store, None, limits(2))
        .unwrap();
    assert_eq!(
        first
            .records()
            .iter()
            .map(|row| row.id())
            .collect::<Vec<_>>(),
        vec![id(0), id(3)]
    );
    assert!(first.has_more());
    assert!(first.stored_bytes() > 0);
    assert!(first.decoded_bytes() > 0);
    assert!(first.stored_bytes() <= THREAD_DISCOVERY_PAGE_MAX_BYTES);
    let second = fixture
        .storage
        .threads_page(&fixture.store, Some(id(3)), limits(2))
        .unwrap();
    assert_eq!(
        second
            .records()
            .iter()
            .map(|row| row.id())
            .collect::<Vec<_>>(),
        vec![id(7), id(255)]
    );
    assert!(!second.has_more());
    let terminal = fixture
        .storage
        .threads_page(&fixture.store, Some(id(255)), limits(2))
        .unwrap();
    assert!(terminal.records().is_empty());
    assert!(!terminal.has_more());
}

#[test]
fn oversized_requests_still_publish_only_the_bounded_source_page() {
    let fixture = Fixture::new();
    for seed in 0..=(THREAD_DISCOVERY_PAGE_MAX_ITEMS as u8) {
        fixture.create(seed);
    }
    let requested = CursorReadLimits::new(usize::MAX, usize::MAX).unwrap();
    let first = fixture
        .storage
        .threads_page(&fixture.store, None, requested)
        .unwrap();
    assert_eq!(first.records().len(), THREAD_DISCOVERY_PAGE_MAX_ITEMS);
    assert!(first.has_more());
    assert!(first.stored_bytes() <= THREAD_DISCOVERY_PAGE_MAX_BYTES);
    let second = fixture
        .storage
        .threads_page(
            &fixture.store,
            Some(first.records().last().unwrap().id()),
            requested,
        )
        .unwrap();
    assert_eq!(second.records().len(), 1);
    assert!(!second.has_more());
}

#[test]
fn insufficient_byte_budget_rejects_instead_of_reporting_empty_source() {
    let fixture = Fixture::new();
    fixture.create(1);
    assert!(
        fixture
            .storage
            .threads_page(&fixture.store, None, CursorReadLimits::new(1, 1).unwrap())
            .is_err()
    );
}
