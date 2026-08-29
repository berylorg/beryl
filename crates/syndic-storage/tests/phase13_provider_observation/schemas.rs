use super::*;

fn enum_value<C: ProviderObservationStageCallback>(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    value: ProviderEnumValue,
    callback: &mut C,
) -> Result<(), ProviderObservationStagingError> {
    stager
        .control(
            ProviderObservationControl::Enum {
                context: ProviderValueContext::Field(field),
                value,
            },
            callback,
        )
        .map(clean_stage)
}

fn empty_container<C: ProviderObservationStageCallback>(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    container: ProviderContainer,
    callback: &mut C,
) -> Result<(), ProviderObservationStagingError> {
    let context = ProviderValueContext::Field(field);
    clean_stage(stager.control(
        ProviderObservationControl::BeginContainer { context, container },
        callback,
    )?);
    stager
        .control(
            ProviderObservationControl::EndContainer { context, container },
            callback,
        )
        .map(clean_stage)
}

fn required_item<C: ProviderObservationStageCallback>(
    stager: &mut ProviderObservationStager,
    kind: ProviderObservationItemKind,
    callback: &mut C,
) -> Result<(), ProviderObservationStagingError> {
    use ProviderObservationItemKind as I;
    match kind {
        I::HookPrompt => empty_container(
            stager,
            ProviderField::HookFragments,
            ProviderContainer::List,
            callback,
        )?,
        I::AgentMessage => text(
            stager,
            ProviderField::AgentMessageText,
            &[b"message"],
            callback,
        )?,
        I::Plan => text(stager, ProviderField::PlanText, &[b"plan"], callback)?,
        I::Reasoning | I::ContextCompaction => {}
        I::CommandExecution => {
            text(stager, ProviderField::Command, &[b"command"], callback)?;
            text(stager, ProviderField::WorkingDirectory, &[b"cwd"], callback)?;
            enum_value(
                stager,
                ProviderField::CommandStatus,
                ProviderEnumValue::Completed,
                callback,
            )?;
            empty_container(
                stager,
                ProviderField::CommandActions,
                ProviderContainer::List,
                callback,
            )?;
        }
        I::FileChange => {
            enum_value(
                stager,
                ProviderField::FileChangeStatus,
                ProviderEnumValue::Completed,
                callback,
            )?;
            empty_container(
                stager,
                ProviderField::FileChanges,
                ProviderContainer::List,
                callback,
            )?;
        }
        I::McpToolCall => {
            text(stager, ProviderField::McpServer, &[b"server"], callback)?;
            text(stager, ProviderField::McpTool, &[b"tool"], callback)?;
            enum_value(
                stager,
                ProviderField::McpStatus,
                ProviderEnumValue::Completed,
                callback,
            )?;
            scalar(
                stager,
                ProviderField::McpArguments,
                ProviderScalar::Null,
                callback,
            )?;
        }
        I::DynamicToolCall => {
            text(stager, ProviderField::DynamicTool, &[b"tool"], callback)?;
            scalar(
                stager,
                ProviderField::DynamicArguments,
                ProviderScalar::Null,
                callback,
            )?;
            enum_value(
                stager,
                ProviderField::DynamicStatus,
                ProviderEnumValue::Completed,
                callback,
            )?;
        }
        I::CollabAgentToolCall => {
            enum_value(
                stager,
                ProviderField::CollabTool,
                ProviderEnumValue::Wait,
                callback,
            )?;
            enum_value(
                stager,
                ProviderField::CollabStatus,
                ProviderEnumValue::Completed,
                callback,
            )?;
            text(
                stager,
                ProviderField::CollabSenderThreadId,
                &[b"sender"],
                callback,
            )?;
            empty_container(
                stager,
                ProviderField::CollabReceiverThreadIds,
                ProviderContainer::List,
                callback,
            )?;
            empty_container(
                stager,
                ProviderField::CollabAgentStates,
                ProviderContainer::Object,
                callback,
            )?;
        }
        I::SubAgentActivity => {
            enum_value(
                stager,
                ProviderField::SubAgentKind,
                ProviderEnumValue::SubAgentStarted,
                callback,
            )?;
            text(
                stager,
                ProviderField::SubAgentThreadId,
                &[b"thread"],
                callback,
            )?;
            text(stager, ProviderField::SubAgentPath, &[b"path"], callback)?;
        }
        I::WebSearch => text(stager, ProviderField::WebSearchQuery, &[b"query"], callback)?,
        I::ImageView => text(stager, ProviderField::ImageViewPath, &[b"path"], callback)?,
        I::Sleep => scalar(
            stager,
            ProviderField::SleepDurationMs,
            ProviderScalar::Unsigned(1),
            callback,
        )?,
        I::StandaloneImageGeneration => enum_value(
            stager,
            ProviderField::ImageGenerationStatus,
            ProviderEnumValue::Completed,
            callback,
        )?,
        I::EnteredReviewMode => text(stager, ProviderField::EnteredReview, &[b"review"], callback)?,
        I::ExitedReviewMode => text(stager, ProviderField::ExitedReview, &[b"review"], callback)?,
    }
    Ok(())
}

