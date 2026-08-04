use std::{convert::Infallible, io::Read, num::NonZeroU64};

use beryl_model::{
    AssetId, CasItemId, ContentRevision, ImageLabelOrdinal, SyndicContentDigest, SyndicContentId,
};
use sha2::{Digest, Sha256};
use syndic_storage::*;

#[derive(Default)]
struct CaptureSink {
    bytes: Vec<u8>,
    spans: Vec<ProviderFrameTextSpanV1>,
    largest_chunk: usize,
}

impl ProviderFrameSinkV1 for CaptureSink {
    type Error = Infallible;

    fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        self.largest_chunk = self.largest_chunk.max(chunk.len());
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        self.spans.push(span);
        Ok(())
    }
}

struct TinyReader<'a> {
    bytes: &'a [u8],
    position: usize,
    maximum_return: usize,
    largest_request: usize,
}

impl<'a> TinyReader<'a> {
    fn new(bytes: &'a [u8], maximum_return: usize) -> Self {
        Self {
            bytes,
            position: 0,
            maximum_return,
            largest_request: 0,
        }
    }
}

impl Read for TinyReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        self.largest_request = self.largest_request.max(output.len());
        let take = output
            .len()
            .min(self.maximum_return)
            .min(self.bytes.len().saturating_sub(self.position));
        output[..take].copy_from_slice(&self.bytes[self.position..self.position + take]);
        self.position += take;
        Ok(take)
    }
}

fn text(value: impl Into<String>) -> ProviderTextV1 {
    ProviderTextV1::inline(value)
}

fn agent(value: ProviderTextV1) -> ProviderItemV1 {
    agent_with_phase(value, None)
}

fn agent_with_phase(
    value: ProviderTextV1,
    phase: Option<ProviderMessagePhaseV1>,
) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: value,
        phase,
        memory_citation: None,
    })
}

fn submitted_content(
    image_marker_count: u64,
    maximum_image_label: Option<ImageLabelOrdinal>,
) -> ContentReference {
    ContentReference::new(
        SyndicContentId::from_bytes([3; 16]),
        ContentRevision::new(2).unwrap(),
        ContentEncoding::ComposerV1,
        ContentSummary::new(
            1,
            2,
            3,
            4,
            5,
            image_marker_count,
            [7; 32],
            maximum_image_label,
            SyndicContentDigest::from_bytes([8; 32]),
        )
        .unwrap(),
    )
}

fn user(content: ContentReference) -> ProviderItemV1 {
    ProviderItemV1::UserMessage(ProviderUserMessageV1 {
        client_id: None,
        submitted: ProviderSubmittedContentV1 { content },
    })
}

fn frame(ordinal: u64, observation: ProviderItemObservationV1) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::new(ordinal).unwrap(),
        CasItemId::new("streaming-item").unwrap(),
        observation,
    )
}

fn encode(frame: &ProviderItemFrameV1, start: u64) -> (CaptureSink, ProviderFrameReferenceV1) {
    let mut sink = CaptureSink::default();
    let reference = encode_provider_item_frame_v1(frame, start, &mut sink).unwrap();
    (sink, reference)
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[test]
fn structural_summary_retains_exact_publication_facts_across_reader_boundaries() {
    let submitted_with_label = submitted_content(6, Some(ImageLabelOrdinal::new(37).unwrap()));
    let submitted_without_label = submitted_content(0, None);
    let cases: Vec<(
        ProviderItemFrameV1,
        ProviderItemKind,
        Option<ProviderMessagePhaseV1>,
        Option<ContentReference>,
    )> = vec![
        (
            frame(
                1,
                ProviderItemObservationV1::Started {
                    observed_at: ProviderLifecycleTimestampMsV1::new(1),
                    item: agent_with_phase(
                        text("commentary"),
                        Some(ProviderMessagePhaseV1::Commentary),
                    ),
                },
            ),
            ProviderItemKind::AgentMessage,
            Some(ProviderMessagePhaseV1::Commentary),
            None,
        ),
        (
            frame(
                2,
                ProviderItemObservationV1::Completed {
                    observed_at: ProviderLifecycleTimestampMsV1::new(2),
                    item: agent_with_phase(
                        text("final"),
                        Some(ProviderMessagePhaseV1::FinalAnswer),
                    ),
                },
            ),
            ProviderItemKind::AgentMessage,
            Some(ProviderMessagePhaseV1::FinalAnswer),
            None,
        ),
        (
            frame(
                3,
                ProviderItemObservationV1::Completed {
                    observed_at: ProviderLifecycleTimestampMsV1::new(3),
                    item: agent(text("phase absent")),
                },
            ),
            ProviderItemKind::AgentMessage,
            None,
            None,
        ),
        (
            frame(
                4,
                ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                    delta: text("delta"),
                }),
            ),
            ProviderItemKind::AgentMessage,
            None,
            None,
        ),
        (
            frame(
                5,
                ProviderItemObservationV1::Started {
                    observed_at: ProviderLifecycleTimestampMsV1::new(5),
                    item: ProviderItemV1::Plan(ProviderPlanV1 { text: text("plan") }),
                },
            ),
            ProviderItemKind::Plan,
            None,
            None,
        ),
        (
            frame(
                6,
                ProviderItemObservationV1::Started {
                    observed_at: ProviderLifecycleTimestampMsV1::new(6),
                    item: user(submitted_with_label),
                },
            ),
            ProviderItemKind::UserMessage,
            None,
            Some(submitted_with_label),
        ),
        (
            frame(
                7,
                ProviderItemObservationV1::Completed {
                    observed_at: ProviderLifecycleTimestampMsV1::new(7),
                    item: user(submitted_without_label),
                },
            ),
            ProviderItemKind::UserMessage,
            None,
            Some(submitted_without_label),
        ),
    ];

    for (frame, expected_kind, expected_phase, expected_content) in cases {
        let (encoded, reference) = encode(&frame, 0);
        for maximum_return in [1, 2, 7, encoded.bytes.len()] {
            let mut reader = TinyReader::new(&encoded.bytes, maximum_return);
            let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
            let structural = validate_streaming_provider_item_frame_v1(
                &mut reader,
                0,
                reference.encoded_len(),
                reference.encoded_digest(),
                &mut spans,
            )
            .unwrap();
            spans.finish(structural.reference()).unwrap();

            assert_eq!(structural.reference(), &reference);
            assert_eq!(structural.reference().item_kind(), expected_kind);
            assert_eq!(structural.message_phase(), expected_phase);
            assert_eq!(structural.submitted_content(), expected_content);
            assert_eq!(
                structural
                    .submitted_content()
                    .map(|content| content.summary().maximum_image_label()),
                expected_content.map(|content| content.summary().maximum_image_label())
            );
            assert_eq!(reader.position, encoded.bytes.len());
        }
    }
}

