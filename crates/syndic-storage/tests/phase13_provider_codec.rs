use std::{convert::Infallible, io::Cursor, num::NonZeroU64};

use beryl_model::{
    AssetId, CasItemId, CasThreadId, ContentRevision, ImageLabelOrdinal, SyndicContentDigest,
    SyndicContentId,
};
use syndic_storage::*;

#[derive(Default)]
struct CollectSink {
    bytes: Vec<u8>,
    spans: Vec<ProviderFrameTextSpanV1>,
    largest_chunk: usize,
}

impl ProviderFrameSinkV1 for CollectSink {
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

#[derive(Default)]
struct SpanSink(Vec<ProviderFrameTextSpanV1>);

impl ProviderFrameTextSpanSinkV1 for SpanSink {
    type Error = Infallible;

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        self.0.push(span);
        Ok(())
    }
}

fn text(value: &str) -> ProviderTextV1 {
    ProviderTextV1::inline(value)
}

fn asset(value: u8) -> ProviderInlineImageAssetV1 {
    ProviderInlineImageAssetV1::new(AssetId::sha256_v1(
        [value; 32],
        NonZeroU64::new(u64::from(value)).unwrap(),
    ))
}

fn mcp_inline_metadata_item(value: ProviderStructuredValueV1) -> ProviderItemV1 {
    let image = ProviderMcpInlineImageV1::new(
        asset(12),
        vec![ProviderObjectEntryV1 {
            key: "nesting".to_owned(),
            value,
        }],
    )
    .unwrap();
    ProviderItemV1::McpToolCall(ProviderMcpToolCallV1 {
        server: text("server"),
        tool: text("tool"),
        status: ProviderToolCallStatusV1::Completed,
        arguments: ProviderStructuredValueV1::Null,
        app_context: None,
        mcp_app_resource_uri: None,
        plugin_id: None,
        result: Some(ProviderMcpResultV1 {
            content: vec![ProviderMcpContentV1::inline_image(image)],
            structured_content: None,
            meta: None,
        }),
        error: None,
        duration_ms: None,
    })
}

fn submitted_content() -> ContentReference {
    submitted_content_with_markers(6, Some(ImageLabelOrdinal::new(37).unwrap()))
}

