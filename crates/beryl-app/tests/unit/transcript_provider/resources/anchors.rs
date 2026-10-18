use super::*;

#[path = "anchor_sources.rs"]
mod sources;

fn source_anchor(metadata: &ResourceMetadataRecord, position: u64) -> index::SourceAnchor {
    let ResourceBacking::TextRange { source, range } = metadata.backing() else {
        panic!("text source")
    };
    index::SourceAnchor {
        source,
        block_start: range.start(),
        kind: metadata.kind(),
        position,
        visual_offset: -17,
    }
}

fn deliver_anchor(
    fixture: &Fixture,
    core: &mut ResidentTranscriptCore,
    metadata: &ResourceMetadataRecord,
    anchor: index::SourceAnchor,
) -> Arc<IndexedResourceResponse> {
    let cancel = AtomicBool::new(false);
    let mut request = fixture.resource_request(2);
    let owner = core.resource_demand().unwrap();
    assert!(core.set_indexed_resource_demand(
        &owner,
        Some(IndexedResourceDemand {
            revision: metadata.revision(),
            root: None,
            query: IndexedResourceQuery::Anchor {
                anchor,
                bytes: 8192
            },
        })
    ));
    request.demand = core.resource_demand().unwrap();
    let before = core.core_snapshot();
    let prepared = fixture
        .reader
        .prepare_resource_delivery(request.clone(), &cancel)
        .unwrap();
    let ResourceDeliveryResponse::Indexed(response) = prepared.response() else {
        panic!("indexed")
    };
    let candidate = core
        .prepare_indexed_resource_response(&request.demand, response)
        .unwrap();
    fixture
        .reader
        .publish_resource_delivery_if_current(&prepared, &request, &cancel, || ())
        .unwrap();
    drop(prepared);
    *core = candidate;
    assert_eq!(
        core.core_snapshot().resident.visible_range,
        before.resident.visible_range
    );
    assert_eq!(
        core.presentation_snapshot().activation_revision,
        before.presentation.activation_revision
    );
    let response = core
        .indexed_resource_response(&request.demand.resource_id)
        .unwrap();
    assert_eq!(
        core.core_snapshot().resident.resource_bytes,
        response.encoded_bytes
    );
    assert!(response.encoded_bytes <= 8192);
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    response
}

#[test]
fn selected_anchor_crosses_resource_revision_with_only_the_old_source_position() {
    let fixture = Fixture::new();
    let previous = fixture.seed_resource("one\ntwo\n", 0, 4, None, 7);
    complete(&fixture, &previous);
    let mut core = resident(&fixture, ResidentTranscriptPolicy::default());
    let response = deliver_anchor(&fixture, &mut core, &previous, source_anchor(&previous, 2));
    let IndexedResourceValue::Anchor(value) = &response.value else {
        panic!("anchor")
    };
    let old = value.resolved.as_ref().unwrap().anchor;
    let old_root = response.root.as_ref().unwrap().digest;
    let weak = Arc::downgrade(&response);
    drop(response);
    core = core.empty_successor();
    assert_eq!(core.core_snapshot().resident.resource_bytes, 0);
    assert!(weak.upgrade().is_none());
    let successor = fixture.seed_structured_resource(
        "one\ntwo\n",
        0,
        8,
        None,
        8,
        ResourceStructure::code(None, 2).unwrap(),
    );
    complete(&fixture, &successor);
    core = resident(&fixture, ResidentTranscriptPolicy::default());
    let response = deliver_anchor(&fixture, &mut core, &successor, old);
    assert_ne!(response.root.as_ref().unwrap().digest, old_root);
    let IndexedResourceValue::Anchor(value) = &response.value else {
        panic!("anchor")
    };
    assert_eq!(value.binding.metadata, successor);
    assert_eq!(value.resolved.as_ref().unwrap().anchor, old);
    assert!(matches!(
        value.resolved.as_ref().unwrap().descriptor.descriptor,
        index::Descriptor::CodeLine { ordinal: 0, .. }
    ));
}

