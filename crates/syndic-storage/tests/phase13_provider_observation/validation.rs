use super::*;

fn enum_control<C: ProviderObservationStageCallback>(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    value: ProviderEnumValue,
    callback: &mut C,
) -> Result<(), ProviderObservationStagingError> {
    stager.control(
        ProviderObservationControl::Enum {
            context: ProviderValueContext::Field(field),
            value,
        },
        callback,
    )
}

fn begin_item<C: ProviderObservationStageCallback>(
    byte: u8,
    lifecycle: ProviderObservationItemLifecycle,
    kind: ProviderObservationItemKind,
    callback: &mut C,
) -> Result<ProviderObservationStager, ProviderObservationStagingError> {
    let mut stager = ProviderObservationStager::begin(
        ProviderObservationId::from_bytes([byte; 16]),
        ProviderObservationBegin::Item { lifecycle, kind },
        callback,
    )?;
    common_item(&mut stager, callback)?;
    Ok(stager)
}

fn container<C: ProviderObservationStageCallback>(
    stager: &mut ProviderObservationStager,
    begin: bool,
    context: ProviderValueContext,
    kind: ProviderContainer,
    callback: &mut C,
) -> Result<(), ProviderObservationStagingError> {
    let control = if begin {
        ProviderObservationControl::BeginContainer {
            context,
            container: kind,
        }
    } else {
        ProviderObservationControl::EndContainer {
            context,
            container: kind,
        }
    };
    stager.control(control, callback)
}

fn element<C: ProviderObservationStageCallback>(
    stager: &mut ProviderObservationStager,
    begin: bool,
    context: ProviderValueContext,
    index: u64,
    callback: &mut C,
) -> Result<(), ProviderObservationStagingError> {
    let control = if begin {
        ProviderObservationControl::BeginElement { context, index }
    } else {
        ProviderObservationControl::EndElement { context, index }
    };
    stager.control(control, callback)
}

fn validation_error(result: Result<(), ProviderObservationStagingError>) -> ProviderObservationValidatorError {
    match result {
        Err(ProviderObservationStagingError::Validation(error)) => error,
        Err(error) => panic!("unexpected staging error: {error}"),
        Ok(()) => panic!("invalid control was accepted"),
    }
}

fn context_text<C: ProviderObservationStageCallback>(
    stager: &mut ProviderObservationStager,
    context: ProviderValueContext,
    bytes: &[u8],
    callback: &mut C,
) -> Result<(), ProviderObservationStagingError> {
    stager.control(ProviderObservationControl::BeginField(context), callback)?;
    stager.fragment(
        ProviderObservationStagingBytes::new(context, bytes).unwrap(),
        callback,
    )?;
    stager.control(ProviderObservationControl::EndField(context), callback)
}

