use super::*;
use crate::resource_structure::{
    construction::{
        ConstructionError, initial_build, validate_narrative_frontier, validate_selected,
        validate_source_manifest,
    },
    ledger::Ledger,
};

#[test]
fn source_admission_requires_exact_sealed_composer_reference() {
    let binding = binding();
    let ProjectionTextSource::Composer(content) = binding.source else {
        panic!()
    };
    let manifest = crate::ContentManifestRecord::new(
        content.id(),
        content.revision(),
        content.encoding(),
        crate::ContentLifecycle::Sealed,
        1,
        100,
        content.summary().digest(),
        content.summary(),
    );
    validate_source_manifest(&binding, &manifest).unwrap();
    for (id, revision, encoding, lifecycle) in [
        (
            SyndicContentId::from_bytes([9; 16]),
            content.revision(),
            content.encoding(),
            crate::ContentLifecycle::Sealed,
        ),
        (
            content.id(),
            ContentRevision::new(3).unwrap(),
            content.encoding(),
            crate::ContentLifecycle::Sealed,
        ),
        (
            content.id(),
            content.revision(),
            ContentEncoding::Utf8V1,
            crate::ContentLifecycle::Sealed,
        ),
        (
            content.id(),
            content.revision(),
            content.encoding(),
            crate::ContentLifecycle::Building,
        ),
    ] {
        let wrong = crate::ContentManifestRecord::new(
            id,
            revision,
            encoding,
            lifecycle,
            1,
            100,
            content.summary().digest(),
            content.summary(),
        );
        assert!(validate_source_manifest(&binding, &wrong).is_err());
    }
}

#[test]
fn provider_admission_keeps_retained_frontier_when_live_manifest_advances() {
    let mut binding = binding();
    let content_id = SyndicContentId::from_bytes([0x33; 16]);
    let generation = crate::ProviderNarrativeGeneration::FIRST;
    let span = crate::ProviderNarrativeSpanRecord::new(
        content_id,
        generation,
        0,
        100,
        crate::ProviderFrameOrdinalV1::FIRST,
        [0x12; 32],
        20,
        120,
        [0x13; 32],
        [0x14; 32],
    )
    .unwrap();
    let narrative = crate::ProviderNarrativeReference::new(
        content_id,
        generation,
        1,
        100,
        span.resulting_chain_digest(),
    )
    .unwrap();
    binding.source = ProjectionTextSource::ProviderNarrative(narrative);
    let manifest = crate::ContentManifestRecord::with_owner(
        content_id,
        Some(binding.metadata.item_id()),
        ContentRevision::new(8).unwrap(),
        ContentEncoding::ProviderItemV1,
        crate::ContentLifecycle::Live,
        2,
        200,
        SyndicContentDigest::from_bytes([0x44; 32]),
        match super::binding().source {
            ProjectionTextSource::Composer(content) => content.summary(),
            _ => panic!(),
        },
    );
    validate_source_manifest(&binding, &manifest).unwrap();
    let key = crate::codec::ProviderNarrativeSpanKey::first_for_generation(content_id, generation);
    validate_narrative_frontier(narrative, manifest.encoded_bytes(), &key, &span).unwrap();
    assert!(validate_narrative_frontier(narrative, 119, &key, &span).is_err());
    let wrong_key = crate::codec::ProviderNarrativeSpanKey::new(content_id, generation, 1);
    assert!(validate_narrative_frontier(narrative, 200, &wrong_key, &span).is_err());
    let wrong_digest =
        crate::ProviderNarrativeReference::new(content_id, generation, 1, 100, [0; 32]).unwrap();
    assert!(validate_narrative_frontier(wrong_digest, 200, &key, &span).is_err());
    let wrong_end = crate::ProviderNarrativeReference::new(
        content_id,
        generation,
        1,
        99,
        span.resulting_chain_digest(),
    )
    .unwrap();
    assert!(validate_narrative_frontier(wrong_end, 200, &key, &span).is_err());
}

