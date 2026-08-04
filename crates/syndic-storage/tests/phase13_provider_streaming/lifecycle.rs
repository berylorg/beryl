use super::*;

#[test]
fn lifecycle_state_resumes_many_deltas_and_completion_without_replay() {
    let started_at = ProviderLifecycleTimestampMsV1::new(100);
    let started = frame(
        1,
        ProviderItemObservationV1::Started {
            observed_at: started_at,
            item: agent(text("start")),
        },
    );
    let mut lifecycle = ProviderItemStreamValidatorV1::new();
    assert!(lifecycle.state().is_none());
    lifecycle.observe(&started).unwrap();

    for ordinal in 2..=129 {
        let state = lifecycle.state().unwrap().clone();
        assert_eq!(state.next_ordinal().get(), ordinal);
        let mut resumed = ProviderItemStreamValidatorV1::from_state(state);
        resumed
            .observe(&frame(
                ordinal,
                ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                    delta: text(format!("delta-{ordinal}")),
                }),
            ))
            .unwrap();
        lifecycle = resumed;
    }

    let state = lifecycle.state().unwrap();
    assert_eq!(state.item_id().as_str(), "streaming-item");
    assert_eq!(state.kind(), ProviderItemKind::AgentMessage);
    assert_eq!(state.next_ordinal().get(), 130);
    assert_eq!(state.started_at(), Some(started_at));
    assert!(!state.is_complete());
    assert_eq!(
        state.history_support(),
        ProviderFrameHistorySupportV1::Supported
    );

    let mut resumed = ProviderItemStreamValidatorV1::from_state(state.clone());
    resumed
        .observe(&frame(
            130,
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(101),
                item: agent(text("complete")),
            },
        ))
        .unwrap();
    let completed = resumed.state().unwrap();
    assert_eq!(completed.next_ordinal().get(), 131);
    assert_eq!(completed.started_at(), Some(started_at));
    assert!(completed.is_complete());
    assert!(resumed.is_history_complete());
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
    let completion_only_state = completion_only_lifecycle.state().unwrap();
    assert_eq!(
        completion_only_state.kind(),
        ProviderItemKind::SubAgentActivity
    );
    assert_eq!(completion_only_state.next_ordinal().get(), 2);
    assert_eq!(completion_only_state.started_at(), None);
    assert!(completion_only_state.is_complete());

    let mut resumed = ProviderItemStreamValidatorV1::from_state(completion_only_state.clone());
    assert_eq!(
        resumed.observe(&frame(
            2,
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(31),
                item: ProviderItemV1::SubAgentActivity(ProviderSubAgentActivityV1 {
                    kind: ProviderSubAgentActivityKindV1::Interacted,
                    agent_thread_id: beryl_model::CasThreadId::new("completion-only").unwrap(),
                    agent_path: text("root/worker"),
                }),
            },
        )),
        Err(ProviderItemValidationError::EventAfterCompletion)
    );
}

#[test]
fn lifecycle_state_constructor_rejects_corrupt_or_impossible_snapshots() {
    let item_id = CasItemId::new("state-item").unwrap();
    let started_at = Some(ProviderLifecycleTimestampMsV1::new(10));
    let supported = ProviderFrameHistorySupportV1::Supported;

    for actual in [0, 1] {
        assert_eq!(
            ProviderItemStreamStateV1::new(
                item_id.clone(),
                ProviderItemKind::AgentMessage,
                actual,
                started_at,
                false,
                supported,
            ),
            Err(ProviderItemValidationError::InvalidStreamStateOrdinal { actual })
        );
    }
    for (kind, next_ordinal, started_at, completed) in [
        (ProviderItemKind::AgentMessage, 2, None, false),
        (
            ProviderItemKind::AgentMessage,
            2,
            Some(ProviderLifecycleTimestampMsV1::new(10)),
            true,
        ),
        (ProviderItemKind::SubAgentActivity, 2, started_at, true),
        (ProviderItemKind::SubAgentActivity, 2, None, false),
        (ProviderItemKind::SubAgentActivity, 3, None, true),
    ] {
        assert_eq!(
            ProviderItemStreamStateV1::new(
                item_id.clone(),
                kind,
                next_ordinal,
                started_at,
                completed,
                supported,
            ),
            Err(ProviderItemValidationError::InvalidStreamStateLifecycle { kind })
        );
    }

    assert!(
        ProviderItemStreamStateV1::new(
            item_id.clone(),
            ProviderItemKind::AgentMessage,
            2,
            started_at,
            false,
            supported,
        )
        .is_ok()
    );
    assert!(
        ProviderItemStreamStateV1::new(
            item_id.clone(),
            ProviderItemKind::AgentMessage,
            3,
            started_at,
            true,
            supported,
        )
        .is_ok()
    );
    assert!(
        ProviderItemStreamStateV1::new(
            item_id,
            ProviderItemKind::SubAgentActivity,
            2,
            None,
            true,
            supported,
        )
        .is_ok()
    );
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
    let materialized_state = materialized.state().unwrap().clone();
    assert_eq!(materialized_state.history_support(), unsupported);
    let mut materialized = ProviderItemStreamValidatorV1::from_state(materialized_state);
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
        structural_lifecycle = ProviderItemStreamValidatorV1::from_state(
            structural_lifecycle.state().unwrap().clone(),
        );
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
