use super::*;

#[test]
fn typed_image_boundaries_reject_bytes_without_scanning_ordinary_strings() {
    assert_eq!(
        ProviderImageLocatorV1::new("DATA:image/png;base64,AAAA"),
        Err(ProviderItemValidationError::DynamicImageDataUrlRequiresAsset)
    );
    assert_eq!(
        ProviderImageLocatorV1::new(" \tdata:image/png;base64,AAAA"),
        Err(ProviderItemValidationError::DynamicImageDataUrlRequiresAsset)
    );
    for malformed in [
        "",
        "   ",
        "not a locator",
        "https://bad%escape",
        "https://[",
        "https://[xyz]",
        "https://host[::1]",
        "https://host:port/image",
        "x:a#b#c",
    ] {
        assert_eq!(
            ProviderImageLocatorV1::new(malformed),
            Err(ProviderItemValidationError::InvalidDynamicImageLocator)
        );
    }
    assert!(ProviderImageLocatorV1::new("file:///tmp/image.png").is_ok());
    assert!(ProviderImageLocatorV1::new("x:").is_ok());
    assert!(ProviderImageLocatorV1::new("https://example.test/data:image.png").is_ok());
    assert!(ProviderImageLocatorV1::new("https://[2001:db8::1]/image.png").is_ok());
    assert!(ProviderImageLocatorV1::new("x://[v1.alpha:beta]/image").is_ok());

    let typed_image = ProviderStructuredValueV1::Object(vec![ProviderObjectEntryV1 {
        key: "type".to_owned(),
        value: ProviderStructuredValueV1::String(text("image")),
    }]);
    assert_eq!(
        ProviderMcpContentV1::structured(typed_image),
        Err(ProviderItemValidationError::McpInlineImageRequiresAsset)
    );

    let opaque = ProviderStructuredValueV1::Object(vec![ProviderObjectEntryV1 {
        key: "body".to_owned(),
        value: ProviderStructuredValueV1::String(text("data:image/png;base64,AAAA")),
    }]);
    assert!(ProviderMcpContentV1::structured(opaque).is_ok());

    assert_eq!(
        ProviderMcpInlineImageV1::new(
            asset(11),
            vec![ProviderObjectEntryV1 {
                key: "image_url".to_owned(),
                value: ProviderStructuredValueV1::String(text("anything")),
            }],
        ),
        Err(ProviderItemValidationError::McpImageMetadataContainsBytes { field: "image URL" })
    );
}

#[test]
fn structured_values_enforce_the_exact_container_depth() {
    let mut maximum = ProviderStructuredValueV1::Null;
    for _ in 0..PROVIDER_STRUCTURED_VALUE_MAX_DEPTH {
        maximum = ProviderStructuredValueV1::List(vec![maximum]);
    }
    maximum.validate(0).unwrap();
    assert_round_trip(ProviderItemObservationV1::Completed {
        observed_at: ProviderLifecycleTimestampMsV1::new(1),
        item: ProviderItemV1::DynamicToolCall(ProviderDynamicToolCallV1 {
            namespace: None,
            tool: text("depth"),
            arguments: maximum,
            status: ProviderToolCallStatusV1::Completed,
            content_items: None,
            success: Some(true),
            duration_ms: None,
        }),
    });

    let mut excessive = ProviderStructuredValueV1::Null;
    for _ in 0..=PROVIDER_STRUCTURED_VALUE_MAX_DEPTH {
        excessive = ProviderStructuredValueV1::List(vec![excessive]);
    }
    assert_eq!(
        excessive.validate(0),
        Err(ProviderItemValidationError::StructuredDepthExceeded {
            maximum: PROVIDER_STRUCTURED_VALUE_MAX_DEPTH,
        })
    );
}

#[test]
fn mcp_inline_metadata_counts_the_already_consumed_image_object_depth() {
    let mut maximum = ProviderStructuredValueV1::Null;
    for _ in 1..PROVIDER_STRUCTURED_VALUE_MAX_DEPTH {
        maximum = ProviderStructuredValueV1::List(vec![maximum]);
    }
    assert_round_trip(ProviderItemObservationV1::Completed {
        observed_at: ProviderLifecycleTimestampMsV1::new(1),
        item: mcp_inline_metadata_item(maximum),
    });

    let mut excessive = ProviderStructuredValueV1::Null;
    for _ in 0..PROVIDER_STRUCTURED_VALUE_MAX_DEPTH {
        excessive = ProviderStructuredValueV1::List(vec![excessive]);
    }
    let frame = ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        CasItemId::new("mcp-depth").unwrap(),
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(1),
            item: mcp_inline_metadata_item(excessive),
        },
    );
    assert_eq!(
        frame.validate(0),
        Err(ProviderItemValidationError::StructuredDepthExceeded {
            maximum: PROVIDER_STRUCTURED_VALUE_MAX_DEPTH,
        })
    );
}

#[test]
fn standalone_image_generation_uses_closed_statuses_and_rejects_in_progress_completion() {
    assert_round_trip(ProviderItemObservationV1::Started {
        observed_at: ProviderLifecycleTimestampMsV1::new(1),
        item: ProviderItemV1::StandaloneImageGeneration(ProviderImageGenerationV1 {
            status: ProviderImageGenerationStatusV1::InProgress,
            revised_prompt: None,
            saved_path: None,
        }),
    });
    for status in [
        ProviderImageGenerationStatusV1::Failed,
        ProviderImageGenerationStatusV1::Completed,
    ] {
        assert_round_trip(ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(2),
            item: ProviderItemV1::StandaloneImageGeneration(ProviderImageGenerationV1 {
                status,
                revised_prompt: None,
                saved_path: Some(text("file.png")),
            }),
        });
    }

    let completed = ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        CasItemId::new("generated-image").unwrap(),
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(2),
            item: ProviderItemV1::StandaloneImageGeneration(ProviderImageGenerationV1 {
                status: ProviderImageGenerationStatusV1::InProgress,
                revised_prompt: None,
                saved_path: None,
            }),
        },
    );
    assert_eq!(
        completed.validate(0),
        Err(ProviderItemValidationError::CompletionStatusInProgress)
    );
}
