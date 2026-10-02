use beryl_home_store::{
    TemporaryReadError, TemporaryReadPool, TemporaryReadPoolLimits, TemporaryReadPoolUsage,
};

#[test]
fn reservations_admit_exact_bytes_and_counts_until_last_clone_releases() {
    let pool = TemporaryReadPool::new(TemporaryReadPoolLimits::new(8, 1, 4).unwrap());
    let mut writer = pool.begin(8).unwrap();
    assert!(matches!(pool.begin(0), Err(TemporaryReadError::Capacity)));
    assert_eq!(
        pool.usage().unwrap(),
        TemporaryReadPoolUsage {
            reserved_bytes: 8,
            sources: 1
        }
    );
    writer.write(0, b"abcd").unwrap();
    writer.write(4, b"efgh").unwrap();
    let reader = writer.seal().unwrap();
    let clone = reader.clone();
    drop(reader);
    assert_eq!(clone.read(2, 4).unwrap(), b"cdef");
    assert!(matches!(pool.begin(1), Err(TemporaryReadError::Capacity)));
    drop(clone);
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
    assert!(matches!(pool.begin(9), Err(TemporaryReadError::Capacity)));
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
}

#[test]
fn chunks_must_be_contiguous_and_sealing_requires_complete_bytes() {
    let pool = TemporaryReadPool::new(TemporaryReadPoolLimits::new(20, 2, 4).unwrap());
    let mut writer = pool.begin(8).unwrap();
    assert!(matches!(
        writer.write(4, b"abcd"),
        Err(TemporaryReadError::Range)
    ));
    assert!(matches!(
        writer.write(0, b"abcde"),
        Err(TemporaryReadError::Range)
    ));
    assert!(matches!(
        writer.write(u64::MAX, b"ab"),
        Err(TemporaryReadError::Range)
    ));
    writer.write(0, b"abcd").unwrap();
    assert!(matches!(
        writer.write(0, b"abcd"),
        Err(TemporaryReadError::Range)
    ));
    assert!(matches!(writer.seal(), Err(TemporaryReadError::Range)));
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
    let mut writer = pool.begin(4).unwrap();
    writer.write(0, b"abcd").unwrap();
    let reader = writer.seal().unwrap();
    assert!(matches!(reader.read(1, 4), Err(TemporaryReadError::Range)));
    assert!(matches!(reader.read(0, 5), Err(TemporaryReadError::Range)));
    assert!(matches!(
        reader.read(u64::MAX, 1),
        Err(TemporaryReadError::Range)
    ));
    assert_eq!(reader.read(4, 0).unwrap(), b"");
    drop(reader);
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
    drop(pool.begin(4).unwrap());
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
}

#[test]
fn empty_artifact_and_invalid_configuration_are_explicit() {
    assert!(TemporaryReadPoolLimits::new(0, 1, 1).is_err());
    assert!(TemporaryReadPoolLimits::new(1, 0, 1).is_err());
    assert!(TemporaryReadPoolLimits::new(1, 257, 1).is_err());
    assert!(TemporaryReadPoolLimits::new(1, 1, 0).is_err());
    let pool = TemporaryReadPool::new(TemporaryReadPoolLimits::new(1, 1, 1).unwrap());
    let reader = pool.begin(0).unwrap().seal().unwrap();
    assert!(reader.is_empty());
    assert!(reader.read(0, 0).unwrap().is_empty());
    drop(reader);
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
}

#[cfg(feature = "test-faults")]
#[test]
fn io_failures_never_publish_partial_sources_and_reads_can_fail_without_release() {
    use beryl_home_store::TemporaryReadFault;
    let pool = TemporaryReadPool::new(TemporaryReadPoolLimits::new(8, 1, 4).unwrap());
    pool.test_fail_next(TemporaryReadFault::Create);
    assert!(matches!(pool.begin(4), Err(TemporaryReadError::Io { .. })));
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
    let mut writer = pool.begin(4).unwrap();
    pool.test_fail_next(TemporaryReadFault::Write);
    assert!(matches!(
        writer.write(0, b"abcd"),
        Err(TemporaryReadError::Io { .. })
    ));
    assert!(writer.write(0, b"abcd").is_err());
    assert!(writer.seal().is_err());
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
    let mut writer = pool.begin(4).unwrap();
    writer.write(0, b"abcd").unwrap();
    let reader = writer.seal().unwrap();
    for fault in [TemporaryReadFault::Read, TemporaryReadFault::ShortRead] {
        pool.test_fail_next(fault);
        assert!(matches!(
            reader.read(0, 4),
            Err(TemporaryReadError::Io { .. })
        ));
        assert_eq!(reader.read(0, 4).unwrap(), b"abcd");
    }
    drop(reader);
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
}
