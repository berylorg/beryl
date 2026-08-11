use super::*;

fn control(
    stager: &mut ProviderObservationStager,
    value: ProviderObservationControl,
    callback: &mut impl ProviderObservationStageCallback,
) {
    stager.control(value, callback).unwrap();
}

fn begin_item(
    byte: u8,
    kind: ProviderObservationItemKind,
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationStager {
    let mut stager = ProviderObservationStager::begin(
        ProviderObservationId::from_bytes([byte; 16]),
        ProviderObservationBegin::Item {
            lifecycle: ProviderObservationItemLifecycle::Completed,
            kind,
        },
        callback,
    )
    .unwrap();
    common_item(&mut stager, callback).unwrap();
    stager
}

fn begin_container(
    stager: &mut ProviderObservationStager,
    context: ProviderValueContext,
    container: ProviderContainer,
    callback: &mut impl ProviderObservationStageCallback,
) {
    control(
        stager,
        ProviderObservationControl::BeginContainer { context, container },
        callback,
    );
}

fn end_container(
    stager: &mut ProviderObservationStager,
    context: ProviderValueContext,
    container: ProviderContainer,
    callback: &mut impl ProviderObservationStageCallback,
) {
    control(
        stager,
        ProviderObservationControl::EndContainer { context, container },
        callback,
    );
}

fn enum_value(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    value: ProviderEnumValue,
    callback: &mut impl ProviderObservationStageCallback,
) {
    control(
        stager,
        ProviderObservationControl::Enum {
            context: ProviderValueContext::Field(field),
            value,
        },
        callback,
    );
}

#[test]
fn typed_lists_objects_discriminants_and_agent_state_entries_seal() {
    let home = TestHome::new("provider-observation-nested-grammar");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, storage);

    let mut hook = begin_item(87, ProviderObservationItemKind::HookPrompt, &mut callback);
    let fragments = ProviderValueContext::Field(ProviderField::HookFragments);
    begin_container(&mut hook, fragments, ProviderContainer::List, &mut callback);
    control(
        &mut hook,
        ProviderObservationControl::BeginElement {
            context: fragments,
            index: 0,
        },
        &mut callback,
    );
    begin_container(
        &mut hook,
        fragments,
        ProviderContainer::Object,
        &mut callback,
    );
    text(
        &mut hook,
        ProviderField::HookFragmentText,
        &[b"fragment"],
        &mut callback,
    )
    .unwrap();
    text(
        &mut hook,
        ProviderField::HookRunId,
        &[b"run"],
        &mut callback,
    )
    .unwrap();
    end_container(
        &mut hook,
        fragments,
        ProviderContainer::Object,
        &mut callback,
    );
    control(
        &mut hook,
        ProviderObservationControl::EndElement {
            context: fragments,
            index: 0,
        },
        &mut callback,
    );
    end_container(&mut hook, fragments, ProviderContainer::List, &mut callback);
    hook.seal(&mut callback).unwrap().abandon();

    let mut delta = ProviderObservationStager::begin(
        ProviderObservationId::from_bytes([88; 16]),
        ProviderObservationBegin::Delta {
            kind: ProviderDeltaKind::FileChangePatchUpdated,
        },
        &mut callback,
    )
    .unwrap();
    text(
        &mut delta,
        ProviderField::ItemId,
        &[b"delta-item"],
        &mut callback,
    )
    .unwrap();
    let changes = ProviderValueContext::Field(ProviderField::DeltaChanges);
    begin_container(&mut delta, changes, ProviderContainer::List, &mut callback);
    control(
        &mut delta,
        ProviderObservationControl::BeginElement {
            context: changes,
            index: 0,
        },
        &mut callback,
    );
    begin_container(
        &mut delta,
        changes,
        ProviderContainer::Object,
        &mut callback,
    );
    text(
        &mut delta,
        ProviderField::FileChangePath,
        &[b"file.rs"],
        &mut callback,
    )
    .unwrap();
    text(
        &mut delta,
        ProviderField::FileChangeDiff,
        &[b"+line"],
        &mut callback,
    )
    .unwrap();
    let patch = ProviderValueContext::Field(ProviderField::FileChangeKind);
    begin_container(&mut delta, patch, ProviderContainer::Object, &mut callback);
    enum_value(
        &mut delta,
        ProviderField::FileChangeKind,
        ProviderEnumValue::Add,
        &mut callback,
    );
    end_container(&mut delta, patch, ProviderContainer::Object, &mut callback);
    end_container(
        &mut delta,
        changes,
        ProviderContainer::Object,
        &mut callback,
    );
    control(
        &mut delta,
        ProviderObservationControl::EndElement {
            context: changes,
            index: 0,
        },
        &mut callback,
    );
    end_container(&mut delta, changes, ProviderContainer::List, &mut callback);
    delta.seal(&mut callback).unwrap().abandon();

    let mut collab = begin_item(
        89,
        ProviderObservationItemKind::CollabAgentToolCall,
        &mut callback,
    );
    enum_value(
        &mut collab,
        ProviderField::CollabTool,
        ProviderEnumValue::Wait,
        &mut callback,
    );
    enum_value(
        &mut collab,
        ProviderField::CollabStatus,
        ProviderEnumValue::Completed,
        &mut callback,
    );
    text(
        &mut collab,
        ProviderField::CollabSenderThreadId,
        &[b"sender"],
        &mut callback,
    )
    .unwrap();
    let receivers = ProviderValueContext::Field(ProviderField::CollabReceiverThreadIds);
    begin_container(
        &mut collab,
        receivers,
        ProviderContainer::List,
        &mut callback,
    );
    end_container(
        &mut collab,
        receivers,
        ProviderContainer::List,
        &mut callback,
    );
    let states = ProviderValueContext::Field(ProviderField::CollabAgentStates);
    begin_container(
        &mut collab,
        states,
        ProviderContainer::Object,
        &mut callback,
    );
    control(
        &mut collab,
        ProviderObservationControl::BeginObjectEntry {
            root: ProviderField::CollabAgentStates,
            depth: 0,
            entry: 0,
        },
        &mut callback,
    );
    let key = ProviderValueContext::Structured {
        root: ProviderField::CollabAgentStates,
        depth: 0,
        position: ProviderStructuredPosition::ObjectKey { entry: 0 },
    };
    control(
        &mut collab,
        ProviderObservationControl::BeginField(key),
        &mut callback,
    );
    collab
        .fragment(
            ProviderObservationStagingBytes::new(key, b"agent-1").unwrap(),
            &mut callback,
        )
        .unwrap();
    control(
        &mut collab,
        ProviderObservationControl::EndField(key),
        &mut callback,
    );
    enum_value(
        &mut collab,
        ProviderField::CollabAgentStateStatus,
        ProviderEnumValue::Running,
        &mut callback,
    );
    text(
        &mut collab,
        ProviderField::CollabAgentStateMessage,
        &[b"working"],
        &mut callback,
    )
    .unwrap();
    control(
        &mut collab,
        ProviderObservationControl::EndObjectEntry {
            root: ProviderField::CollabAgentStates,
            depth: 0,
            entry: 0,
        },
        &mut callback,
    );
    end_container(
        &mut collab,
        states,
        ProviderContainer::Object,
        &mut callback,
    );
    collab.seal(&mut callback).unwrap().abandon();

    drop(callback);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn recursive_structured_object_and_list_contexts_seal() {
    let home = TestHome::new("provider-observation-structured-grammar");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, storage);
    let mut stager = begin_item(
        91,
        ProviderObservationItemKind::DynamicToolCall,
        &mut callback,
    );
    text(
        &mut stager,
        ProviderField::DynamicTool,
        &[b"tool"],
        &mut callback,
    )
    .unwrap();
    enum_value(
        &mut stager,
        ProviderField::DynamicStatus,
        ProviderEnumValue::Completed,
        &mut callback,
    );
    let root = ProviderField::DynamicArguments;
    let root_context = ProviderValueContext::Field(root);
    begin_container(
        &mut stager,
        root_context,
        ProviderContainer::Object,
        &mut callback,
    );
    control(
        &mut stager,
        ProviderObservationControl::BeginObjectEntry {
            root,
            depth: 1,
            entry: 0,
        },
        &mut callback,
    );
    let key = ProviderValueContext::Structured {
        root,
        depth: 1,
        position: ProviderStructuredPosition::ObjectKey { entry: 0 },
    };
    control(
        &mut stager,
        ProviderObservationControl::BeginField(key),
        &mut callback,
    );
    stager
        .fragment(
            ProviderObservationStagingBytes::new(key, b"values").unwrap(),
            &mut callback,
        )
        .unwrap();
    control(
        &mut stager,
        ProviderObservationControl::EndField(key),
        &mut callback,
    );
    let value = ProviderValueContext::Structured {
        root,
        depth: 1,
        position: ProviderStructuredPosition::ObjectValue { entry: 0 },
    };
    begin_container(&mut stager, value, ProviderContainer::List, &mut callback);
    control(
        &mut stager,
        ProviderObservationControl::BeginElement {
            context: value,
            index: 0,
        },
        &mut callback,
    );
    control(
        &mut stager,
        ProviderObservationControl::Scalar {
            context: ProviderValueContext::Structured {
                root,
                depth: 2,
                position: ProviderStructuredPosition::ListElement { index: 0 },
            },
            value: ProviderScalar::Boolean(true),
        },
        &mut callback,
    );
    control(
        &mut stager,
        ProviderObservationControl::EndElement {
            context: value,
            index: 0,
        },
        &mut callback,
    );
    end_container(&mut stager, value, ProviderContainer::List, &mut callback);
    control(
        &mut stager,
        ProviderObservationControl::EndObjectEntry {
            root,
            depth: 1,
            entry: 0,
        },
        &mut callback,
    );
    end_container(
        &mut stager,
        root_context,
        ProviderContainer::Object,
        &mut callback,
    );
    stager.seal(&mut callback).unwrap().abandon();
    drop(callback);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