#[test]
fn selected_table_anchor_delimiter_gap_keeps_leading_tie_and_visual_offset() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_structured_resource(
        "a|b\n---|---:\nc|d\n",
        0,
        17,
        None,
        7,
        ResourceStructure::table(1, 2, ProjectionSourceRange::new(0, 4).unwrap()).unwrap(),
    );
    complete(&fixture, &metadata);
    let mut core = resident(&fixture, ResidentTranscriptPolicy::default());
    let response = deliver_anchor(&fixture, &mut core, &metadata, source_anchor(&metadata, 8));
    let IndexedResourceValue::Anchor(value) = &response.value else {
        panic!("anchor")
    };
    let resolved = value.resolved.as_ref().unwrap();
    assert_eq!(resolved.anchor.position, 3);
    assert_eq!(resolved.anchor.visual_offset, -17);
    assert_eq!(
        resolved.descriptor.absolute_raw,
        Some(index::ByteRange { start: 2, end: 3 })
    );
}

#[test]
fn selected_anchor_invalidates_replaced_source_block_or_kind_without_losing_exact_binding() {
    for change in 0..3 {
        let fixture = Fixture::new();
        let previous = fixture.seed_resource("old\n", 0, 4, None, 7);
        let mut old = source_anchor(&previous, 2);
        let metadata = fixture.seed_resource("new\n", 0, 4, None, 8);
        match change {
            0 => {}
            1 => {
                old = source_anchor(&metadata, 2);
                old.block_start = 1;
            }
            _ => {
                old = source_anchor(&metadata, 2);
                old.kind = syndic_storage::ResourceKind::Table;
            }
        }
        complete(&fixture, &metadata);
        let mut core = resident(&fixture, ResidentTranscriptPolicy::default());
        let response = deliver_anchor(&fixture, &mut core, &metadata, old);
        let IndexedResourceValue::Anchor(value) = &response.value else {
            panic!("anchor")
        };
        assert_eq!(value.binding.metadata, metadata);
        assert!(value.resolved.is_none());
        let weak = Arc::downgrade(&response);
        drop(response);
        core = core.empty_successor();
        assert_eq!(core.core_snapshot().resident.resource_bytes, 0);
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn selected_append_successor_anchor_retains_position_and_rejects_different_generation() {
    let fixture = Fixture::new();
    fixture.seed_resource("one\n", 0, 4, None, 7);
    let (previous, target) = sources::append(&fixture, None, "one\n", 7, 1);
    complete(&fixture, &previous);
    let mut core = resident(&fixture, ResidentTranscriptPolicy::default());
    let response = deliver_anchor(&fixture, &mut core, &previous, source_anchor(&previous, 2));
    let IndexedResourceValue::Anchor(value) = &response.value else {
        panic!("anchor")
    };
    let old = value.resolved.as_ref().unwrap().anchor;
    let weak = Arc::downgrade(&response);
    drop(response);
    core = core.empty_successor();
    assert_eq!(core.core_snapshot().resident.resource_bytes, 0);
    assert!(weak.upgrade().is_none());
    let (successor, _) = sources::append(&fixture, Some(target), "two\n", 8, 2);
    complete(&fixture, &successor);
    core = resident(&fixture, ResidentTranscriptPolicy::default());
    let response = deliver_anchor(&fixture, &mut core, &successor, old);
    let IndexedResourceValue::Anchor(value) = &response.value else {
        panic!("anchor")
    };
    let resolved = value.resolved.as_ref().unwrap();
    assert_eq!(value.binding.metadata, successor);
    assert_eq!(resolved.anchor.position, old.position);
    assert_eq!(resolved.anchor.visual_offset, old.visual_offset);
    assert_ne!(resolved.anchor.source, old.source);
    assert!(old.source.can_extend(resolved.anchor.source));
    drop(response);
    core = resident(&fixture, ResidentTranscriptPolicy::default());
    let ProjectionTextSource::ProviderNarrative(narrative) = old.source else {
        panic!("provider")
    };
    let replaced = index::SourceAnchor {
        source: ProjectionTextSource::provider_narrative(
            syndic_storage::ProviderNarrativeReference::new(
                narrative.content_id(),
                syndic_storage::ProviderNarrativeGeneration::new(2).unwrap(),
                narrative.span_count(),
                narrative.logical_utf8_bytes(),
                narrative.chain_digest(),
            )
            .unwrap(),
        ),
        ..old
    };
    let response = deliver_anchor(&fixture, &mut core, &successor, replaced);
    let IndexedResourceValue::Anchor(value) = &response.value else {
        panic!("anchor")
    };
    assert!(value.resolved.is_none());
    assert_eq!(value.binding.metadata, successor);
}
