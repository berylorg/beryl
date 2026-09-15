use beryl_home_store::{
    CursorReadLimits, HomeCandidateRecoveryAccess, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{ProjectionRevision, SyndicItemId, SyndicTurnId};
use syndic_storage::{
    SyndicReadError, SyndicStorage, TurnItemIndexRecord, TurnItemOrdinal,
    test_faults::FixtureRecord,
};

use crate::support::{TestHome, batch, commit, open};

fn assert_pages(
    storage: &SyndicStorage,
    access: &HomeCandidateRecoveryAccess<'_>,
    turn: SyndicTurnId,
    expected: &[TurnItemIndexRecord],
) {
    let limits = CursorReadLimits::new(2, 65_536).unwrap();
    let first = storage
        .turn_items_candidate(access, turn, None, limits)
        .unwrap();
    assert_eq!(first.records(), &expected[..2]);
    assert!(first.has_more());
    assert!(first.stored_bytes() > 0);
    assert!(first.decoded_bytes() > 0);
    let tail = storage
        .turn_items_candidate(access, turn, Some(expected[1].ordinal()), limits)
        .unwrap();
    assert_eq!(tail.records(), &expected[2..]);
    assert!(!tail.has_more());
    let empty = storage
        .turn_items_candidate(access, turn, Some(expected[2].ordinal()), limits)
        .unwrap();
    assert!(empty.records().is_empty());
    assert!(!empty.has_more());
    let absent = storage
        .turn_items_candidate(access, SyndicTurnId::from_bytes([250; 16]), None, limits)
        .unwrap();
    assert!(absent.records().is_empty());
    assert!(!absent.has_more());
    let one = storage
        .turn_items_candidate(
            access,
            turn,
            None,
            CursorReadLimits::new(1, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(one.records(), &expected[..1]);
    assert!(one.has_more());
    assert_eq!(
        storage
            .turn_items_candidate(
                access,
                turn,
                None,
                CursorReadLimits::new(1, one.stored_bytes()).unwrap()
            )
            .unwrap(),
        one
    );
    assert!(
        storage
            .turn_items_candidate(access, turn, None, CursorReadLimits::new(1, 1).unwrap())
            .is_err()
    );
}

#[test]
fn candidate_turn_item_pages_preserve_owner_bounds_continuation_and_publication_parity() {
    let home = TestHome::new("candidate-turn-items");
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let turn = SyndicTurnId::from_bytes([100; 16]);
    let expected: Vec<_> = (1..=3)
        .map(|ordinal| {
            TurnItemIndexRecord::new(
                turn,
                TurnItemOrdinal::new(ordinal).unwrap(),
                SyndicItemId::from_bytes([ordinal as u8; 16]),
                ProjectionRevision::new(1).unwrap(),
            )
        })
        .collect();
    let neighbors = [99, 101].map(|id| {
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            SyndicTurnId::from_bytes([id; 16]),
            TurnItemOrdinal::FIRST,
            SyndicItemId::from_bytes([id; 16]),
            ProjectionRevision::new(1).unwrap(),
        ))
    });
    commit(
        &store,
        storage.clone(),
        batch(
            expected
                .iter()
                .cloned()
                .map(FixtureRecord::TurnItem)
                .chain(neighbors),
        ),
    );
    let limits = CursorReadLimits::new(2, 65_536).unwrap();
    let ordinary = storage.turn_items(&store, turn, None, limits).unwrap();
    store.close().unwrap();
    let foreign_home = TestHome::new("candidate-turn-items-foreign");
    let mut foreign = open(foreign_home.path());
    let foreign_storage = SyndicStorage::register(&mut foreign).unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let fresh = SyndicStorage::register(&mut candidate).unwrap();
    let mut publication = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let access = publication.recovery_access().unwrap();
    assert_pages(&fresh, &access, turn, &expected);
    assert_eq!(
        fresh
            .turn_items_candidate(&access, turn, None, limits)
            .unwrap(),
        ordinary
    );
    for invalid in [&storage, &foreign_storage] {
        assert!(matches!(
            invalid.turn_items_candidate(&access, turn, None, limits),
            Err(SyndicReadError::Read(_))
        ));
    }
    beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
        assert!(fresh.turn_items(store, turn, None, limits).is_err());
    });
    let store = publication.publish().unwrap();
    assert_eq!(
        fresh.turn_items(&store, turn, None, limits).unwrap(),
        ordinary
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let latest = SyndicStorage::reacquire_candidate(&recovered).unwrap();
    let access = recovered.recovery_access().unwrap();
    assert!(matches!(
        fresh.turn_items_candidate(&access, turn, None, limits),
        Err(SyndicReadError::Read(_))
    ));
    assert_pages(&latest, &access, turn, &expected);
    let store = recovered.publish().unwrap();
    assert_eq!(
        latest.turn_items(&store, turn, None, limits).unwrap(),
        ordinary
    );
    store.close().unwrap();
    foreign.close().unwrap();
}