#[test]
fn arbitrarily_large_frame_is_validated_with_constant_resident_reads() {
    let large = "€".repeat(800_000);
    let frame = frame(
        1,
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(7),
            item: agent(text(large)),
        },
    );
    let (encoded, reference) = encode(&frame, 0);
    assert!(encoded.bytes.len() > 2_000_000);
    assert!(encoded.largest_chunk <= PROVIDER_FRAME_CHUNK_MAX_BYTES);
    assert!(matches!(
        decode_bounded_provider_item_frame_v1(
            &encoded.bytes,
            PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES,
            0,
        ),
        Err(ProviderFrameDecodeError::FrameTooLarge { .. })
    ));

    let mut reader = TinyReader::new(&encoded.bytes, 13);
    let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
    let structural = validate_streaming_provider_item_frame_v1(
        &mut reader,
        0,
        encoded.bytes.len() as u64,
        reference.encoded_digest(),
        &mut spans,
    )
    .unwrap();
    assert_eq!(structural.reference(), &reference);
    spans.finish(structural.reference()).unwrap();
    assert!(reader.largest_request <= 4_096);
    assert_eq!(reader.position, encoded.bytes.len());
}

#[test]
fn completion_reuses_prior_text_range_without_copying_bytes() {
    let original = "reused provider text".repeat(1_000);
    let started = frame(
        1,
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(100),
            item: agent(text(original)),
        },
    );
    let (start_encoded, start_reference) = encode(&started, 0);
    let source = start_encoded.spans[0];
    let reused = ProviderTextReferenceV1::new(
        source.source_start(),
        source.source_end(),
        source.source_digest(),
    )
    .unwrap();
    let completed = frame(
        2,
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(101),
            item: agent(ProviderTextV1::reused(reused)),
        },
    );
    let (completion_encoded, completion_reference) =
        encode(&completed, start_reference.encoded_end());
    assert!(completion_encoded.bytes.len() < start_encoded.bytes.len());
    assert_eq!(completion_encoded.spans.len(), 1);
    assert_eq!(completion_encoded.spans[0].source_start(), reused.start());
    assert_eq!(completion_encoded.spans[0].source_end(), reused.end());
    assert_eq!(completion_encoded.spans[0].source_digest(), reused.digest());
    assert_eq!(
        decode_bounded_provider_item_frame_v1(
            &completion_encoded.bytes,
            PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES,
            start_reference.encoded_end(),
        )
        .unwrap(),
        completed
    );

    let mut lifecycle = ProviderItemStreamValidatorV1::new();
    let mut start_spans = ProviderFrameTextSpanValidatorV1::new(start_reference.ordinal());
    let start_structural = validate_streaming_provider_item_frame_v1(
        &mut TinyReader::new(&start_encoded.bytes, 17),
        0,
        start_reference.encoded_len(),
        start_reference.encoded_digest(),
        &mut start_spans,
    )
    .unwrap();
    start_spans.finish(start_structural.reference()).unwrap();
    lifecycle.observe_structural(&start_structural).unwrap();

    let mut completion_spans =
        ProviderFrameTextSpanValidatorV1::new(completion_reference.ordinal());
    let completion_structural = validate_streaming_provider_item_frame_v1(
        &mut TinyReader::new(&completion_encoded.bytes, 19),
        start_reference.encoded_end(),
        completion_reference.encoded_len(),
        completion_reference.encoded_digest(),
        &mut completion_spans,
    )
    .unwrap();
    completion_spans
        .finish(completion_structural.reference())
        .unwrap();
    lifecycle
        .observe_structural(&completion_structural)
        .unwrap();
    assert!(lifecycle.is_complete());
    assert_eq!(lifecycle.kind(), Some(ProviderItemKind::AgentMessage));
}

#[path = "phase13_provider_streaming/corruption.rs"]
mod corruption;
#[path = "phase13_provider_streaming/lifecycle.rs"]
mod lifecycle;
