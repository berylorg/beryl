use beryl_home_store::{
    CursorReadLimits, HomeCandidateRecoveryAccess, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, ReadError,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::SyndicProjectionId;
use syndic_storage::{
    SyndicPointReadLimit, SyndicReadError, SyndicStorage, TranscriptGeneration, TranscriptPosition,
    TranscriptViewEntryRecord, test_faults::FixtureRecord,
};

use crate::support::{
    TestHome, batch, commit, id, open, populated::source_projection, seed_populated,
};

fn entry_at(
    source: &TranscriptViewEntryRecord,
    thread: beryl_model::SyndicThreadId,
    generation: TranscriptGeneration,
    position: u64,
) -> TranscriptViewEntryRecord {
    TranscriptViewEntryRecord::new(
        thread,
        generation,
        TranscriptPosition::new(position).unwrap(),
        source.item_id(),
        source.item_revision(),
        source.item_projection_generation(),
        source.projection_id(),
        source.projection_revision(),
    )
}

fn assert_pages(
    storage: &SyndicStorage,
    access: &HomeCandidateRecoveryAccess<'_>,
    expected: &[TranscriptViewEntryRecord],
) {
    let thread = expected[0].thread_id();
    let generation = expected[0].generation();
    let limits = CursorReadLimits::new(2, 65_536).unwrap();
    let first = storage
        .transcript_entries_candidate(access, thread, generation, None, limits)
        .unwrap();
    assert_eq!(first.records(), &expected[..2]);
    assert!(first.has_more());
    assert!(first.stored_bytes() > 0);
    assert!(first.decoded_bytes() > 0);
    let tail = storage
        .transcript_entries_candidate(
            access,
            thread,
            generation,
            Some(expected[1].position()),
            limits,
        )
        .unwrap();
    assert_eq!(tail.records(), &expected[2..]);
    assert!(!tail.has_more());
    for (thread, generation, after) in [
        (thread, generation, Some(expected[2].position())),
        (id(250), generation, None),
        (thread, TranscriptGeneration::new(999).unwrap(), None),
    ] {
        let empty = storage
            .transcript_entries_candidate(access, thread, generation, after, limits)
            .unwrap();
        assert!(empty.records().is_empty());
        assert!(!empty.has_more());
    }
    let one = storage
        .transcript_entries_candidate(
            access,
            thread,
            generation,
            None,
            CursorReadLimits::new(1, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(one.records(), &expected[..1]);
    assert!(one.has_more());
    assert_eq!(
        storage
            .transcript_entries_candidate(
                access,
                thread,
                generation,
                None,
                CursorReadLimits::new(2, one.stored_bytes()).unwrap(),
            )
            .unwrap(),
        one
    );
    assert!(matches!(
        storage.transcript_entries_candidate(
            access,
            thread,
            generation,
            None,
            CursorReadLimits::new(1, one.stored_bytes() - 1).unwrap(),
        ),
        Err(SyndicReadError::Read(_))
    ));
}

#[test]
fn candidate_transcript_pages_preserve_nonempty_bounds_cursor_and_generation_authority() {
    let home = TestHome::new("candidate-transcript-pages");
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_populated(&store, storage.clone());
    let head = storage
        .transcript_view_head(&store, id(30), SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap();
    assert!(head.entry_count() > 0);
    let generation = head.generation();
    let limits = CursorReadLimits::new(2, 65_536).unwrap();
    let source = storage
        .transcript_entries(&store, id(30), generation, None, limits)
        .unwrap()
        .records()
        .first()
        .expect("the published populated transcript head must have an entry")
        .clone();
    let expected: Vec<_> = (1..=3)
        .map(|position| entry_at(&source, id(30), generation, position))
        .collect();
    let neighbors = [
        entry_at(&source, id(29), generation, 1),
        entry_at(&source, id(31), generation, 1),
        entry_at(&source, id(30), generation.checked_next().unwrap(), 1),
    ];
    commit(
        &store,
        storage.clone(),
        batch(
            expected
                .iter()
                .cloned()
                .chain(neighbors)
                .map(FixtureRecord::TranscriptViewEntry),
        ),
    );
    let ordinary = storage
        .transcript_entries(&store, id(30), generation, None, limits)
        .unwrap();
    store.close().unwrap();
    let foreign_home = TestHome::new("candidate-transcript-foreign");
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
    assert_pages(&fresh, &access, &expected);
    assert_eq!(
        fresh
            .transcript_entries_candidate(&access, id(30), generation, None, limits)
            .unwrap(),
        ordinary
    );
    for invalid in [&storage, &foreign_storage] {
        assert!(matches!(
            invalid.transcript_entries_candidate(&access, id(30), generation, None, limits),
            Err(SyndicReadError::Read(ReadError::ForeignDomain { .. }))
        ));
    }
    beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
        assert!(
            fresh
                .transcript_entries(store, id(30), generation, None, limits)
                .is_err()
        );
    });
    let store = publication.publish().unwrap();
    assert_eq!(
        fresh
            .transcript_entries(&store, id(30), generation, None, limits)
            .unwrap(),
        ordinary
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let latest = SyndicStorage::reacquire_candidate(&recovered).unwrap();
    let access = recovered.recovery_access().unwrap();
    assert!(matches!(
        fresh.transcript_entries_candidate(&access, id(30), generation, None, limits),
        Err(SyndicReadError::Read(ReadError::ForeignDomain { .. }))
    ));
    assert_pages(&latest, &access, &expected);
    let store = recovered.publish().unwrap();
    assert_eq!(
        latest
            .transcript_entries(&store, id(30), generation, None, limits)
            .unwrap(),
        ordinary
    );
    store.close().unwrap();
    foreign.close().unwrap();
}

#[test]
fn candidate_projection_preserves_exact_content_absence_limits_and_fresh_handle_fences() {
    let home = TestHome::new("candidate-projection");
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_populated(&store, storage.clone());
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    let expected = storage
        .projection(&store, source_projection(), limit)
        .unwrap();
    assert!(expected.is_some());
    store.close().unwrap();
    let foreign_home = TestHome::new("candidate-projection-foreign");
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
    assert_eq!(
        fresh
            .projection_candidate(&access, source_projection(), limit)
            .unwrap(),
        expected
    );
    assert_eq!(
        fresh
            .projection_candidate(&access, SyndicProjectionId::from_bytes([250; 16]), limit)
            .unwrap(),
        None
    );
    assert!(matches!(
        fresh.projection_candidate(
            &access,
            source_projection(),
            SyndicPointReadLimit::new(1).unwrap()
        ),
        Err(SyndicReadError::Read(_))
    ));
    for invalid in [&storage, &foreign_storage] {
        assert!(matches!(
            invalid.projection_candidate(&access, source_projection(), limit),
            Err(SyndicReadError::Read(ReadError::ForeignDomain { .. }))
        ));
    }
    beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
        assert!(fresh.projection(store, source_projection(), limit).is_err());
    });
    let store = publication.publish().unwrap();
    assert_eq!(
        fresh
            .projection(&store, source_projection(), limit)
            .unwrap(),
        expected
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let latest = SyndicStorage::reacquire_candidate(&recovered).unwrap();
    let access = recovered.recovery_access().unwrap();
    assert!(matches!(
        fresh.projection_candidate(&access, source_projection(), limit),
        Err(SyndicReadError::Read(ReadError::ForeignDomain { .. }))
    ));
    assert_eq!(
        latest
            .projection_candidate(&access, source_projection(), limit)
            .unwrap(),
        expected
    );
    let store = recovered.publish().unwrap();
    assert_eq!(
        latest
            .projection(&store, source_projection(), limit)
            .unwrap(),
        expected
    );
    store.close().unwrap();
    foreign.close().unwrap();
}