#[test]
fn selected_companion_requires_exact_terminal_build_receipt_and_metadata_facts() {
    let mut build = build();
    build.revision = 4;
    build.transition = 3;
    build.lifecycle = Lifecycle::Completed;
    build.next_page = 2;
    build.chain = [0x99; 32];
    build.counts = [2, 0, 0, 0];
    build.checkpoint.pass = Pass::Sealed;
    build.checkpoint.phase = ParsePhase::Eof;
    build.checkpoint.next_byte = 10;
    let mut root = root();
    root.binding_digest = build.binding_digest;
    root.digest = root_digest(&root).unwrap();
    build.progress = Some(Reference {
        key: root.progress,
        digest: [0; 32],
    });
    let mut receipt = Progress {
        key: root.progress,
        command: Command::Complete,
        prior: Some(Reference {
            key: SequenceKey {
                build: build.key,
                ordinal: 2,
            },
            digest: [0x21; 32],
        }),
        before_revision: 3,
        after_revision: 4,
        before_digest: [0x22; 32],
        after_digest: build_digest(&build).unwrap(),
        page: None,
        root: Some(root.digest),
        digest: [0; 32],
    };
    receipt.digest = progress_digest(&receipt).unwrap();
    build.progress.as_mut().unwrap().digest = receipt.digest;
    let mut head = Head {
        key: build.key.resource,
        binding_digest: build.binding_digest,
        build: build.key,
        root: root.digest,
        digest: [0; 32],
    };
    head.digest = head_digest(&head).unwrap();
    validate_selected(&build.binding, &head, &root, &build, &receipt).unwrap();
    let mut wrong = root.clone();
    wrong.language = Some("other".into());
    wrong.digest = root_digest(&wrong).unwrap();
    let mut wrong_head = head.clone();
    wrong_head.root = wrong.digest;
    assert!(validate_selected(&build.binding, &wrong_head, &wrong, &build, &receipt).is_err());
    let mut wrong_build = build.clone();
    wrong_build.lifecycle = Lifecycle::Cancelled;
    assert!(validate_selected(&build.binding, &head, &root, &wrong_build, &receipt).is_err());
    let mut wrong_receipt = receipt.clone();
    wrong_receipt.after_digest = [0; 32];
    assert!(validate_selected(&build.binding, &head, &root, &build, &wrong_receipt).is_err());
    wrong_head = head.clone();
    wrong_head.build.operation = [0x91; 16];
    assert!(validate_selected(&build.binding, &wrong_head, &root, &build, &receipt).is_err());
}

fn metadata(structure: ResourceStructure, kind: ResourceKind) -> ResourceMetadataRecord {
    let original = binding();
    ResourceMetadataRecord::new(
        key().resource.id,
        key().resource.revision,
        original.metadata.projection_id().unwrap(),
        original.metadata.item_id(),
        ResourceOrdinal::FIRST,
        kind,
        "text/plain",
        original.metadata.backing(),
        [0x77; 32],
        None,
        structure,
    )
    .unwrap()
}

#[test]
fn admission_seeds_exact_genesis_without_output_or_receipt() {
    let initial = initial_build(key(), binding().metadata).unwrap();
    assert_eq!(initial, build());
    assert_eq!(
        decode_build(&encode_build(&initial).unwrap()).unwrap(),
        initial
    );
    let table = metadata(
        ResourceStructure::table(1, 2, ProjectionSourceRange::new(0, 4).unwrap()).unwrap(),
        ResourceKind::Table,
    );
    let initial = initial_build(key(), table).unwrap();
    assert_eq!(initial.checkpoint.pass, Pass::TableRows);
    assert_eq!(initial.checkpoint.next_byte, 0);
    assert_eq!(initial.counts, [0; 4]);
    assert!(initial.progress.is_none());
    assert_eq!(initial.next_page, 1);
    assert_eq!(
        initial.chain,
        empty_chain(initial.key, &initial.binding_digest)
    );
    assert_eq!(
        decode_build(&encode_build(&initial).unwrap()).unwrap(),
        initial
    );
}

