use std::{convert::Infallible, io::Read, num::NonZeroU64};

use beryl_model::{AssetId, CasItemId};
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
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: value,
        phase: None,
        memory_citation: None,
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

#[test]
fn lifecycle_summary_rejects_missing_start_and_reversed_timestamps() {
    let delta = frame(
        1,
        ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
            delta: text("delta"),
        }),
    );
    let (delta_bytes, delta_reference) = encode(&delta, 0);
    let mut no_spans = ProviderFrameTextSpanValidatorV1::new(delta_reference.ordinal());
    let delta_structural = validate_streaming_provider_item_frame_v1(
        &mut TinyReader::new(&delta_bytes.bytes, 7),
        0,
        delta_reference.encoded_len(),
        delta_reference.encoded_digest(),
        &mut no_spans,
    )
    .unwrap();
    assert_eq!(
        ProviderItemStreamValidatorV1::new().observe_structural(&delta_structural),
        Err(ProviderItemValidationError::MissingItemStart)
    );

    let started = frame(
        1,
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(20),
            item: agent(text("start")),
        },
    );
    let completed = frame(
        2,
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(19),
            item: agent(text("complete")),
        },
    );
    let mut lifecycle = ProviderItemStreamValidatorV1::new();
    lifecycle.observe(&started).unwrap();
    assert_eq!(
        lifecycle.observe(&completed),
        Err(ProviderItemValidationError::CompletionBeforeStart {
            started: 20,
            completed: 19,
        })
    );

    let completion_only = frame(
        1,
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(30),
            item: ProviderItemV1::SubAgentActivity(ProviderSubAgentActivityV1 {
                kind: ProviderSubAgentActivityKindV1::Started,
                agent_thread_id: beryl_model::CasThreadId::new("completion-only").unwrap(),
                agent_path: text("root/worker"),
            }),
        },
    );
    let mut completion_only_lifecycle = ProviderItemStreamValidatorV1::new();
    completion_only_lifecycle.observe(&completion_only).unwrap();
    assert!(completion_only_lifecycle.is_complete());
}

#[test]
fn unsupported_web_search_evidence_is_retained_and_monotonic_in_both_paths() {
    let started = frame(
        1,
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(10),
            item: ProviderItemV1::WebSearch(ProviderWebSearchV1 {
                query: text("query"),
                action: Some(ProviderWebSearchActionV1::Other),
            }),
        },
    );
    let completed = frame(
        2,
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(11),
            item: ProviderItemV1::WebSearch(ProviderWebSearchV1 {
                query: text("query"),
                action: None,
            }),
        },
    );
    let unsupported = ProviderFrameHistorySupportV1::Unsupported(
        UnsupportedHistoryReason::UnsupportedRequiredPayload,
    );
    assert_eq!(started.history_support(), unsupported);

    let mut materialized = ProviderItemStreamValidatorV1::new();
    materialized.observe(&started).unwrap();
    materialized.observe(&completed).unwrap();
    assert!(materialized.is_complete());
    assert!(!materialized.is_history_complete());
    assert_eq!(materialized.history_support(), unsupported);

    let (start_bytes, start_reference) = encode(&started, 0);
    let (completion_bytes, completion_reference) =
        encode(&completed, start_reference.encoded_end());
    let mut structural_lifecycle = ProviderItemStreamValidatorV1::new();
    for (bytes, reference, start) in [
        (&start_bytes.bytes, &start_reference, 0),
        (
            &completion_bytes.bytes,
            &completion_reference,
            start_reference.encoded_end(),
        ),
    ] {
        let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
        let structural = validate_streaming_provider_item_frame_v1(
            &mut TinyReader::new(bytes, 3),
            start,
            reference.encoded_len(),
            reference.encoded_digest(),
            &mut spans,
        )
        .unwrap();
        structural_lifecycle
            .observe_structural(&structural)
            .unwrap();
    }
    assert!(structural_lifecycle.is_complete());
    assert!(!structural_lifecycle.is_history_complete());
    assert_eq!(structural_lifecycle.history_support(), unsupported);
}

#[test]
fn completed_image_generation_rejects_in_progress_status_in_both_decoders() {
    let started = frame(
        1,
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(1),
            item: ProviderItemV1::StandaloneImageGeneration(ProviderImageGenerationV1 {
                status: ProviderImageGenerationStatusV1::InProgress,
                revised_prompt: None,
                saved_path: None,
            }),
        },
    );
    let (encoded, reference) = encode(&started, 0);
    let item_id_length = u32::from_be_bytes(encoded.bytes[12..16].try_into().unwrap()) as usize;
    let observation_position = 16 + item_id_length;
    let mut invalid_completion = encoded.bytes.clone();
    invalid_completion[observation_position] = 2;
    let expected = ProviderItemValidationError::CompletionStatusInProgress;
    assert!(matches!(
        decode_bounded_provider_item_frame_v1(
            &invalid_completion,
            PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES,
            0,
        ),
        Err(ProviderFrameDecodeError::InvalidValue(error)) if error == expected
    ));
    let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
    assert!(matches!(
        validate_streaming_provider_item_frame_v1(
            &mut TinyReader::new(&invalid_completion, 1),
            0,
            invalid_completion.len() as u64,
            digest(&invalid_completion),
            &mut spans,
        ),
        Err(ProviderFrameStreamError::Decode(
            ProviderFrameDecodeError::InvalidValue(error)
        )) if error == expected
    ));
}

