#![cfg(feature = "test-faults")]

use beryl_model::{
    CasItemId, CasThreadId, CasTurnId, ContentRevision, SyndicContentDigest, SyndicContentId,
    SyndicItemId, SyndicTurnId,
};
use sha2::{Digest, Sha256};
use syndic_storage::{
    CasItemSource, CasTurnSource, ContentEncoding, ContentReference, ContentSummary,
    ProviderFrameHistorySupportV1, ProviderFrameObservationSummaryV1, ProviderFrameOrdinalV1,
    ProviderFrameReferenceV1, ProviderFrameTextSpanV1, ProviderItemBuildLifecycle,
    ProviderItemBuildRecord, ProviderItemBuildRevision, ProviderItemKind,
    ProviderItemStreamStateV1, ProviderLifecycleTimestampMsV1, ProviderLogicalTextRoleV1,
    ProviderNarrativeComparisonFrontier, ProviderNarrativeCompletionCheck,
    ProviderNarrativeCompletionState, ProviderNarrativeGeneration, ProviderNarrativeReference,
    ProviderNarrativeSpanRecord, SealedProviderFrameReference, SourceEventSequence,
    advance_provider_narrative_chain, provider_narrative_chain_seed,
    test_faults::{
        PhysicalFamily, ProviderFixtureCorruption, ProviderFixtureFamily, ProviderFixtureRecord,
        decode_corrupted_provider_fixture, roundtrip_provider_fixture,
    },
};

const ENCODED_FRAME_BYTES: u64 = 64;

fn empty_marker_digest() -> [u8; 32] {
    beryl_model::content_marker_digest_seed()
}

fn summary(chunks: u64, encoded: u64, digest: u8) -> ContentSummary {
    ContentSummary::new(
        chunks,
        0,
        encoded,
        0,
        0,
        0,
        empty_marker_digest(),
        None,
        SyndicContentDigest::from_bytes([digest; 32]),
    )
    .unwrap()
}

fn source(item_id: &str) -> CasItemSource {
    CasItemSource::new(
        CasTurnSource::new(
            CasThreadId::new("provider-thread").unwrap(),
            CasTurnId::new("provider-turn").unwrap(),
        ),
        CasItemId::new(item_id).unwrap(),
    )
}

fn narrative(
    content_id: SyndicContentId,
    generation: u64,
    spans: u64,
    bytes: u64,
    digest: u8,
) -> ProviderNarrativeReference {
    if spans == 0 {
        ProviderNarrativeReference::empty(
            content_id,
            ProviderNarrativeGeneration::new(generation).unwrap(),
        )
    } else {
        ProviderNarrativeReference::new(
            content_id,
            ProviderNarrativeGeneration::new(generation).unwrap(),
            spans,
            bytes,
            [digest; 32],
        )
        .unwrap()
    }
}

#[allow(clippy::too_many_arguments)]
fn sealed_frame(
    content_id: SyndicContentId,
    revision: u64,
    ordinal: u64,
    kind: ProviderItemKind,
    observation: ProviderFrameObservationSummaryV1,
    frame_spans: u64,
    frame_bytes: u64,
    selected_narrative: Option<ProviderNarrativeReference>,
) -> SealedProviderFrameReference {
    let encoded_end = ordinal * ENCODED_FRAME_BYTES;
    let content = ContentReference::new(
        content_id,
        ContentRevision::new(revision).unwrap(),
        ContentEncoding::ProviderItemV1,
        summary(revision, encoded_end, revision as u8),
    );
    let frame = ProviderFrameReferenceV1::new(
        CasItemId::new("provider-item").unwrap(),
        kind,
        ProviderFrameOrdinalV1::new(ordinal).unwrap(),
        encoded_end - ENCODED_FRAME_BYTES,
        encoded_end,
        [ordinal as u8; 32],
        frame_bytes,
        frame_spans,
    )
    .unwrap();
    let started_at = ProviderLifecycleTimestampMsV1::new(1);
    let (start, completed) = match observation {
        ProviderFrameObservationSummaryV1::Started(_) => (Some(started_at), false),
        ProviderFrameObservationSummaryV1::Delta => (Some(started_at), false),
        ProviderFrameObservationSummaryV1::Completed(_) if kind.permits_completion_only() => {
            (None, true)
        }
        ProviderFrameObservationSummaryV1::Completed(_) => (Some(started_at), true),
    };
    let stream_state = ProviderItemStreamStateV1::new(
        CasItemId::new("provider-item").unwrap(),
        kind,
        ordinal + 1,
        start,
        completed,
        ProviderFrameHistorySupportV1::Supported,
    )
    .unwrap();
    SealedProviderFrameReference::new(
        content,
        frame,
        observation,
        stream_state,
        selected_narrative,
    )
    .unwrap()
}