#[test]
fn admission_refuses_wrong_identity_and_noncovering_table_header() {
    let mut wrong = key();
    wrong.operation = [0x91; 16];
    assert_ne!(
        initial_build(wrong, binding().metadata.clone())
            .unwrap()
            .chain,
        build().chain
    );
    wrong.resource.revision = ProjectionRevision::new(8).unwrap();
    assert!(matches!(
        initial_build(wrong, binding().metadata),
        Err(ConstructionError::Collision)
    ));
    for (start, end) in [(1, 4), (0, 10)] {
        let table = metadata(
            ResourceStructure::table(1, 2, ProjectionSourceRange::new(start, end).unwrap())
                .unwrap(),
            ResourceKind::Table,
        );
        assert!(matches!(
            initial_build(key(), table),
            Err(ConstructionError::Unavailable)
        ));
    }
}

#[test]
fn admission_preserves_maximum_supported_language_in_exact_binding() {
    let language = "r".repeat(256);
    let original = metadata(
        ResourceStructure::code(Some(&language), 2).unwrap(),
        ResourceKind::Code,
    );
    let before = crate::codec::ResourcesFamily::encode_value(&original).unwrap();
    let admitted = initial_build(key(), original.clone()).unwrap();
    assert!(before.len() <= 4_096);
    assert!(binding_bytes(&admitted.binding).unwrap().len() <= 4_608);
    assert_eq!(admitted.binding.metadata, original);
    assert_eq!(
        crate::codec::ResourcesFamily::encode_value(&admitted.binding.metadata).unwrap(),
        before
    );
}

#[test]
fn ledger_reserves_all_reads_and_refuses_atomically_at_each_ceiling() {
    let mut ledger = Ledger::default();
    ledger.reserve_read(520, 524_288, 524_288).unwrap();
    let full = ledger;
    for request in [(1, 0, 0), (0, 1, 0), (0, 0, 1), (usize::MAX, 0, 0)] {
        assert!(matches!(
            ledger.reserve_read(request.0, request.1, request.2),
            Err(ConstructionError::Limit)
        ));
        assert_eq!(ledger, full);
    }
    let mut fragmented = Ledger::default();
    for _ in 0..520 {
        fragmented.reserve_read(1, 512, 512).unwrap();
    }
    let before = fragmented;
    assert!(matches!(
        fragmented.reserve_read(1, 512, 512),
        Err(ConstructionError::Limit)
    ));
    assert_eq!(fragmented, before);
}

#[test]
fn ledger_enforces_normal_and_completion_write_envelopes() {
    let mut ledger = Ledger::default();
    for _ in 0..515 {
        ledger.reserve_write(1, 1, false).unwrap();
    }
    let before = ledger;
    assert!(matches!(
        ledger.reserve_write(1, 1, false),
        Err(ConstructionError::Limit)
    ));
    assert_eq!(ledger, before);
    ledger.reserve_write(40, 4_608, true).unwrap();
    ledger.reserve_write(24, 168, true).unwrap();
    let before = ledger;
    assert!(matches!(
        ledger.reserve_write(1, 1, true),
        Err(ConstructionError::Limit)
    ));
    assert_eq!(ledger, before);
    let mut byte_bound = Ledger::default();
    byte_bound.reserve_write(40, 262_104, false).unwrap();
    let before = byte_bound;
    assert!(matches!(
        byte_bound.reserve_write(0, 1, false),
        Err(ConstructionError::Limit)
    ));
    assert_eq!(byte_bound, before);
    assert!(matches!(
        byte_bound.reserve_write(usize::MAX, 1, false),
        Err(ConstructionError::Limit)
    ));
    assert_eq!(byte_bound, before);
}
