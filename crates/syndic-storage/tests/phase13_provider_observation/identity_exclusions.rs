use super::*;

fn control(
    stager: &mut ProviderObservationStager,
    value: ProviderObservationControl,
    callback: &mut impl ProviderObservationStageCallback,
) {
    clean_stage(stager.control(value, callback).unwrap());
}

fn begin_item(
    byte: u8,
    kind: ProviderObservationItemKind,
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationStager {
    let mut stager = clean_stage(
        ProviderObservationStager::begin(
            ProviderObservationId::from_bytes([byte; 16]),
            ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::Completed,
                kind,
            },
            callback,
        )
        .unwrap(),
    );
    common_item(&mut stager, callback).unwrap();
    stager
}

fn begin_container(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    container: ProviderContainer,
    callback: &mut impl ProviderObservationStageCallback,
) {
    control(
        stager,
        ProviderObservationControl::BeginContainer {
            context: ProviderValueContext::Field(field),
            container,
        },
        callback,
    );
}

fn end_container(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    container: ProviderContainer,
    callback: &mut impl ProviderObservationStageCallback,
) {
    control(
        stager,
        ProviderObservationControl::EndContainer {
            context: ProviderValueContext::Field(field),
            container,
        },
        callback,
    );
}

fn context_text(
    stager: &mut ProviderObservationStager,
    context: ProviderValueContext,
    bytes: &[u8],
    callback: &mut impl ProviderObservationStageCallback,
) {
    control(
        stager,
        ProviderObservationControl::BeginField(context),
        callback,
    );
    clean_stage(
        stager
            .fragment(
                ProviderObservationStagingBytes::new(context, bytes).unwrap(),
                callback,
            )
            .unwrap(),
    );
    control(
        stager,
        ProviderObservationControl::EndField(context),
        callback,
    );
}

fn generic_text() -> Vec<u8> {
    let mut value = vec![b'g'; 300];
    value[0] = b' ';
    value[150] = 0;
    value[299] = b' ';
    value
}

#[test]
fn memory_citation_thread_text_and_agent_state_keys_remain_generic() {
    let home = TestHome::new("provider-observation-generic-thread-text");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, storage);
    let generic = generic_text();

    let mut agent = begin_item(
        199,
        ProviderObservationItemKind::AgentMessage,
        &mut callback,
    );
    text(
        &mut agent,
        ProviderField::AgentMessageText,
        &[b"message"],
        &mut callback,
    )
    .unwrap();
    begin_container(
        &mut agent,
        ProviderField::MemoryCitation,
        ProviderContainer::Object,
        &mut callback,
    );
    begin_container(
        &mut agent,
        ProviderField::MemoryCitationEntries,
        ProviderContainer::List,
        &mut callback,
    );
    end_container(
        &mut agent,
        ProviderField::MemoryCitationEntries,
        ProviderContainer::List,
        &mut callback,
    );
    let threads = ProviderValueContext::Field(ProviderField::MemoryCitationThreadIds);
    begin_container(
        &mut agent,
        ProviderField::MemoryCitationThreadIds,
        ProviderContainer::List,
        &mut callback,
    );
    control(
        &mut agent,
        ProviderObservationControl::BeginElement {
            context: threads,
            index: 0,
        },
        &mut callback,
    );
    context_text(
        &mut agent,
        ProviderValueContext::Field(ProviderField::MemoryCitationThreadId),
        &generic,
        &mut callback,
    );
    control(
        &mut agent,
        ProviderObservationControl::EndElement {
            context: threads,
            index: 0,
        },
        &mut callback,
    );
    end_container(
        &mut agent,
        ProviderField::MemoryCitationThreadIds,
        ProviderContainer::List,
        &mut callback,
    );
    end_container(
        &mut agent,
        ProviderField::MemoryCitation,
        ProviderContainer::Object,
        &mut callback,
    );
    clean_seal(agent.seal(&mut callback).unwrap()).abandon();

    let mut collab = begin_item(
        200,
        ProviderObservationItemKind::CollabAgentToolCall,
        &mut callback,
    );
    control(
        &mut collab,
        ProviderObservationControl::Enum {
            context: ProviderValueContext::Field(ProviderField::CollabTool),
            value: ProviderEnumValue::Wait,
        },
        &mut callback,
    );
    control(
        &mut collab,
        ProviderObservationControl::Enum {
            context: ProviderValueContext::Field(ProviderField::CollabStatus),
            value: ProviderEnumValue::Completed,
        },
        &mut callback,
    );
    text(
        &mut collab,
        ProviderField::CollabSenderThreadId,
        &[b"sender"],
        &mut callback,
    )
    .unwrap();
    begin_container(
        &mut collab,
        ProviderField::CollabReceiverThreadIds,
        ProviderContainer::List,
        &mut callback,
    );
    end_container(
        &mut collab,
        ProviderField::CollabReceiverThreadIds,
        ProviderContainer::List,
        &mut callback,
    );
    begin_container(
        &mut collab,
        ProviderField::CollabAgentStates,
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
    context_text(&mut collab, key, &generic, &mut callback);
    control(
        &mut collab,
        ProviderObservationControl::Enum {
            context: ProviderValueContext::Field(ProviderField::CollabAgentStateStatus),
            value: ProviderEnumValue::Running,
        },
        &mut callback,
    );
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
        ProviderField::CollabAgentStates,
        ProviderContainer::Object,
        &mut callback,
    );
    clean_seal(collab.seal(&mut callback).unwrap()).abandon();

    drop(callback);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