fn assert_narrative_rejected(
    target: &SealedProviderFrameReference,
    narrative: Option<ProviderNarrativeReference>,
) {
    assert!(
        SealedProviderFrameReference::new(
            target.content(),
            target.frame().clone(),
            target.observation(),
            target.stream_state().clone(),
            narrative,
        )
        .is_err()
    );
}

fn initial_build() -> ProviderItemBuildRecord {
    let content_id = SyndicContentId::from_bytes([1; 16]);
    let target_narrative = narrative(content_id, 1, 1, 4, 21);
    let target = sealed_frame(
        content_id,
        1,
        1,
        ProviderItemKind::AgentMessage,
        ProviderFrameObservationSummaryV1::Started(ProviderLifecycleTimestampMsV1::new(1)),
        1,
        4,
        Some(target_narrative),
    );
    ProviderItemBuildRecord::new(
        SyndicItemId::from_bytes([2; 16]),
        SyndicTurnId::from_bytes([3; 16]),
        source("provider-item"),
        SourceEventSequence::FIRST,
        ProviderItemBuildRevision::FIRST,
        None,
        target,
        0,
        0,
        SyndicContentDigest::from_bytes([99; 32]),
        Some(ProviderNarrativeReference::empty(
            content_id,
            ProviderNarrativeGeneration::FIRST,
        )),
        None,
        ProviderItemBuildLifecycle::Staging,
    )
    .unwrap()
}

fn continued_build() -> ProviderItemBuildRecord {
    let content_id = SyndicContentId::from_bytes([5; 16]);
    let prior_narrative = narrative(content_id, 1, 1, 4, 31);
    let prior = sealed_frame(
        content_id,
        1,
        1,
        ProviderItemKind::AgentMessage,
        ProviderFrameObservationSummaryV1::Started(ProviderLifecycleTimestampMsV1::new(1)),
        1,
        4,
        Some(prior_narrative),
    );
    let target_narrative = narrative(content_id, 1, 2, 8, 32);
    let target = sealed_frame(
        content_id,
        2,
        2,
        ProviderItemKind::AgentMessage,
        ProviderFrameObservationSummaryV1::Delta,
        1,
        4,
        Some(target_narrative),
    );
    ProviderItemBuildRecord::new(
        SyndicItemId::from_bytes([6; 16]),
        SyndicTurnId::from_bytes([7; 16]),
        source("provider-item"),
        SourceEventSequence::new(2).unwrap(),
        ProviderItemBuildRevision::FIRST,
        Some(prior),
        target,
        1,
        ENCODED_FRAME_BYTES,
        SyndicContentDigest::from_bytes([1; 32]),
        Some(prior_narrative),
        None,
        ProviderItemBuildLifecycle::Staging,
    )
    .unwrap()
}

fn span_record() -> ProviderNarrativeSpanRecord {
    let content_id = SyndicContentId::from_bytes([1; 16]);
    let generation = ProviderNarrativeGeneration::FIRST;
    ProviderNarrativeSpanRecord::new(
        content_id,
        generation,
        0,
        4,
        ProviderFrameOrdinalV1::FIRST,
        [1; 32],
        20,
        24,
        [4; 32],
        provider_narrative_chain_seed(content_id, generation),
    )
    .unwrap()
}