fn dynamic_locator_frame() -> ProviderItemFrameV1 {
    frame(
        1,
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(1),
            item: ProviderItemV1::DynamicToolCall(ProviderDynamicToolCallV1 {
                namespace: None,
                tool: text("tool"),
                arguments: ProviderStructuredValueV1::Null,
                status: ProviderToolCallStatusV1::Completed,
                content_items: Some(vec![ProviderDynamicToolOutputV1::InputImageLocator {
                    locator: ProviderImageLocatorV1::new("https://example.test/image").unwrap(),
                }]),
                success: Some(true),
                duration_ms: None,
            }),
        },
    )
}

fn replace_dynamic_locator(bytes: &[u8], replacement: &[u8]) -> Vec<u8> {
    const ORIGINAL: &[u8] = b"https://example.test/image";
    let position = bytes
        .windows(ORIGINAL.len())
        .position(|window| window == ORIGINAL)
        .unwrap();
    let length_position = position - std::mem::size_of::<u64>();
    let mut changed = bytes.to_vec();
    changed[length_position..position].copy_from_slice(&(replacement.len() as u64).to_be_bytes());
    changed.splice(
        position..position + ORIGINAL.len(),
        replacement.iter().copied(),
    );
    changed
}

#[test]
fn streaming_validator_rejects_typed_data_locators_and_frame_corruption() {
    let frame = dynamic_locator_frame();
    let (encoded, reference) = encode(&frame, 0);
    for (replacement, expected) in [
        (
            b" \tDATA:image/png;base64,AAAA".as_slice(),
            ProviderItemValidationError::DynamicImageDataUrlRequiresAsset,
        ),
        (
            b"".as_slice(),
            ProviderItemValidationError::InvalidDynamicImageLocator,
        ),
        (
            b"   ".as_slice(),
            ProviderItemValidationError::InvalidDynamicImageLocator,
        ),
        (
            b"not-a-locator".as_slice(),
            ProviderItemValidationError::InvalidDynamicImageLocator,
        ),
        (
            b"https://bad%escape".as_slice(),
            ProviderItemValidationError::InvalidDynamicImageLocator,
        ),
    ] {
        let malformed = replace_dynamic_locator(&encoded.bytes, replacement);
        let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
        assert!(matches!(
            validate_streaming_provider_item_frame_v1(
                &mut TinyReader::new(&malformed, 2),
                0,
                malformed.len() as u64,
                digest(&malformed),
                &mut spans,
            ),
            Err(ProviderFrameStreamError::Decode(
                ProviderFrameDecodeError::InvalidValue(error)
            )) if error == expected
        ));
        assert!(matches!(
            decode_bounded_provider_item_frame_v1(
                &malformed,
                PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES,
                0,
            ),
            Err(ProviderFrameDecodeError::InvalidValue(error)) if error == expected
        ));
    }

    let truncated = &encoded.bytes[..encoded.bytes.len() - 1];
    let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
    assert!(matches!(
        validate_streaming_provider_item_frame_v1(
            &mut TinyReader::new(truncated, 5),
            0,
            reference.encoded_len(),
            reference.encoded_digest(),
            &mut spans,
        ),
        Err(ProviderFrameStreamError::Decode(
            ProviderFrameDecodeError::Truncated
        ))
    ));

    let mut trailing = encoded.bytes.clone();
    trailing.push(0);
    let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
    assert!(matches!(
        validate_streaming_provider_item_frame_v1(
            &mut TinyReader::new(&trailing, 5),
            0,
            trailing.len() as u64,
            digest(&trailing),
            &mut spans,
        ),
        Err(ProviderFrameStreamError::Decode(
            ProviderFrameDecodeError::TrailingBytes
        ))
    ));

    let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
    assert!(matches!(
        validate_streaming_provider_item_frame_v1(
            &mut TinyReader::new(&encoded.bytes, 5),
            0,
            reference.encoded_len(),
            [0; 32],
            &mut spans,
        ),
        Err(ProviderFrameStreamError::Decode(
            ProviderFrameDecodeError::DigestMismatch
        ))
    ));
}

