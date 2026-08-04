use super::*;

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
        (
            b"https://[".as_slice(),
            ProviderItemValidationError::InvalidDynamicImageLocator,
        ),
        (
            b"https://[xyz]".as_slice(),
            ProviderItemValidationError::InvalidDynamicImageLocator,
        ),
        (
            b"https://host[::1]".as_slice(),
            ProviderItemValidationError::InvalidDynamicImageLocator,
        ),
        (
            b"https://host:port/image".as_slice(),
            ProviderItemValidationError::InvalidDynamicImageLocator,
        ),
        (
            b"x:a#b#c".as_slice(),
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