#[test]
fn every_item_and_delta_schema_seals_through_durable_staging() {
    let home = TestHome::new("provider-observation-schemas");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let item_kinds = [
        ProviderObservationItemKind::HookPrompt,
        ProviderObservationItemKind::AgentMessage,
        ProviderObservationItemKind::Plan,
        ProviderObservationItemKind::Reasoning,
        ProviderObservationItemKind::CommandExecution,
        ProviderObservationItemKind::FileChange,
        ProviderObservationItemKind::McpToolCall,
        ProviderObservationItemKind::DynamicToolCall,
        ProviderObservationItemKind::CollabAgentToolCall,
        ProviderObservationItemKind::SubAgentActivity,
        ProviderObservationItemKind::WebSearch,
        ProviderObservationItemKind::ImageView,
        ProviderObservationItemKind::Sleep,
        ProviderObservationItemKind::StandaloneImageGeneration,
        ProviderObservationItemKind::EnteredReviewMode,
        ProviderObservationItemKind::ExitedReviewMode,
        ProviderObservationItemKind::ContextCompaction,
    ];
    let mut identity_byte = 10_u8;
    for lifecycle in [
        ProviderObservationItemLifecycle::Started,
        ProviderObservationItemLifecycle::Completed,
    ] {
        for kind in item_kinds {
            if lifecycle == ProviderObservationItemLifecycle::Started
                && kind == ProviderObservationItemKind::SubAgentActivity
            {
                continue;
            }
            let mut callback = commit_callback(&store, &storage);
            let mut stager = clean_stage(
                ProviderObservationStager::begin(
                    ProviderObservationId::from_bytes([identity_byte; 16]),
                    ProviderObservationBegin::Item { lifecycle, kind },
                    &mut callback,
                )
                .unwrap(),
            );
            common_item(&mut stager, &mut callback).unwrap();
            required_item(&mut stager, kind, &mut callback).unwrap();
            clean_seal(stager.seal(&mut callback).unwrap()).abandon();
            identity_byte += 1;
        }
    }

    let deltas = [
        ProviderDeltaKind::AgentMessage,
        ProviderDeltaKind::Plan,
        ProviderDeltaKind::ReasoningSummaryPartAdded,
        ProviderDeltaKind::ReasoningSummaryText,
        ProviderDeltaKind::ReasoningTextObserved,
        ProviderDeltaKind::CommandExecutionOutput,
        ProviderDeltaKind::FileChangeOutput,
        ProviderDeltaKind::FileChangePatchUpdated,
        ProviderDeltaKind::McpToolCallProgress,
    ];
    for kind in deltas {
        let mut callback = commit_callback(&store, &storage);
        let mut stager = clean_stage(
            ProviderObservationStager::begin(
                ProviderObservationId::from_bytes([identity_byte; 16]),
                ProviderObservationBegin::Delta { kind },
                &mut callback,
            )
            .unwrap(),
        );
        text(
            &mut stager,
            ProviderField::ItemId,
            &[b"delta-item"],
            &mut callback,
        )
        .unwrap();
        match kind {
            ProviderDeltaKind::AgentMessage
            | ProviderDeltaKind::Plan
            | ProviderDeltaKind::CommandExecutionOutput
            | ProviderDeltaKind::FileChangeOutput => {
                text(
                    &mut stager,
                    ProviderField::DeltaText,
                    &[b"delta"],
                    &mut callback,
                )
                .unwrap();
            }
            ProviderDeltaKind::ReasoningSummaryPartAdded => scalar(
                &mut stager,
                ProviderField::DeltaSummaryIndex,
                ProviderScalar::Unsigned(0),
                &mut callback,
            )
            .unwrap(),
            ProviderDeltaKind::ReasoningSummaryText => {
                scalar(
                    &mut stager,
                    ProviderField::DeltaSummaryIndex,
                    ProviderScalar::Unsigned(0),
                    &mut callback,
                )
                .unwrap();
                text(
                    &mut stager,
                    ProviderField::DeltaText,
                    &[b"summary"],
                    &mut callback,
                )
                .unwrap();
            }
            ProviderDeltaKind::ReasoningTextObserved => {
                scalar(
                    &mut stager,
                    ProviderField::DeltaContentIndex,
                    ProviderScalar::Unsigned(0),
                    &mut callback,
                )
                .unwrap();
            }
            ProviderDeltaKind::FileChangePatchUpdated => empty_container(
                &mut stager,
                ProviderField::DeltaChanges,
                ProviderContainer::List,
                &mut callback,
            )
            .unwrap(),
            ProviderDeltaKind::McpToolCallProgress => text(
                &mut stager,
                ProviderField::McpProgressMessage,
                &[b"progress"],
                &mut callback,
            )
            .unwrap(),
        }
        clean_seal(stager.seal(&mut callback).unwrap()).abandon();
        identity_byte += 1;
    }
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn web_search_other_survives_restart_and_seals_unsupported_history_evidence() {
    let home = TestHome::new("provider-observation-web-other");
    let identity = ProviderObservationId::from_bytes([73; 16]);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    {
        let mut callback = commit_callback(&store, &storage);
        let mut stager = clean_stage(
            ProviderObservationStager::begin(
                identity,
                ProviderObservationBegin::Item {
                    lifecycle: ProviderObservationItemLifecycle::Completed,
                    kind: ProviderObservationItemKind::WebSearch,
                },
                &mut callback,
            )
            .unwrap(),
        );
        common_item(&mut stager, &mut callback).unwrap();
        text(
            &mut stager,
            ProviderField::WebSearchQuery,
            &[b"query"],
            &mut callback,
        )
        .unwrap();
        let action = ProviderValueContext::Field(ProviderField::WebSearchAction);
        clean_stage(
            stager
                .control(
                    ProviderObservationControl::BeginContainer {
                        context: action,
                        container: ProviderContainer::Object,
                    },
                    &mut callback,
                )
                .unwrap(),
        );
        clean_stage(
            stager
                .control(
                    ProviderObservationControl::Enum {
                        context: ProviderValueContext::Field(ProviderField::WebSearchActionKind),
                        value: ProviderEnumValue::Other,
                    },
                    &mut callback,
                )
                .unwrap(),
        );
        assert_eq!(
            storage
                .provider_observation_build(&store, identity, limit())
                .unwrap()
                .unwrap()
                .history_support(),
            ProviderFrameHistorySupportV1::Unsupported(
                UnsupportedHistoryReason::UnsupportedRequiredPayload
            )
        );
        stager.abandon();
    }
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let mut stager = storage
        .resume_provider_observation(&reopened, identity, limit())
        .unwrap()
        .unwrap();
    let mut callback = commit_callback(&reopened, &storage);
    clean_stage(
        stager
            .control(
                ProviderObservationControl::EndContainer {
                    context: ProviderValueContext::Field(ProviderField::WebSearchAction),
                    container: ProviderContainer::Object,
                },
                &mut callback,
            )
            .unwrap(),
    );
    let sealed = clean_seal(stager.seal(&mut callback).unwrap());
    assert_eq!(
        sealed.history_support(),
        ProviderFrameHistorySupportV1::Unsupported(
            UnsupportedHistoryReason::UnsupportedRequiredPayload
        )
    );
    sealed.abandon();
    let reopened_handle = storage
        .reopen_provider_observation(&reopened, identity, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        reopened_handle.history_support(),
        ProviderFrameHistorySupportV1::Unsupported(
            UnsupportedHistoryReason::UnsupportedRequiredPayload
        )
    );
    reopened_handle.abandon();

    let mut callback = commit_callback(&reopened, &storage);
    let mut invalid =
        begin_agent(ProviderObservationId::from_bytes([74; 16]), &mut callback).unwrap();
    assert!(matches!(
        invalid.control(
            ProviderObservationControl::Enum {
                context: ProviderValueContext::Field(ProviderField::MessagePhase),
                value: ProviderEnumValue::Other,
            },
            &mut callback,
        ),
        Err(ProviderObservationStagingError::Validation(
            ProviderObservationValidatorError::OtherMarkerMismatch
        ))
    ));
    invalid.abandon();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}