#[test]
fn exact_scalar_enum_and_value_controls_reject_substitutions() {
    let home = TestHome::new("provider-observation-value-grammar");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, storage);

    let mut agent = ProviderObservationStager::begin(
        ProviderObservationId::from_bytes([80; 16]),
        ProviderObservationBegin::Item {
            lifecycle: ProviderObservationItemLifecycle::Completed,
            kind: ProviderObservationItemKind::AgentMessage,
        },
        &mut callback,
    )
    .unwrap();
    assert_eq!(
        validation_error(scalar(
            &mut agent,
            ProviderField::LifecycleObservedAt,
            ProviderScalar::Null,
            &mut callback,
        )),
        ProviderObservationValidatorError::ValueMismatch
    );
    scalar(
        &mut agent,
        ProviderField::LifecycleObservedAt,
        ProviderScalar::Unsigned(42),
        &mut callback,
    )
    .unwrap();
    text(
        &mut agent,
        ProviderField::ItemId,
        &[b"provider-item"],
        &mut callback,
    )
    .unwrap();
    assert_eq!(
        validation_error(scalar(
            &mut agent,
            ProviderField::AgentMessageText,
            ProviderScalar::Unsigned(1),
            &mut callback,
        )),
        ProviderObservationValidatorError::ValueMismatch
    );
    assert_eq!(
        validation_error(scalar(
            &mut agent,
            ProviderField::AgentMessageText,
            ProviderScalar::Null,
            &mut callback,
        )),
        ProviderObservationValidatorError::ValueMismatch
    );
    assert_eq!(
        validation_error(enum_control(
            &mut agent,
            ProviderField::MessagePhase,
            ProviderEnumValue::Completed,
            &mut callback,
        )),
        ProviderObservationValidatorError::EnumMismatch
    );
    text(
        &mut agent,
        ProviderField::AgentMessageText,
        &[b"valid after rejections"],
        &mut callback,
    )
    .unwrap();
    scalar(
        &mut agent,
        ProviderField::MessagePhase,
        ProviderScalar::Null,
        &mut callback,
    )
    .unwrap();
    agent.seal(&mut callback).unwrap().abandon();

    let mut sleep = begin_item(
        81,
        ProviderObservationItemLifecycle::Completed,
        ProviderObservationItemKind::Sleep,
        &mut callback,
    )
    .unwrap();
    assert_eq!(
        validation_error(text(
            &mut sleep,
            ProviderField::SleepDurationMs,
            &[b"1"],
            &mut callback,
        )),
        ProviderObservationValidatorError::ValueMismatch
    );
    assert_eq!(
        validation_error(scalar(
            &mut sleep,
            ProviderField::SleepDurationMs,
            ProviderScalar::Null,
            &mut callback,
        )),
        ProviderObservationValidatorError::ValueMismatch
    );
    scalar(
        &mut sleep,
        ProviderField::SleepDurationMs,
        ProviderScalar::Unsigned(1),
        &mut callback,
    )
    .unwrap();
    sleep.seal(&mut callback).unwrap().abandon();

    let mut hook = begin_item(
        82,
        ProviderObservationItemLifecycle::Completed,
        ProviderObservationItemKind::HookPrompt,
        &mut callback,
    )
    .unwrap();
    assert_eq!(
        validation_error(scalar(
            &mut hook,
            ProviderField::HookFragments,
            ProviderScalar::Null,
            &mut callback,
        )),
        ProviderObservationValidatorError::ValueMismatch
    );
    assert_eq!(
        validation_error(container(
            &mut hook,
            true,
            ProviderValueContext::Field(ProviderField::HookFragments),
            ProviderContainer::Object,
            &mut callback,
        )),
        ProviderObservationValidatorError::ValueMismatch
    );
    hook.abandon();
    drop(callback);
    store.close().unwrap();
}

#[test]
fn duplicates_and_other_placement_are_rejected_while_completion_only_start_seals() {
    let home = TestHome::new("provider-observation-conflicts");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, storage);

    let mut agent = begin_item(
        83,
        ProviderObservationItemLifecycle::Completed,
        ProviderObservationItemKind::AgentMessage,
        &mut callback,
    )
    .unwrap();
    text(
        &mut agent,
        ProviderField::AgentMessageText,
        &[b"once"],
        &mut callback,
    )
    .unwrap();
    assert_eq!(
        validation_error(text(
            &mut agent,
            ProviderField::AgentMessageText,
            &[b"twice"],
            &mut callback,
        )),
        ProviderObservationValidatorError::DuplicateField
    );
    assert_eq!(
        validation_error(enum_control(
            &mut agent,
            ProviderField::MessagePhase,
            ProviderEnumValue::Other,
            &mut callback,
        )),
        ProviderObservationValidatorError::OtherMarkerMismatch
    );
    agent.seal(&mut callback).unwrap().abandon();

    let mut subagent = begin_item(
        84,
        ProviderObservationItemLifecycle::Started,
        ProviderObservationItemKind::SubAgentActivity,
        &mut callback,
    )
    .unwrap();
    enum_control(
        &mut subagent,
        ProviderField::SubAgentKind,
        ProviderEnumValue::SubAgentStarted,
        &mut callback,
    )
    .unwrap();
    text(
        &mut subagent,
        ProviderField::SubAgentThreadId,
        &[b"thread"],
        &mut callback,
    )
    .unwrap();
    text(
        &mut subagent,
        ProviderField::SubAgentPath,
        &[b"path"],
        &mut callback,
    )
    .unwrap();
    let sealed = subagent.seal(&mut callback).unwrap();
    assert_eq!(
        sealed.begin(),
        ProviderObservationBegin::Item {
            lifecycle: ProviderObservationItemLifecycle::Started,
            kind: ProviderObservationItemKind::SubAgentActivity,
        }
    );
    sealed.abandon();
    drop(callback);
    store.close().unwrap();
}

