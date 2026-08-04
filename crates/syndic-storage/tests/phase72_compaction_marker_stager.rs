use beryl_model::CasItemId;
use syndic_storage::{
    CompactionMarkerLifecycle, ProviderCompactionMarkerStager,
    ProviderCompactionMarkerStagingError, ProviderField, ProviderObservationBegin,
    ProviderObservationControl, ProviderObservationItemKind, ProviderObservationItemLifecycle,
    ProviderObservationStagingBytes, ProviderScalar, ProviderValueContext, SyndicTimestamp,
};

fn begin(lifecycle: ProviderObservationItemLifecycle) -> ProviderCompactionMarkerStager {
    ProviderCompactionMarkerStager::begin(ProviderObservationBegin::Item {
        lifecycle,
        kind: ProviderObservationItemKind::ContextCompaction,
    })
    .unwrap()
}

fn stage_marker(
    lifecycle: ProviderObservationItemLifecycle,
    observed_at: u64,
    identity_fragments: &[&[u8]],
) -> syndic_storage::ProviderCompactionMarker {
    let mut marker = begin(lifecycle);
    marker
        .control(ProviderObservationControl::Scalar {
            context: ProviderValueContext::Field(ProviderField::LifecycleObservedAt),
            value: ProviderScalar::Unsigned(observed_at),
        })
        .unwrap();
    let identity = ProviderValueContext::Field(ProviderField::ItemId);
    marker
        .control(ProviderObservationControl::BeginField(identity))
        .unwrap();
    for fragment in identity_fragments {
        marker
            .fragment(ProviderObservationStagingBytes::new(identity, fragment).unwrap())
            .unwrap();
    }
    marker
        .control(ProviderObservationControl::EndField(identity))
        .unwrap();
    marker.seal().unwrap()
}

#[test]
fn resident_marker_parser_preserves_exact_lifecycle_identity_and_timestamp() {
    let started = stage_marker(
        ProviderObservationItemLifecycle::Started,
        72_001,
        &[b"compaction-", b"marker"],
    );
    assert_eq!(
        started.item_id(),
        &CasItemId::new("compaction-marker").unwrap()
    );
    assert_eq!(started.lifecycle(), CompactionMarkerLifecycle::Started);
    assert_eq!(
        started.observed_at(),
        SyndicTimestamp::from_unix_millis(72_001)
    );

    let completed = stage_marker(
        ProviderObservationItemLifecycle::Completed,
        72_002,
        &[b"compaction-marker"],
    );
    assert_eq!(completed.lifecycle(), CompactionMarkerLifecycle::Completed);
    assert_eq!(
        completed.observed_at(),
        SyndicTimestamp::from_unix_millis(72_002)
    );
}

#[test]
fn resident_marker_parser_rejects_other_kinds_and_incomplete_schema() {
    assert!(matches!(
        ProviderCompactionMarkerStager::begin(ProviderObservationBegin::Item {
            lifecycle: ProviderObservationItemLifecycle::Started,
            kind: ProviderObservationItemKind::AgentMessage,
        }),
        Err(ProviderCompactionMarkerStagingError::ItemKindMismatch)
    ));

    let mut marker = begin(ProviderObservationItemLifecycle::Started);
    let identity = ProviderValueContext::Field(ProviderField::ItemId);
    marker
        .control(ProviderObservationControl::BeginField(identity))
        .unwrap();
    marker
        .fragment(ProviderObservationStagingBytes::new(identity, b"marker").unwrap())
        .unwrap();
    marker
        .control(ProviderObservationControl::EndField(identity))
        .unwrap();
    assert!(matches!(
        marker.seal(),
        Err(ProviderCompactionMarkerStagingError::Validation(_))
    ));
}

#[test]
fn resident_marker_parser_rejects_identity_beyond_fixed_bound() {
    let mut marker = begin(ProviderObservationItemLifecycle::Started);
    marker
        .control(ProviderObservationControl::Scalar {
            context: ProviderValueContext::Field(ProviderField::LifecycleObservedAt),
            value: ProviderScalar::Unsigned(72_003),
        })
        .unwrap();
    let identity = ProviderValueContext::Field(ProviderField::ItemId);
    marker
        .control(ProviderObservationControl::BeginField(identity))
        .unwrap();
    let maximum = [b'x'; 256];
    marker
        .fragment(ProviderObservationStagingBytes::new(identity, &maximum).unwrap())
        .unwrap();
    let overflow = ProviderObservationStagingBytes::new(identity, b"x").unwrap();
    assert!(matches!(
        marker.fragment(overflow),
        Err(ProviderCompactionMarkerStagingError::Validation(_))
    ));
}
