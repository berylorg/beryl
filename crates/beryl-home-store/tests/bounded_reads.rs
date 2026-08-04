mod support;

use beryl_home_store::{
    CommandError, CursorDirection, CursorRange, CursorReadLimits, HomeCommand, HomeHealthState,
    MutationBuildError, PointReadLimit, ReadError,
};
use tempfile::tempdir;

use support::{AlphaDomain, BytesRecord, BytesRecordV2, FixtureMutationError, PutBytes, open_home};

#[test]
fn typed_point_and_cursor_reads_return_only_decoded_records() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    for (key, value) in [
        (1, b"one".to_vec()),
        (2, b"two".to_vec()),
        (3, b"three".to_vec()),
    ] {
        put(&store, alpha, key, value);
    }

    assert_eq!(
        store
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                alpha,
                &2,
                PointReadLimit::new(64).unwrap(),
            )
            .unwrap(),
        Some(b"two".to_vec())
    );

    let page = store
        .read_cursor::<AlphaDomain, BytesRecord<AlphaDomain>>(
            alpha,
            &CursorRange::closed(1, 3),
            CursorDirection::Forward,
            CursorReadLimits::new(2, 128).unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 2);
    assert_eq!(page.records()[0].key(), &1);
    assert_eq!(page.records()[1].value(), b"two");
    assert!(page.has_more());
    assert!(page.stored_bytes() <= 128);
    assert!(page.decoded_bytes() <= 128);
    assert!(page.decoded_bytes() > 0);
    assert_eq!(store.health().state(), HomeHealthState::Healthy);

    let reverse = store
        .read_cursor::<AlphaDomain, BytesRecord<AlphaDomain>>(
            alpha,
            &CursorRange::closed(1, 3),
            CursorDirection::Reverse,
            CursorReadLimits::new(3, 128).unwrap(),
        )
        .unwrap();
    assert_eq!(
        reverse
            .records()
            .iter()
            .map(|record| *record.key())
            .collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
    assert!(!reverse.has_more());
}

#[test]
fn point_and_cursor_materialization_obey_explicit_byte_bounds() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    put(&store, alpha, 1, vec![7; 32]);

    assert!(matches!(
        store.read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            alpha,
            &1,
            PointReadLimit::new(16).unwrap(),
        ),
        Err(ReadError::BoundExceeded {
            maximum: 16,
            actual: 36,
            ..
        })
    ));
    assert!(matches!(
        store.read_cursor::<AlphaDomain, BytesRecord<AlphaDomain>>(
            alpha,
            &CursorRange::closed(1, 1),
            CursorDirection::Forward,
            CursorReadLimits::new(1, 24).unwrap(),
        ),
        Err(ReadError::BoundExceeded { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert_eq!(
        store
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                alpha,
                &1,
                PointReadLimit::new(64).unwrap(),
            )
            .unwrap(),
        Some(vec![7; 32])
    );
}

#[test]
fn reversed_cursor_range_and_non_owning_record_codec_are_typed() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    assert!(matches!(
        store.read_cursor::<AlphaDomain, BytesRecord<AlphaDomain>>(
            alpha,
            &CursorRange::closed(9, 1),
            CursorDirection::Forward,
            CursorReadLimits::new(4, 128).unwrap(),
        ),
        Err(ReadError::ReversedRange {
            domain: "alpha",
            family: "records"
        })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain, BytesRecordV2<AlphaDomain>>::new(1, b"v2".to_vec()),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        Err(CommandError::ContributorAssembly {
            domain: "alpha",
            source,
        }) if matches!(
            source.downcast_ref::<FixtureMutationError>(),
            Some(FixtureMutationError::Build(
                MutationBuildError::CodecTypeMismatch {
                    domain: "alpha",
                    family: "records",
                }
            ))
        )
    ));
}

fn put(
    store: &beryl_home_store::HomeStore,
    domain: beryl_home_store::DomainHandle<AlphaDomain>,
    key: u64,
    value: Vec<u8>,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<AlphaDomain>::new(key, value),
        ))
        .unwrap();
    store.execute(command).unwrap();
}