#[test]
fn exact_provider_families_round_trip_and_registry_name_is_replaced_in_place() {
    let build = ProviderFixtureRecord::ItemBuild(Box::new(initial_build()));
    assert_eq!(build.family(), ProviderFixtureFamily::ItemBuilds);
    assert_eq!(build.family().name(), "provider-item-builds");
    assert_eq!(build.family().record_version(), 1);
    assert_eq!(build.family().maximum_key_bytes(), 16);
    assert_eq!(roundtrip_provider_fixture(&build).unwrap(), build);
    let continued = ProviderFixtureRecord::ItemBuild(Box::new(continued_build()));
    assert_eq!(roundtrip_provider_fixture(&continued).unwrap(), continued);

    let span = ProviderFixtureRecord::NarrativeSpan(Box::new(span_record()));
    assert_eq!(span.family(), ProviderFixtureFamily::NarrativeSpans);
    assert_eq!(span.family().name(), "provider-narrative-spans");
    assert_eq!(span.family().record_version(), 1);
    assert_eq!(span.family().maximum_key_bytes(), 32);
    assert_eq!(roundtrip_provider_fixture(&span).unwrap(), span);

    let names = ProviderFixtureFamily::domain_family_names();
    assert_eq!(names.len(), PhysicalFamily::ALL.len());
    assert!(names.contains(&"provider-narrative-spans"));
    assert!(names.contains(&"provider-observation-builds"));
    assert!(names.contains(&"provider-observation-chunks"));
    assert!(!names.contains(&"provider-frame-text-spans"));
}

#[test]
fn provider_families_reject_exact_structural_and_key_agreement_corruption() {
    let build = ProviderFixtureRecord::ItemBuild(Box::new(initial_build()));
    for corruption in [
        ProviderFixtureCorruption::TruncatedKey,
        ProviderFixtureCorruption::TruncatedValue,
        ProviderFixtureCorruption::TrailingKey,
        ProviderFixtureCorruption::TrailingValue,
        ProviderFixtureCorruption::InvalidValueTag,
        ProviderFixtureCorruption::KeyValueMismatch,
    ] {
        assert!(
            decode_corrupted_provider_fixture(&build, corruption).is_err(),
            "build family admitted {corruption:?}"
        );
    }

    let span = ProviderFixtureRecord::NarrativeSpan(Box::new(span_record()));
    for corruption in [
        ProviderFixtureCorruption::TruncatedKey,
        ProviderFixtureCorruption::TruncatedValue,
        ProviderFixtureCorruption::TrailingKey,
        ProviderFixtureCorruption::TrailingValue,
        ProviderFixtureCorruption::ZeroNarrativeGeneration,
        ProviderFixtureCorruption::KeyValueMismatch,
    ] {
        assert!(
            decode_corrupted_provider_fixture(&span, corruption).is_err(),
            "narrative-span family admitted {corruption:?}"
        );
    }
}

#[test]
fn narrative_seed_and_advance_hash_domain_separated_exact_fields() {
    let content_id = SyndicContentId::from_bytes([9; 16]);
    let generation = ProviderNarrativeGeneration::new(7).unwrap();
    let mut expected_seed = Sha256::new();
    expected_seed.update(b"beryl.syndic.provider-narrative-chain.seed.v1");
    expected_seed.update(content_id.as_bytes());
    expected_seed.update(generation.get().to_be_bytes());
    let expected_seed: [u8; 32] = expected_seed.finalize().into();
    assert_eq!(
        provider_narrative_chain_seed(content_id, generation),
        expected_seed
    );

    let frame_ordinal = ProviderFrameOrdinalV1::new(3).unwrap();
    let frame_digest = [5; 32];
    let source_digest = [6; 32];
    let mut expected_advance = Sha256::new();
    expected_advance.update(b"beryl.syndic.provider-narrative-chain.span.v1");
    expected_advance.update(expected_seed);
    expected_advance.update(content_id.as_bytes());
    expected_advance.update(generation.get().to_be_bytes());
    expected_advance.update(4_u64.to_be_bytes());
    expected_advance.update(9_u64.to_be_bytes());
    expected_advance.update(frame_ordinal.get().to_be_bytes());
    expected_advance.update(frame_digest);
    expected_advance.update(20_u64.to_be_bytes());
    expected_advance.update(25_u64.to_be_bytes());
    expected_advance.update(source_digest);
    let expected_advance: [u8; 32] = expected_advance.finalize().into();
    assert_eq!(
        advance_provider_narrative_chain(
            expected_seed,
            content_id,
            generation,
            4,
            9,
            frame_ordinal,
            frame_digest,
            20,
            25,
            source_digest,
        ),
        expected_advance
    );
    let record = ProviderNarrativeSpanRecord::new(
        content_id,
        generation,
        4,
        9,
        frame_ordinal,
        frame_digest,
        20,
        25,
        source_digest,
        expected_seed,
    )
    .unwrap();
    assert_eq!(record.resulting_chain_digest(), expected_advance);
}

