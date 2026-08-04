use super::*;

const STATUS_KINDS: [ProviderObservationItemKind; 6] = [
    ProviderObservationItemKind::CommandExecution,
    ProviderObservationItemKind::FileChange,
    ProviderObservationItemKind::McpToolCall,
    ProviderObservationItemKind::DynamicToolCall,
    ProviderObservationItemKind::CollabAgentToolCall,
    ProviderObservationItemKind::StandaloneImageGeneration,
];

fn status_field(kind: ProviderObservationItemKind) -> ProviderField {
    match kind {
        ProviderObservationItemKind::CommandExecution => ProviderField::CommandStatus,
        ProviderObservationItemKind::FileChange => ProviderField::FileChangeStatus,
        ProviderObservationItemKind::McpToolCall => ProviderField::McpStatus,
        ProviderObservationItemKind::DynamicToolCall => ProviderField::DynamicStatus,
        ProviderObservationItemKind::CollabAgentToolCall => ProviderField::CollabStatus,
        ProviderObservationItemKind::StandaloneImageGeneration => {
            ProviderField::ImageGenerationStatus
        }
        _ => unreachable!("status matrix is closed"),
    }
}

fn enum_value(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    value: ProviderEnumValue,
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) {
    stager
        .control(
            ProviderObservationControl::Enum {
                context: ProviderValueContext::Field(field),
                value,
            },
            callback,
        )
        .unwrap();
}

fn empty_container(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    container: ProviderContainer,
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) {
    let context = ProviderValueContext::Field(field);
    stager
        .control(
            ProviderObservationControl::BeginContainer { context, container },
            callback,
        )
        .unwrap();
    stager
        .control(
            ProviderObservationControl::EndContainer { context, container },
            callback,
        )
        .unwrap();
}

fn required_except_status(
    stager: &mut ProviderObservationStager,
    kind: ProviderObservationItemKind,
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) {
    common_item(stager, callback).unwrap();
    match kind {
        ProviderObservationItemKind::CommandExecution => {
            text(stager, ProviderField::Command, &[b"command"], callback).unwrap();
            text(stager, ProviderField::WorkingDirectory, &[b"cwd"], callback).unwrap();
            empty_container(
                stager,
                ProviderField::CommandActions,
                ProviderContainer::List,
                callback,
            );
        }
        ProviderObservationItemKind::FileChange => empty_container(
            stager,
            ProviderField::FileChanges,
            ProviderContainer::List,
            callback,
        ),
        ProviderObservationItemKind::McpToolCall => {
            text(stager, ProviderField::McpServer, &[b"server"], callback).unwrap();
            text(stager, ProviderField::McpTool, &[b"tool"], callback).unwrap();
            scalar(
                stager,
                ProviderField::McpArguments,
                ProviderScalar::Null,
                callback,
            )
            .unwrap();
        }
        ProviderObservationItemKind::DynamicToolCall => {
            text(stager, ProviderField::DynamicTool, &[b"tool"], callback).unwrap();
            scalar(
                stager,
                ProviderField::DynamicArguments,
                ProviderScalar::Null,
                callback,
            )
            .unwrap();
        }
        ProviderObservationItemKind::CollabAgentToolCall => {
            enum_value(
                stager,
                ProviderField::CollabTool,
                ProviderEnumValue::Wait,
                callback,
            );
            text(
                stager,
                ProviderField::CollabSenderThreadId,
                &[b"sender"],
                callback,
            )
            .unwrap();
            empty_container(
                stager,
                ProviderField::CollabReceiverThreadIds,
                ProviderContainer::List,
                callback,
            );
            empty_container(
                stager,
                ProviderField::CollabAgentStates,
                ProviderContainer::Object,
                callback,
            );
        }
        ProviderObservationItemKind::StandaloneImageGeneration => {}
        _ => unreachable!("status matrix is closed"),
    }
}

fn begin(
    byte: u8,
    lifecycle: ProviderObservationItemLifecycle,
    kind: ProviderObservationItemKind,
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) -> ProviderObservationStager {
    ProviderObservationStager::begin(
        ProviderObservationId::from_bytes([byte; 16]),
        ProviderObservationBegin::Item { lifecycle, kind },
        callback,
    )
    .unwrap()
}

#[test]
fn all_status_bearing_kinds_accept_legal_started_and_completed_statuses() {
    let home = TestHome::new("provider-observation-status-positive");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, storage);
    for (index, kind) in STATUS_KINDS.into_iter().enumerate() {
        let mut started = begin(
            130 + index as u8,
            ProviderObservationItemLifecycle::Started,
            kind,
            &mut callback,
        );
        required_except_status(&mut started, kind, &mut callback);
        enum_value(
            &mut started,
            status_field(kind),
            ProviderEnumValue::InProgress,
            &mut callback,
        );
        started.seal(&mut callback).unwrap().abandon();

        let mut completed = begin(
            136 + index as u8,
            ProviderObservationItemLifecycle::Completed,
            kind,
            &mut callback,
        );
        enum_value(
            &mut completed,
            status_field(kind),
            ProviderEnumValue::Completed,
            &mut callback,
        );
        required_except_status(&mut completed, kind, &mut callback);
        completed.seal(&mut callback).unwrap().abandon();
    }
    drop(callback);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn completed_in_progress_status_is_rejected_for_all_six_kinds_after_restart() {
    let home = TestHome::new("provider-observation-status-restart-negative");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    {
        let mut callback = commit_callback(&store, storage);
        for (index, kind) in STATUS_KINDS.into_iter().enumerate() {
            let mut stager = begin(
                142 + index as u8,
                ProviderObservationItemLifecycle::Completed,
                kind,
                &mut callback,
            );
            enum_value(
                &mut stager,
                status_field(kind),
                ProviderEnumValue::InProgress,
                &mut callback,
            );
            stager.abandon();
        }
    }
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    for (index, kind) in STATUS_KINDS.into_iter().enumerate() {
        let identity = ProviderObservationId::from_bytes([142 + index as u8; 16]);
        let mut stager = storage
            .resume_provider_observation(&reopened, identity, limit())
            .unwrap()
            .unwrap();
        let mut callback = commit_callback(&reopened, storage);
        assert!(matches!(
            stager.control(
                ProviderObservationControl::Enum {
                    context: ProviderValueContext::Field(status_field(kind)),
                    value: ProviderEnumValue::Completed,
                },
                &mut callback,
            ),
            Err(ProviderObservationStagingError::Validation(
                ProviderObservationValidatorError::DuplicateField
            ))
        ));
        required_except_status(&mut stager, kind, &mut callback);
        assert!(matches!(
            stager.seal(&mut callback),
            Err(ProviderObservationStagingError::Validation(
                ProviderObservationValidatorError::InvalidLifecycle
            ))
        ));
        drop(callback);
        assert_eq!(
            storage
                .provider_observation_build(&reopened, identity, limit())
                .unwrap()
                .unwrap()
                .lifecycle(),
            ProviderObservationBuildLifecycle::Building
        );
    }
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}