#[test]
fn malformed_nesting_context_depth_and_indices_are_rejected() {
    let home = TestHome::new("provider-observation-structure-grammar");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, storage);

    let mut hook = begin_item(
        85,
        ProviderObservationItemLifecycle::Completed,
        ProviderObservationItemKind::HookPrompt,
        &mut callback,
    )
    .unwrap();
    let fragments = ProviderValueContext::Field(ProviderField::HookFragments);
    container(
        &mut hook,
        true,
        fragments,
        ProviderContainer::List,
        &mut callback,
    )
    .unwrap();
    assert_eq!(
        validation_error(element(&mut hook, true, fragments, 1, &mut callback)),
        ProviderObservationValidatorError::IndexMismatch
    );
    element(&mut hook, true, fragments, 0, &mut callback).unwrap();
    assert_eq!(
        validation_error(text(
            &mut hook,
            ProviderField::HookFragmentText,
            &[b"missing object"],
            &mut callback,
        )),
        ProviderObservationValidatorError::StructureMismatch
    );
    container(
        &mut hook,
        true,
        fragments,
        ProviderContainer::Object,
        &mut callback,
    )
    .unwrap();
    text(
        &mut hook,
        ProviderField::HookFragmentText,
        &[b"fragment"],
        &mut callback,
    )
    .unwrap();
    assert_eq!(
        validation_error(text(
            &mut hook,
            ProviderField::HookFragmentText,
            &[b"duplicate"],
            &mut callback,
        )),
        ProviderObservationValidatorError::DuplicateField
    );
    assert_eq!(
        validation_error(container(
            &mut hook,
            false,
            fragments,
            ProviderContainer::Object,
            &mut callback,
        )),
        ProviderObservationValidatorError::MissingRequiredField
    );
    hook.abandon();

    let mut dynamic = begin_item(
        86,
        ProviderObservationItemLifecycle::Completed,
        ProviderObservationItemKind::DynamicToolCall,
        &mut callback,
    )
    .unwrap();
    text(
        &mut dynamic,
        ProviderField::DynamicTool,
        &[b"tool"],
        &mut callback,
    )
    .unwrap();
    enum_control(
        &mut dynamic,
        ProviderField::DynamicStatus,
        ProviderEnumValue::Completed,
        &mut callback,
    )
    .unwrap();
    let arguments = ProviderValueContext::Field(ProviderField::DynamicArguments);
    container(
        &mut dynamic,
        true,
        arguments,
        ProviderContainer::Object,
        &mut callback,
    )
    .unwrap();
    assert_eq!(
        validation_error(dynamic.control(
            ProviderObservationControl::BeginObjectEntry {
                root: ProviderField::DynamicArguments,
                depth: 0,
                entry: 0,
            },
            &mut callback,
        )),
        ProviderObservationValidatorError::IndexMismatch
    );
    dynamic
        .control(
            ProviderObservationControl::BeginObjectEntry {
                root: ProviderField::DynamicArguments,
                depth: 1,
                entry: 0,
            },
            &mut callback,
        )
        .unwrap();
    let wrong_key = ProviderValueContext::Structured {
        root: ProviderField::DynamicArguments,
        depth: 0,
        position: ProviderStructuredPosition::ObjectKey { entry: 0 },
    };
    assert_eq!(
        validation_error(context_text(&mut dynamic, wrong_key, b"key", &mut callback,)),
        ProviderObservationValidatorError::StructureMismatch
    );
    dynamic.abandon();
    drop(callback);
    store.close().unwrap();
}