#[test]
fn sealed_frames_enforce_exact_narrative_presence_content_and_empty_view() {
    let target = initial_build().target().clone();
    assert_narrative_rejected(&target, None);
    let wrong_content = narrative(SyndicContentId::from_bytes([99; 16]), 1, 1, 4, 21);
    assert_narrative_rejected(&target, Some(wrong_content));

    let content_id = SyndicContentId::from_bytes([11; 16]);
    let empty = ProviderNarrativeReference::empty(content_id, ProviderNarrativeGeneration::FIRST);
    let sealed_empty = sealed_frame(
        content_id,
        1,
        1,
        ProviderItemKind::AgentMessage,
        ProviderFrameObservationSummaryV1::Started(ProviderLifecycleTimestampMsV1::new(1)),
        0,
        0,
        Some(empty),
    );
    assert_eq!(sealed_empty.narrative(), Some(empty));
    let operational = sealed_frame(
        SyndicContentId::from_bytes([12; 16]),
        1,
        1,
        ProviderItemKind::Reasoning,
        ProviderFrameObservationSummaryV1::Started(ProviderLifecycleTimestampMsV1::new(1)),
        1,
        4,
        None,
    );
    assert!(operational.narrative().is_none());
}

#[test]
fn build_seals_only_at_targets_and_completion_preserves_narrative_pending_equality() {
    let initial = initial_build();
    let summary = initial.target().content().summary();
    assert!(
        initial
            .advance(
                summary.chunk_count(),
                summary.encoded_bytes(),
                summary.digest(),
                initial.staged_narrative(),
                ProviderItemBuildLifecycle::Sealed,
            )
            .is_err()
    );
    let sealed = initial
        .advance(
            summary.chunk_count(),
            summary.encoded_bytes(),
            summary.digest(),
            initial.target().narrative(),
            ProviderItemBuildLifecycle::Sealed,
        )
        .unwrap();
    assert_eq!(sealed.lifecycle(), ProviderItemBuildLifecycle::Sealed);

    let prior = continued_build().target().clone();
    let content_id = prior.content().id();
    let prior_narrative = prior.narrative().unwrap();
    let completion_ordinal = ProviderFrameOrdinalV1::new(3).unwrap();
    let target = sealed_frame(
        content_id,
        3,
        completion_ordinal.get(),
        ProviderItemKind::AgentMessage,
        ProviderFrameObservationSummaryV1::Completed(ProviderLifecycleTimestampMsV1::new(2)),
        1,
        8,
        Some(prior_narrative),
    );
    let completion_check = ProviderNarrativeCompletionCheck::new(
        Some(
            ProviderFrameTextSpanV1::new(
                completion_ordinal,
                0,
                8,
                128,
                136,
                [44; 32],
                ProviderLogicalTextRoleV1::Narrative,
            )
            .unwrap(),
        ),
        ProviderNarrativeCompletionState::Pending(ProviderNarrativeComparisonFrontier::initial(
            prior_narrative,
        )),
    );
    let build = ProviderItemBuildRecord::new(
        SyndicItemId::from_bytes([6; 16]),
        SyndicTurnId::from_bytes([7; 16]),
        source("provider-item"),
        SourceEventSequence::new(3).unwrap(),
        ProviderItemBuildRevision::FIRST,
        Some(prior),
        target,
        2,
        128,
        SyndicContentDigest::from_bytes([2; 32]),
        Some(prior_narrative),
        Some(completion_check),
        ProviderItemBuildLifecycle::Staging,
    )
    .unwrap();
    assert_eq!(build.target().narrative(), Some(prior_narrative));
    assert_eq!(build.completion_check(), Some(completion_check));
    assert_eq!(build.lifecycle(), ProviderItemBuildLifecycle::Staging);
    let fixture = ProviderFixtureRecord::ItemBuild(Box::new(build));
    assert_eq!(roundtrip_provider_fixture(&fixture).unwrap(), fixture);
}