fn submitted_content_with_markers(
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

fn rich_items() -> Vec<ProviderItemV1> {
    let safe_metadata = vec![ProviderObjectEntryV1 {
        key: "alt".to_owned(),
        value: ProviderStructuredValueV1::String(text("data:image/png;base64,opaque")),
    }];
    let inline_image = ProviderMcpInlineImageV1::new(asset(9), safe_metadata).unwrap();
    let ordinary_mcp = ProviderMcpContentV1::structured(ProviderStructuredValueV1::Object(vec![
        ProviderObjectEntryV1 {
            key: "type".to_owned(),
            value: ProviderStructuredValueV1::String(text("text")),
        },
        ProviderObjectEntryV1 {
            key: "body".to_owned(),
            value: ProviderStructuredValueV1::String(text("data:image/jpeg;base64,still opaque")),
        },
    ]))
    .unwrap();
    let nested_type_value =
        ProviderMcpContentV1::structured(ProviderStructuredValueV1::Object(vec![
            ProviderObjectEntryV1 {
                key: "type".to_owned(),
                value: ProviderStructuredValueV1::Object(vec![ProviderObjectEntryV1 {
                    key: "type".to_owned(),
                    value: ProviderStructuredValueV1::String(text("image")),
                }]),
            },
        ]))
        .unwrap();
    vec![
        ProviderItemV1::UserMessage(ProviderUserMessageV1 {
            client_id: Some(text("client")),
            submitted: ProviderSubmittedContentV1 {
                content: submitted_content(),
            },
        }),
        ProviderItemV1::HookPrompt(ProviderHookPromptV1 {
            fragments: vec![ProviderHookPromptFragmentV1 {
                text: text("hook text"),
                hook_run_id: text("hook-id"),
            }],
        }),
        ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
            text: text("agent answer"),
            phase: Some(ProviderMessagePhaseV1::FinalAnswer),
            memory_citation: Some(ProviderMemoryCitationV1 {
                entries: vec![ProviderMemoryCitationEntryV1 {
                    path: text("memory.md"),
                    line_start: 4,
                    line_end: 8,
                    note: text("note"),
                }],
                thread_ids: vec![text("thread-1")],
            }),
        }),
        ProviderItemV1::Plan(ProviderPlanV1 { text: text("plan") }),
        ProviderItemV1::Reasoning(ProviderReasoningV1 {
            summary: vec![text("reasoning")],
        }),
        ProviderItemV1::CommandExecution(ProviderCommandExecutionV1 {
            command: text("cargo check"),
            cwd: text("C:/repo"),
            process_id: Some(text("42")),
            source: ProviderCommandSourceV1::UnifiedExecInteraction,
            status: ProviderCommandStatusV1::Completed,
            command_actions: vec![ProviderCommandActionV1::Search {
                command: text("rg needle"),
                query: Some(text("needle")),
                path: Some(text("src")),
            }],
            aggregated_output: Some(text("ok")),
            exit_code: Some(0),
            duration_ms: Some(12),
        }),
        ProviderItemV1::FileChange(ProviderFileChangeV1 {
            status: ProviderPatchStatusV1::Completed,
            changes: vec![ProviderFileUpdateChangeV1 {
                path: text("new.rs"),
                diff: text("+new"),
                kind: ProviderPatchChangeKindV1::Update {
                    move_path: Some(text("old.rs")),
                },
            }],
        }),
        ProviderItemV1::McpToolCall(ProviderMcpToolCallV1 {
            server: text("server"),
            tool: text("tool"),
            status: ProviderToolCallStatusV1::Completed,
            arguments: ProviderStructuredValueV1::List(vec![
                ProviderStructuredValueV1::Number(ProviderNumberV1::Signed(-7)),
                ProviderStructuredValueV1::Number(ProviderNumberV1::Unsigned(9)),
                ProviderStructuredValueV1::Number(ProviderNumberV1::FiniteFloat(
                    ProviderFiniteF64V1::new(1.25).unwrap(),
                )),
            ]),
            app_context: Some(ProviderMcpAppContextV1 {
                connector_id: text("connector"),
                link_id: Some(text("link")),
                resource_uri: Some(text("resource://one")),
                app_name: Some(text("app")),
                template_id: Some(text("template")),
                action_name: Some(text("action")),
            }),
            mcp_app_resource_uri: Some(text("resource://app")),
            plugin_id: Some(text("plugin")),
            result: Some(ProviderMcpResultV1 {
                content: vec![
                    ordinary_mcp,
                    nested_type_value,
                    ProviderMcpContentV1::inline_image(inline_image),
                ],
                structured_content: Some(ProviderStructuredValueV1::Boolean(true)),
                meta: Some(ProviderStructuredValueV1::Null),
            }),
            error: Some(ProviderMcpErrorV1 {
                message: text("diagnostic"),
            }),
            duration_ms: Some(13),
        }),
        ProviderItemV1::DynamicToolCall(ProviderDynamicToolCallV1 {
            namespace: Some(text("namespace")),
            tool: text("dynamic"),
            arguments: ProviderStructuredValueV1::Object(vec![ProviderObjectEntryV1 {
                key: "value".to_owned(),
                value: ProviderStructuredValueV1::String(text("opaque data:image/png;base64,x")),
            }]),
            status: ProviderToolCallStatusV1::Completed,
            content_items: Some(vec![
                ProviderDynamicToolOutputV1::InputText {
                    text: text("output"),
                },
                ProviderDynamicToolOutputV1::InputImageLocator {
                    locator: ProviderImageLocatorV1::new("https://example.test/image.png").unwrap(),
                },
                ProviderDynamicToolOutputV1::InputImageAsset { asset: asset(10) },
            ]),
            success: Some(true),
            duration_ms: Some(14),
        }),
        ProviderItemV1::CollabAgentToolCall(ProviderCollabAgentToolCallV1 {
            tool: ProviderCollabToolV1::SpawnAgent,
            status: ProviderCollabToolStatusV1::Completed,
            sender_thread_id: CasThreadId::new("sender").unwrap(),
            receiver_thread_ids: vec![CasThreadId::new("receiver").unwrap()],
            prompt: Some(text("prompt")),
            model: Some(text("model")),
            reasoning_effort: Some(text("high")),
            agents_states: vec![ProviderCollabAgentStateEntryV1 {
                agent: text("worker"),
                state: ProviderCollabAgentStateV1 {
                    status: ProviderCollabAgentStatusV1::Completed,
                    message: Some(text("done")),
                },
            }],
        }),
        ProviderItemV1::SubAgentActivity(ProviderSubAgentActivityV1 {
            kind: ProviderSubAgentActivityKindV1::Interacted,
            agent_thread_id: CasThreadId::new("agent").unwrap(),
            agent_path: text("root/agent"),
        }),
        ProviderItemV1::WebSearch(ProviderWebSearchV1 {
            query: text("query"),
            action: Some(ProviderWebSearchActionV1::FindInPage {
                url: Some(text("https://example.test")),
                pattern: Some(text("pattern")),
            }),
        }),
        ProviderItemV1::ImageView(ProviderImageViewV1 {
            path: text("image.png"),
        }),
        ProviderItemV1::Sleep(ProviderSleepV1 { duration_ms: 25 }),
        ProviderItemV1::StandaloneImageGeneration(ProviderImageGenerationV1 {
            status: ProviderImageGenerationStatusV1::Completed,
            revised_prompt: Some(text("revised")),
            saved_path: Some(text("saved.png")),
        }),
        ProviderItemV1::EnteredReviewMode(ProviderEnteredReviewModeV1 {
            review: text("entered"),
        }),
        ProviderItemV1::ExitedReviewMode(ProviderExitedReviewModeV1 {
            review: text("exited"),
        }),
        ProviderItemV1::ContextCompaction,
    ]
}

