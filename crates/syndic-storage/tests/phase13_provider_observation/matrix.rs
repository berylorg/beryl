use super::*;

pub(super) const ITEMS: [ProviderObservationItemKind; 17] = [
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

pub(super) const DELTAS: [ProviderDeltaKind; 9] = [
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

fn begin_item(
    byte: u8,
    kind: ProviderObservationItemKind,
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationStager {
    ProviderObservationStager::begin(
        ProviderObservationId::from_bytes([byte; 16]),
        ProviderObservationBegin::Item {
            lifecycle: ProviderObservationItemLifecycle::Completed,
            kind,
        },
        callback,
    )
    .unwrap()
}

fn begin_delta(
    byte: u8,
    kind: ProviderDeltaKind,
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationStager {
    ProviderObservationStager::begin(
        ProviderObservationId::from_bytes([byte; 16]),
        ProviderObservationBegin::Delta { kind },
        callback,
    )
    .unwrap()
}

fn validation_error(
    result: Result<(), ProviderObservationStagingError>,
) -> ProviderObservationValidatorError {
    match result {
        Err(ProviderObservationStagingError::Validation(error)) => error,
        Err(error) => panic!("unexpected staging error: {error}"),
        Ok(()) => panic!("invalid schema control was accepted"),
    }
}

fn wrong_scalar(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationValidatorError {
    validation_error(scalar(stager, field, ProviderScalar::Signed(-1), callback))
}

fn wrong_enum(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationValidatorError {
    validation_error(stager.control(
        ProviderObservationControl::Enum {
            context: ProviderValueContext::Field(field),
            value: ProviderEnumValue::Commentary,
        },
        callback,
    ))
}

fn wrong_item_value(
    stager: &mut ProviderObservationStager,
    kind: ProviderObservationItemKind,
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationValidatorError {
    match kind {
        ProviderObservationItemKind::HookPrompt => {
            wrong_scalar(stager, ProviderField::HookFragments, callback)
        }
        ProviderObservationItemKind::AgentMessage => {
            wrong_scalar(stager, ProviderField::AgentMessageText, callback)
        }
        ProviderObservationItemKind::Plan => {
            wrong_scalar(stager, ProviderField::PlanText, callback)
        }
        ProviderObservationItemKind::Reasoning => {
            wrong_scalar(stager, ProviderField::ReasoningSummaries, callback)
        }
        ProviderObservationItemKind::CommandExecution => {
            wrong_scalar(stager, ProviderField::Command, callback)
        }
        ProviderObservationItemKind::FileChange => {
            wrong_scalar(stager, ProviderField::FileChanges, callback)
        }
        ProviderObservationItemKind::McpToolCall => {
            wrong_enum(stager, ProviderField::McpArguments, callback)
        }
        ProviderObservationItemKind::DynamicToolCall => {
            wrong_enum(stager, ProviderField::DynamicArguments, callback)
        }
        ProviderObservationItemKind::CollabAgentToolCall => {
            wrong_scalar(stager, ProviderField::CollabAgentStates, callback)
        }
        ProviderObservationItemKind::SubAgentActivity => {
            wrong_enum(stager, ProviderField::SubAgentKind, callback)
        }
        ProviderObservationItemKind::WebSearch => {
            wrong_scalar(stager, ProviderField::WebSearchQuery, callback)
        }
        ProviderObservationItemKind::ImageView => {
            wrong_scalar(stager, ProviderField::ImageViewPath, callback)
        }
        ProviderObservationItemKind::Sleep => {
            wrong_scalar(stager, ProviderField::SleepDurationMs, callback)
        }
        ProviderObservationItemKind::StandaloneImageGeneration => {
            wrong_enum(stager, ProviderField::ImageGenerationStatus, callback)
        }
        ProviderObservationItemKind::EnteredReviewMode => {
            wrong_scalar(stager, ProviderField::EnteredReview, callback)
        }
        ProviderObservationItemKind::ExitedReviewMode => {
            wrong_scalar(stager, ProviderField::ExitedReview, callback)
        }
        ProviderObservationItemKind::ContextCompaction => {
            wrong_scalar(stager, ProviderField::ItemId, callback)
        }
    }
}

fn wrong_delta_value(
    stager: &mut ProviderObservationStager,
    kind: ProviderDeltaKind,
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationValidatorError {
    match kind {
        ProviderDeltaKind::AgentMessage
        | ProviderDeltaKind::Plan
        | ProviderDeltaKind::ReasoningSummaryText
        | ProviderDeltaKind::CommandExecutionOutput
        | ProviderDeltaKind::FileChangeOutput => {
            wrong_scalar(stager, ProviderField::DeltaText, callback)
        }
        ProviderDeltaKind::ReasoningSummaryPartAdded => {
            wrong_scalar(stager, ProviderField::DeltaSummaryIndex, callback)
        }
        ProviderDeltaKind::ReasoningTextObserved => {
            wrong_scalar(stager, ProviderField::DeltaContentIndex, callback)
        }
        ProviderDeltaKind::FileChangePatchUpdated => {
            wrong_scalar(stager, ProviderField::DeltaChanges, callback)
        }
        ProviderDeltaKind::McpToolCallProgress => {
            wrong_scalar(stager, ProviderField::McpProgressMessage, callback)
        }
    }
}

#[test]
fn every_item_schema_rejects_a_schema_specific_type_or_shape_substitution() {
    let home = TestHome::new("provider-observation-item-negative-matrix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, storage);
    for (index, kind) in ITEMS.into_iter().enumerate() {
        let byte = 210 + index as u8;
        let identity = ProviderObservationId::from_bytes([byte; 16]);
        let mut stager = begin_item(byte, kind, &mut callback);
        if kind == ProviderObservationItemKind::ContextCompaction {
            scalar(
                &mut stager,
                ProviderField::LifecycleObservedAt,
                ProviderScalar::Unsigned(1),
                &mut callback,
            )
            .unwrap();
        } else {
            common_item(&mut stager, &mut callback).unwrap();
        }
        let before = storage
            .provider_observation_build(&store, identity, limit())
            .unwrap()
            .unwrap()
            .clone();
        assert!(matches!(
            wrong_item_value(&mut stager, kind, &mut callback),
            ProviderObservationValidatorError::ValueMismatch
                | ProviderObservationValidatorError::EnumMismatch
        ));
        assert_eq!(
            storage
                .provider_observation_build(&store, identity, limit())
                .unwrap()
                .unwrap(),
            before
        );
        assert_eq!(
            before.lifecycle(),
            ProviderObservationBuildLifecycle::Building
        );
        stager.abandon();
    }
    drop(callback);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn every_delta_schema_rejects_a_schema_specific_type_or_shape_substitution() {
    let home = TestHome::new("provider-observation-delta-negative-matrix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, storage);
    for (index, kind) in DELTAS.into_iter().enumerate() {
        let byte = 230 + index as u8;
        let identity = ProviderObservationId::from_bytes([byte; 16]);
        let mut stager = begin_delta(byte, kind, &mut callback);
        text(
            &mut stager,
            ProviderField::ItemId,
            &[b"item"],
            &mut callback,
        )
        .unwrap();
        let before = storage
            .provider_observation_build(&store, identity, limit())
            .unwrap()
            .unwrap()
            .clone();
        assert_eq!(
            wrong_delta_value(&mut stager, kind, &mut callback),
            ProviderObservationValidatorError::ValueMismatch
        );
        assert_eq!(
            storage
                .provider_observation_build(&store, identity, limit())
                .unwrap()
                .unwrap(),
            before
        );
        assert_eq!(
            before.lifecycle(),
            ProviderObservationBuildLifecycle::Building
        );
        stager.abandon();
    }
    drop(callback);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn every_item_and_delta_schema_rejects_duplicate_identity_without_advancing_state() {
    let home = TestHome::new("provider-observation-duplicate-matrix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, storage);
    for (index, kind) in ITEMS.into_iter().enumerate() {
        let byte = 10 + index as u8;
        let identity = ProviderObservationId::from_bytes([byte; 16]);
        let mut stager = begin_item(byte, kind, &mut callback);
        common_item(&mut stager, &mut callback).unwrap();
        let before = storage
            .provider_observation_build(&store, identity, limit())
            .unwrap()
            .unwrap()
            .clone();
        assert_eq!(
            validation_error(stager.control(
                ProviderObservationControl::BeginField(ProviderValueContext::Field(
                    ProviderField::ItemId,
                )),
                &mut callback,
            )),
            ProviderObservationValidatorError::DuplicateField
        );
        assert_eq!(
            storage
                .provider_observation_build(&store, identity, limit())
                .unwrap()
                .unwrap(),
            before
        );
        stager.abandon();
    }
    for (index, kind) in DELTAS.into_iter().enumerate() {
        let byte = 30 + index as u8;
        let identity = ProviderObservationId::from_bytes([byte; 16]);
        let mut stager = begin_delta(byte, kind, &mut callback);
        text(
            &mut stager,
            ProviderField::ItemId,
            &[b"item"],
            &mut callback,
        )
        .unwrap();
        let before = storage
            .provider_observation_build(&store, identity, limit())
            .unwrap()
            .unwrap()
            .clone();
        assert_eq!(
            validation_error(stager.control(
                ProviderObservationControl::BeginField(ProviderValueContext::Field(
                    ProviderField::ItemId,
                )),
                &mut callback,
            )),
            ProviderObservationValidatorError::DuplicateField
        );
        assert_eq!(
            storage
                .provider_observation_build(&store, identity, limit())
                .unwrap()
                .unwrap(),
            before
        );
        stager.abandon();
    }
    drop(callback);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn every_item_and_delta_schema_rejects_missing_required_state_without_sealing() {
    let home = TestHome::new("provider-observation-missing-matrix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    for (index, kind) in ITEMS.into_iter().enumerate() {
        let byte = 50 + index as u8;
        let identity = ProviderObservationId::from_bytes([byte; 16]);
        let mut callback = commit_callback(&store, storage);
        let mut stager = begin_item(byte, kind, &mut callback);
        let expected = if matches!(
            kind,
            ProviderObservationItemKind::Reasoning | ProviderObservationItemKind::ContextCompaction
        ) {
            scalar(
                &mut stager,
                ProviderField::LifecycleObservedAt,
                ProviderScalar::Unsigned(1),
                &mut callback,
            )
            .unwrap();
            ProviderObservationValidatorError::MissingItemIdentity
        } else {
            common_item(&mut stager, &mut callback).unwrap();
            ProviderObservationValidatorError::MissingRequiredField
        };
        assert!(matches!(
            stager.seal(&mut callback),
            Err(ProviderObservationStagingError::Validation(error)) if error == expected
        ));
        drop(callback);
        assert_eq!(
            storage
                .provider_observation_build(&store, identity, limit())
                .unwrap()
                .unwrap()
                .lifecycle(),
            ProviderObservationBuildLifecycle::Building
        );
    }
    for (index, kind) in DELTAS.into_iter().enumerate() {
        let byte = 70 + index as u8;
        let identity = ProviderObservationId::from_bytes([byte; 16]);
        let mut callback = commit_callback(&store, storage);
        let mut stager = begin_delta(byte, kind, &mut callback);
        text(
            &mut stager,
            ProviderField::ItemId,
            &[b"item"],
            &mut callback,
        )
        .unwrap();
        assert!(matches!(
            stager.seal(&mut callback),
            Err(ProviderObservationStagingError::Validation(
                ProviderObservationValidatorError::MissingRequiredField
            ))
        ));
        drop(callback);
        assert_eq!(
            storage
                .provider_observation_build(&store, identity, limit())
                .unwrap()
                .unwrap()
                .lifecycle(),
            ProviderObservationBuildLifecycle::Building
        );
    }
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
