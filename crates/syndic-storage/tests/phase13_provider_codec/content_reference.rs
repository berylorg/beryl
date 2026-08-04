use super::*;

fn user_frame(content: ContentReference) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        CasItemId::new("submitted-content").unwrap(),
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(44),
            item: ProviderItemV1::UserMessage(ProviderUserMessageV1 {
                client_id: None,
                submitted: ProviderSubmittedContentV1 { content },
            }),
        },
    )
}

fn encode_user_frame(content: ContentReference) -> (CollectSink, ProviderFrameReferenceV1) {
    let mut encoded = CollectSink::default();
    let reference = encode_provider_item_frame_v1(&user_frame(content), 100, &mut encoded).unwrap();
    (encoded, reference)
}

#[test]
fn submitted_content_preserves_maximum_image_label_exactly() {
    for content in [
        submitted_content_with_markers(0, None),
        submitted_content_with_markers(6, Some(ImageLabelOrdinal::new(37).unwrap())),
    ] {
        let expected_label = content.summary().maximum_image_label();
        let frame = user_frame(content);
        let (encoded, reference) = encode_user_frame(content);

        let decoded = decode_bounded_provider_item_frame_v1(
            &encoded.bytes,
            PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES,
            100,
        )
        .unwrap();
        assert_eq!(decoded, frame);
        let ProviderItemObservationV1::Completed {
            item: ProviderItemV1::UserMessage(message),
            ..
        } = decoded.observation()
        else {
            panic!("decoded submitted content changed provider item shape");
        };
        assert_eq!(
            message.submitted.content.summary().maximum_image_label(),
            expected_label
        );

        let mut spans = SpanSink::default();
        let structural = validate_streaming_provider_item_frame_v1(
            &mut Cursor::new(&encoded.bytes),
            100,
            encoded.bytes.len() as u64,
            reference.encoded_digest(),
            &mut spans,
        )
        .unwrap();
        assert_eq!(structural.submitted_content(), Some(content));
        assert_eq!(
            structural
                .submitted_content()
                .unwrap()
                .summary()
                .maximum_image_label(),
            expected_label
        );
    }
}

fn assert_invalid_content_reference(bytes: &[u8]) {
    assert!(matches!(
        decode_bounded_provider_item_frame_v1(bytes, PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES, 100,),
        Err(ProviderFrameDecodeError::InvalidContentReference)
    ));

    let mut spans = SpanSink::default();
    assert!(matches!(
        validate_streaming_provider_item_frame_v1(
            &mut Cursor::new(bytes),
            100,
            bytes.len() as u64,
            [0; 32],
            &mut spans,
        ),
        Err(ProviderFrameStreamError::Decode(
            ProviderFrameDecodeError::InvalidContentReference
        ))
    ));
}

#[test]
fn submitted_content_rejects_marker_count_and_maximum_label_disagreement() {
    let (encoded, _) = encode_user_frame(submitted_content_with_markers(
        6,
        Some(ImageLabelOrdinal::new(37).unwrap()),
    ));
    let marker_digest_position = encoded
        .bytes
        .windows(32)
        .position(|window| window == [7; 32])
        .unwrap();
    let option_position = marker_digest_position + 32;
    let mut missing_label = encoded.bytes;
    assert_eq!(missing_label[option_position], 1);
    missing_label[option_position] = 0;
    missing_label.drain(option_position + 1..option_position + 9);
    assert_invalid_content_reference(&missing_label);

    let (encoded, _) = encode_user_frame(submitted_content_with_markers(0, None));
    let marker_digest_position = encoded
        .bytes
        .windows(32)
        .position(|window| window == [7; 32])
        .unwrap();
    let option_position = marker_digest_position + 32;
    let mut unexpected_label = encoded.bytes;
    assert_eq!(unexpected_label[option_position], 0);
    unexpected_label[option_position] = 1;
    unexpected_label.splice(
        option_position + 1..option_position + 1,
        37_u64.to_be_bytes(),
    );
    assert_invalid_content_reference(&unexpected_label);
}