fn deltas() -> Vec<ProviderItemDeltaV1> {
    vec![
        ProviderItemDeltaV1::AgentMessage { delta: text("a") },
        ProviderItemDeltaV1::Plan { delta: text("p") },
        ProviderItemDeltaV1::ReasoningSummaryPartAdded { summary_index: 2 },
        ProviderItemDeltaV1::ReasoningSummaryText {
            summary_index: 2,
            delta: text("r"),
        },
        ProviderItemDeltaV1::ReasoningTextObserved { content_index: 3 },
        ProviderItemDeltaV1::CommandExecutionOutput { delta: text("c") },
        ProviderItemDeltaV1::FileChangeOutput { delta: text("f") },
        ProviderItemDeltaV1::FileChangePatchUpdated {
            changes: vec![ProviderFileUpdateChangeV1 {
                path: text("file"),
                diff: text("diff"),
                kind: ProviderPatchChangeKindV1::Add,
            }],
        },
        ProviderItemDeltaV1::McpToolCallProgress { message: text("m") },
    ]
}

fn assert_round_trip(observation: ProviderItemObservationV1) {
    let frame = ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        CasItemId::new("provider-item").unwrap(),
        observation,
    );
    let mut encoded = CollectSink::default();
    let reference = encode_provider_item_frame_v1(&frame, 100, &mut encoded).unwrap();
    assert!(encoded.largest_chunk <= PROVIDER_FRAME_CHUNK_MAX_BYTES);
    assert_eq!(reference.encoded_len(), encoded.bytes.len() as u64);
    assert_eq!(
        decode_bounded_provider_item_frame_v1(
            &encoded.bytes,
            PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES,
            100,
        )
        .unwrap(),
        frame
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
    assert_eq!(structural.reference(), &reference);
    assert_eq!(structural.history_support(), frame.history_support());
    assert_eq!(spans.0, encoded.spans);
}

#[test]
fn all_pinned_item_families_round_trip_through_both_decoders() {
    let items = rich_items();
    assert_eq!(items.len(), 18);
    for item in items {
        assert_round_trip(ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(44),
            item,
        });
    }
}

#[test]
fn every_delta_family_round_trips() {
    let values = deltas();
    assert_eq!(values.len(), 9);
    for delta in values {
        assert_round_trip(ProviderItemObservationV1::Delta(delta));
    }
}

#[path = "phase13_provider_codec/content_reference.rs"]
mod content_reference;
#[path = "phase13_provider_codec/validation.rs"]
mod validation;