#[test]
fn streaming_validator_classifies_only_the_mcp_type_discriminator() {
    let content = ProviderMcpContentV1::structured(ProviderStructuredValueV1::Object(vec![
        ProviderObjectEntryV1 {
            key: "type".to_owned(),
            value: ProviderStructuredValueV1::String(text("other")),
        },
        ProviderObjectEntryV1 {
            key: "type".to_owned(),
            value: ProviderStructuredValueV1::String(text("other")),
        },
        ProviderObjectEntryV1 {
            key: "body".to_owned(),
            value: ProviderStructuredValueV1::String(text("data:image/png;base64,opaque")),
        },
    ]))
    .unwrap();
    let frame = frame(
        1,
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(1),
            item: ProviderItemV1::McpToolCall(ProviderMcpToolCallV1 {
                server: text("server"),
                tool: text("tool"),
                status: ProviderToolCallStatusV1::Completed,
                arguments: ProviderStructuredValueV1::Null,
                app_context: None,
                mcp_app_resource_uri: None,
                plugin_id: None,
                result: Some(ProviderMcpResultV1 {
                    content: vec![content],
                    structured_content: None,
                    meta: None,
                }),
                error: None,
                duration_ms: None,
            }),
        },
    );
    let (encoded, reference) = encode(&frame, 0);
    let position = encoded
        .bytes
        .windows(5)
        .position(|window| window == b"other")
        .unwrap();
    let mut typed_image = encoded.bytes.clone();
    typed_image[position..position + 5].copy_from_slice(b"image");
    let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
    assert!(matches!(
        validate_streaming_provider_item_frame_v1(
            &mut TinyReader::new(&typed_image, 3),
            0,
            typed_image.len() as u64,
            digest(&typed_image),
            &mut spans,
        ),
        Err(ProviderFrameStreamError::Decode(
            ProviderFrameDecodeError::InvalidValue(
                ProviderItemValidationError::McpInlineImageRequiresAsset
            )
        ))
    ));
}

#[test]
fn mcp_inline_metadata_depth_maximum_and_plus_one_match_both_decoders() {
    let mut maximum = ProviderStructuredValueV1::Null;
    for _ in 1..PROVIDER_STRUCTURED_VALUE_MAX_DEPTH {
        maximum = ProviderStructuredValueV1::List(vec![maximum]);
    }
    let inline_image = ProviderMcpInlineImageV1::new(
        ProviderInlineImageAssetV1::new(AssetId::sha256_v1([41; 32], NonZeroU64::new(41).unwrap())),
        vec![ProviderObjectEntryV1 {
            key: "depth-marker-unique".to_owned(),
            value: maximum,
        }],
    )
    .unwrap();
    let maximum_frame = frame(
        1,
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(1),
            item: ProviderItemV1::McpToolCall(ProviderMcpToolCallV1 {
                server: text("server"),
                tool: text("tool"),
                status: ProviderToolCallStatusV1::Completed,
                arguments: ProviderStructuredValueV1::Null,
                app_context: None,
                mcp_app_resource_uri: None,
                plugin_id: None,
                result: Some(ProviderMcpResultV1 {
                    content: vec![ProviderMcpContentV1::inline_image(inline_image)],
                    structured_content: None,
                    meta: None,
                }),
                error: None,
                duration_ms: None,
            }),
        },
    );
    let (encoded, reference) = encode(&maximum_frame, 0);
    let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
    validate_streaming_provider_item_frame_v1(
        &mut TinyReader::new(&encoded.bytes, 1),
        0,
        reference.encoded_len(),
        reference.encoded_digest(),
        &mut spans,
    )
    .unwrap();

    let marker = b"depth-marker-unique";
    let marker_end = encoded
        .bytes
        .windows(marker.len())
        .position(|window| window == marker)
        .unwrap()
        + marker.len();
    let mut excessive = encoded.bytes.clone();
    let mut extra_list = vec![7];
    extra_list.extend_from_slice(&1_u64.to_be_bytes());
    excessive.splice(marker_end..marker_end, extra_list);
    let expected = ProviderItemValidationError::StructuredDepthExceeded {
        maximum: PROVIDER_STRUCTURED_VALUE_MAX_DEPTH,
    };
    assert!(matches!(
        decode_bounded_provider_item_frame_v1(
            &excessive,
            PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES,
            0,
        ),
        Err(ProviderFrameDecodeError::InvalidValue(error)) if error == expected
    ));
    let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
    assert!(matches!(
        validate_streaming_provider_item_frame_v1(
            &mut TinyReader::new(&excessive, 1),
            0,
            excessive.len() as u64,
            digest(&excessive),
            &mut spans,
        ),
        Err(ProviderFrameStreamError::Decode(
            ProviderFrameDecodeError::InvalidValue(error)
        )) if error == expected
    ));
}
