use super::*;

#[cfg(feature = "test-faults")]
mod source {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/model_reader/source.rs"
    ));
}

#[test]
fn concurrent_queries_and_pages_share_finite_capacity_and_release_independently() {
    let owner = Arc::new(CapacityOwner {
        queries: AtomicUsize::new(0),
        pages: AtomicUsize::new(0),
    });
    let query = owner.reserve(CapacityKind::Query, 1).unwrap();
    let page = owner.reserve(CapacityKind::Page, 1).unwrap();
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let owner = Arc::clone(&owner);
            scope.spawn(move || {
                assert!(matches!(
                    owner.reserve(CapacityKind::Query, 1),
                    Err(ModelReadError::Capacity)
                ));
                assert!(matches!(
                    owner.reserve(CapacityKind::Page, 1),
                    Err(ModelReadError::Capacity)
                ));
            });
        }
    });
    drop(query);
    assert!(owner.reserve(CapacityKind::Query, 1).is_ok());
    assert!(matches!(
        owner.reserve(CapacityKind::Page, 1),
        Err(ModelReadError::Capacity)
    ));
    drop(page);
    assert!(owner.reserve(CapacityKind::Page, 1).is_ok());
    assert_eq!(owner.queries.load(Ordering::Acquire), 0);
    assert_eq!(owner.pages.load(Ordering::Acquire), 0);
}

#[test]
fn duplicate_request_is_refused_until_original_cleanup_releases_it() {
    let busy = AtomicBool::new(false);
    let original = Request::begin(&busy).unwrap();
    assert!(matches!(
        Request::begin(&busy),
        Err(ModelReadError::Pending)
    ));
    drop(original);
    assert!(Request::begin(&busy).is_ok());
    assert!(!busy.load(Ordering::Acquire));
}

#[test]
fn model_limits_reject_zero_timeout_and_pages_above_backend_boundary() {
    let one = NonZeroUsize::new(1).unwrap();
    assert!(matches!(
        ModelReaderLimits::new(one, one, 64, Duration::ZERO),
        Err(ModelReadError::Limits)
    ));
    for records in [0, 65, u32::MAX] {
        assert!(matches!(
            ModelReaderLimits::new(one, one, records, Duration::from_secs(1)),
            Err(ModelReadError::Limits)
        ));
    }
    assert!(ModelReaderLimits::new(one, one, 64, Duration::from_secs(1)).is_ok());
}

#[test]
fn exact_defaults_keep_missing_reasoning_unknown_and_option_efforts_remain_recognized() {
    use beryl_backend::{
        BackendConfigDefaults, DefaultReasoningEffort, ModelDisplayName, ModelRecord,
        ProtocolIdentity, SupportedReasoningEfforts,
    };
    let defaults = ModelDefaults::from_backend(BackendConfigDefaults::new(
        Some(ProtocolIdentity::try_new("actual-model").unwrap()),
        None,
        true,
        true,
    ));
    assert_eq!(defaults.model.as_deref(), Some("actual-model"));
    assert_eq!(defaults.reasoning, None);
    let mut supported = SupportedReasoningEfforts::empty();
    supported.insert(beryl_backend::ReasoningEffort::Low);
    supported.insert(beryl_backend::ReasoningEffort::Ultra);
    assert!(!supported.insert_wire("unknown-provider-effort"));
    let option = ModelOptionRecord::from_backend(&ModelRecord::new(
        ProtocolIdentity::try_new("stable-id").unwrap(),
        ProtocolIdentity::try_new("wire-model").unwrap(),
        ModelDisplayName::try_new("Visible label").unwrap(),
        false,
        true,
        supported,
        DefaultReasoningEffort::Other,
    ));
    assert_eq!(option.id, "stable-id");
    assert_eq!(option.model, "wire-model");
    assert_eq!(option.label, "Visible label");
    assert_eq!(option.default_reasoning, None);
    for effort in ModelReasoningEffort::ALL {
        assert_eq!(
            option.efforts.contains(effort),
            supported.contains(effort.backend())
        );
    }
    assert_eq!(defaults.reasoning, None);
}
