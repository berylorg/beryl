use super::*;

pub(super) fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub(super) fn execute(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> beryl_home_store::CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

pub(super) fn assert_committed(outcome: beryl_home_store::CommandOutcome) {
    match outcome {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("unexpected live-history command outcome: {outcome:?}"),
    }
}

pub(super) fn provider_timestamp(at: SyndicTimestamp) -> ProviderLifecycleTimestampMsV1 {
    ProviderLifecycleTimestampMsV1::new(at.unix_millis())
}

pub(super) fn agent_value(
    text: impl Into<String>,
    phase: Option<ProviderMessagePhaseV1>,
) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text),
        phase,
        memory_citation: None,
    })
}

pub(super) fn agent_start(
    cas_item: CasItemId,
    text: impl Into<String>,
    phase: Option<ProviderMessagePhaseV1>,
    observed_at: SyndicTimestamp,
) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        cas_item,
        ProviderItemObservationV1::Started {
            observed_at: provider_timestamp(observed_at),
            item: agent_value(text, phase),
        },
    )
}

pub(super) fn agent_delta(
    ordinal: ProviderFrameOrdinalV1,
    cas_item: CasItemId,
    text: impl Into<String>,
) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ordinal,
        cas_item,
        ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
            delta: ProviderTextV1::inline(text),
        }),
    )
}

pub(super) fn agent_completion(
    ordinal: ProviderFrameOrdinalV1,
    cas_item: CasItemId,
    text: impl Into<String>,
    phase: Option<ProviderMessagePhaseV1>,
    observed_at: SyndicTimestamp,
) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ordinal,
        cas_item,
        ProviderItemObservationV1::Completed {
            observed_at: provider_timestamp(observed_at),
            item: agent_value(text, phase),
        },
    )
}

pub(super) fn command_value(
    output: Option<impl Into<String>>,
    status: ProviderCommandStatusV1,
) -> ProviderItemV1 {
    let completed = status == ProviderCommandStatusV1::Completed;
    ProviderItemV1::CommandExecution(ProviderCommandExecutionV1 {
        command: ProviderTextV1::inline("cargo check"),
        cwd: ProviderTextV1::inline("C:/workspace"),
        process_id: None,
        source: ProviderCommandSourceV1::Agent,
        status,
        command_actions: Vec::new(),
        aggregated_output: output.map(ProviderTextV1::inline),
        exit_code: completed.then_some(0),
        duration_ms: completed.then_some(1),
    })
}

pub(super) fn command_start(
    cas_item: CasItemId,
    observed_at: SyndicTimestamp,
) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        cas_item,
        ProviderItemObservationV1::Started {
            observed_at: provider_timestamp(observed_at),
            item: command_value(None::<String>, ProviderCommandStatusV1::InProgress),
        },
    )
}

pub(super) fn image_generation_start(
    cas_item: CasItemId,
    observed_at: SyndicTimestamp,
) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        cas_item,
        ProviderItemObservationV1::Started {
            observed_at: provider_timestamp(observed_at),
            item: ProviderItemV1::StandaloneImageGeneration(ProviderImageGenerationV1 {
                status: ProviderImageGenerationStatusV1::InProgress,
                revised_prompt: None,
                saved_path: None,
            }),
        },
    )
}

pub(super) fn command_delta(
    ordinal: ProviderFrameOrdinalV1,
    cas_item: CasItemId,
    text: impl Into<String>,
) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ordinal,
        cas_item,
        ProviderItemObservationV1::Delta(ProviderItemDeltaV1::CommandExecutionOutput {
            delta: ProviderTextV1::inline(text),
        }),
    )
}

pub(super) fn command_completion(
    ordinal: ProviderFrameOrdinalV1,
    cas_item: CasItemId,
    output: impl Into<String>,
    observed_at: SyndicTimestamp,
) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ordinal,
        cas_item,
        ProviderItemObservationV1::Completed {
            observed_at: provider_timestamp(observed_at),
            item: command_value(Some(output), ProviderCommandStatusV1::Completed),
        },
    )
}

pub(super) fn typed_error(error: &CommandError) -> &SyndicMutationError {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected Syndic validation rejection, got {error}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}
