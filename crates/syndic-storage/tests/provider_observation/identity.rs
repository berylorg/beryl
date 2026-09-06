use super::*;

const ITEM_KINDS: [ProviderObservationItemKind; 17] = [
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

const DELTA_KINDS: [ProviderDeltaKind; 9] = [
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

fn invalid_context_text(
    stager: &mut ProviderObservationStager,
    context: ProviderValueContext,
    bytes: &[u8],
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationValidatorError {
    clean_stage(
        stager
            .control(ProviderObservationControl::BeginField(context), callback)
            .unwrap(),
    );
    if !bytes.is_empty() {
        match stager
            .fragment(
                ProviderObservationStagingBytes::new(context, bytes).unwrap(),
                callback,
            )
            .map(clean_stage)
        {
            Err(ProviderObservationStagingError::Validation(error)) => return error,
            Err(error) => panic!("unexpected staging error: {error}"),
            Ok(()) => {}
        }
    }
    match stager
        .control(ProviderObservationControl::EndField(context), callback)
        .map(clean_stage)
    {
        Err(ProviderObservationStagingError::Validation(error)) => error,
        Err(error) => panic!("unexpected staging error: {error}"),
        Ok(()) => panic!("invalid identity was accepted"),
    }
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
    scalar(
        &mut stager,
        ProviderField::LifecycleObservedAt,
        ProviderScalar::Unsigned(1),
        callback,
    )
    .unwrap();
    stager
}

#[test]
fn every_item_and_delta_item_id_uses_exact_cas_item_identity_validation() {
    let home = TestHome::new("provider-observation-all-item-identities");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, &storage);
    for (index, kind) in ITEM_KINDS.into_iter().enumerate() {
        let mut stager = begin_item(160 + index as u8, kind, &mut callback);
        assert_eq!(
            invalid_context_text(
                &mut stager,
                ProviderValueContext::Field(ProviderField::ItemId),
                b" invalid-item",
                &mut callback,
            ),
            ProviderObservationValidatorError::InvalidIdentity
        );
        stager.abandon();
    }
    for (index, kind) in DELTA_KINDS.into_iter().enumerate() {
        let mut stager = clean_stage(
            ProviderObservationStager::begin(
                ProviderObservationId::from_bytes([180 + index as u8; 16]),
                ProviderObservationBegin::Delta { kind },
                &mut callback,
            )
            .unwrap(),
        );
        assert_eq!(
            invalid_context_text(
                &mut stager,
                ProviderValueContext::Field(ProviderField::ItemId),
                b"invalid-item ",
                &mut callback,
            ),
            ProviderObservationValidatorError::InvalidIdentity
        );
        stager.abandon();
    }
    drop(callback);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn item_identity_enforces_exact_empty_length_trim_and_control_contract() {
    let home = TestHome::new("provider-observation-item-identity-contract");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, &storage);

    let exact = vec![b'a'; 256];
    let mut accepted = begin_item(
        190,
        ProviderObservationItemKind::ContextCompaction,
        &mut callback,
    );
    text(
        &mut accepted,
        ProviderField::ItemId,
        &[exact.as_slice()],
        &mut callback,
    )
    .unwrap();
    clean_seal(accepted.seal(&mut callback).unwrap()).abandon();

    for (byte, invalid) in [
        (191, Vec::new()),
        (192, b" trailing".to_vec()),
        (193, b"control\0byte".to_vec()),
        (194, vec![b'b'; 257]),
    ] {
        let mut stager = begin_item(
            byte,
            ProviderObservationItemKind::ContextCompaction,
            &mut callback,
        );
        assert_eq!(
            invalid_context_text(
                &mut stager,
                ProviderValueContext::Field(ProviderField::ItemId),
                &invalid,
                &mut callback,
            ),
            ProviderObservationValidatorError::InvalidIdentity
        );
        stager.abandon();
    }
    drop(callback);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn identity_byte_frontier_is_persisted_and_enforced_after_restart() {
    let home = TestHome::new("provider-observation-item-identity-restart");
    let identity = ProviderObservationId::from_bytes([195; 16]);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    {
        let mut callback = commit_callback(&store, &storage);
        let mut stager = begin_item(
            195,
            ProviderObservationItemKind::ContextCompaction,
            &mut callback,
        );
        let context = ProviderValueContext::Field(ProviderField::ItemId);
        clean_stage(
            stager
                .control(
                    ProviderObservationControl::BeginField(context),
                    &mut callback,
                )
                .unwrap(),
        );
        clean_stage(
            stager
                .fragment(
                    ProviderObservationStagingBytes::new(context, &[b'x'; 256]).unwrap(),
                    &mut callback,
                )
                .unwrap(),
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
    let before = storage
        .provider_observation_build(&reopened, identity, limit())
        .unwrap()
        .unwrap()
        .clone();
    let mut callback = commit_callback(&reopened, &storage);
    let context = ProviderValueContext::Field(ProviderField::ItemId);
    assert!(matches!(
        stager.fragment(
            ProviderObservationStagingBytes::new(context, b"y").unwrap(),
            &mut callback,
        ),
        Err(ProviderObservationStagingError::Validation(
            ProviderObservationValidatorError::InvalidIdentity
        ))
    ));
    drop(callback);
    assert_eq!(
        storage
            .provider_observation_build(&reopened, identity, limit())
            .unwrap()
            .unwrap(),
        before
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

fn enum_value(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    value: ProviderEnumValue,
    callback: &mut impl ProviderObservationStageCallback,
) {
    clean_stage(
        stager
            .control(
                ProviderObservationControl::Enum {
                    context: ProviderValueContext::Field(field),
                    value,
                },
                callback,
            )
            .unwrap(),
    );
}

fn prepare_collab(
    byte: u8,
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationStager {
    let mut stager = begin_item(
        byte,
        ProviderObservationItemKind::CollabAgentToolCall,
        callback,
    );
    text(&mut stager, ProviderField::ItemId, &[b"item"], callback).unwrap();
    enum_value(
        &mut stager,
        ProviderField::CollabTool,
        ProviderEnumValue::Wait,
        callback,
    );
    enum_value(
        &mut stager,
        ProviderField::CollabStatus,
        ProviderEnumValue::Completed,
        callback,
    );
    stager
}

#[test]
fn only_closed_collaboration_and_subagent_thread_fields_use_thread_identity_rules() {
    let home = TestHome::new("provider-observation-thread-identities");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, &storage);

    let mut sender = prepare_collab(196, &mut callback);
    assert_eq!(
        invalid_context_text(
            &mut sender,
            ProviderValueContext::Field(ProviderField::CollabSenderThreadId),
            b" sender",
            &mut callback,
        ),
        ProviderObservationValidatorError::InvalidIdentity
    );
    sender.abandon();

    let mut receiver = prepare_collab(197, &mut callback);
    text(
        &mut receiver,
        ProviderField::CollabSenderThreadId,
        &[b"sender"],
        &mut callback,
    )
    .unwrap();
    let receivers = ProviderValueContext::Field(ProviderField::CollabReceiverThreadIds);
    clean_stage(
        receiver
            .control(
                ProviderObservationControl::BeginContainer {
                    context: receivers,
                    container: ProviderContainer::List,
                },
                &mut callback,
            )
            .unwrap(),
    );
    clean_stage(
        receiver
            .control(
                ProviderObservationControl::BeginElement {
                    context: receivers,
                    index: 0,
                },
                &mut callback,
            )
            .unwrap(),
    );
    assert_eq!(
        invalid_context_text(
            &mut receiver,
            ProviderValueContext::Field(ProviderField::CollabReceiverThreadId),
            b"receiver ",
            &mut callback,
        ),
        ProviderObservationValidatorError::InvalidIdentity
    );
    receiver.abandon();

    let mut subagent = begin_item(
        198,
        ProviderObservationItemKind::SubAgentActivity,
        &mut callback,
    );
    text(
        &mut subagent,
        ProviderField::ItemId,
        &[b"item"],
        &mut callback,
    )
    .unwrap();
    enum_value(
        &mut subagent,
        ProviderField::SubAgentKind,
        ProviderEnumValue::SubAgentStarted,
        &mut callback,
    );
    assert_eq!(
        invalid_context_text(
            &mut subagent,
            ProviderValueContext::Field(ProviderField::SubAgentThreadId),
            b"thread\0id",
            &mut callback,
        ),
        ProviderObservationValidatorError::InvalidIdentity
    );
    subagent.abandon();
    drop(callback);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
