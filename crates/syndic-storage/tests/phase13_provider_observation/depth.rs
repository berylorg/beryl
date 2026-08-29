use super::*;

const ROOT: ProviderField = ProviderField::McpResultContent;
const MAX_DEPTH: u8 = 128;

fn control(
    stager: &mut ProviderObservationStager,
    control: ProviderObservationControl,
    callback: &mut impl ProviderObservationStageCallback,
) {
    clean_stage(stager.control(control, callback).unwrap());
}

fn container_context(depth: u8) -> ProviderValueContext {
    if depth == 1 {
        ProviderValueContext::Field(ROOT)
    } else {
        ProviderValueContext::Structured {
            root: ROOT,
            depth: depth - 1,
            position: ProviderStructuredPosition::ObjectValue { entry: 0 },
        }
    }
}

fn entry_context(depth: u8, key: bool) -> ProviderValueContext {
    ProviderValueContext::Structured {
        root: ROOT,
        depth,
        position: if key {
            ProviderStructuredPosition::ObjectKey { entry: 0 }
        } else {
            ProviderStructuredPosition::ObjectValue { entry: 0 }
        },
    }
}

fn begin_mcp(
    byte: u8,
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationStager {
    let mut stager = clean_stage(
        ProviderObservationStager::begin(
            ProviderObservationId::from_bytes([byte; 16]),
            ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::Completed,
                kind: ProviderObservationItemKind::McpToolCall,
            },
            callback,
        )
        .unwrap(),
    );
    common_item(&mut stager, callback).unwrap();
    text(
        &mut stager,
        ProviderField::McpServer,
        &[b"server"],
        callback,
    )
    .unwrap();
    text(&mut stager, ProviderField::McpTool, &[b"tool"], callback).unwrap();
    control(
        &mut stager,
        ProviderObservationControl::Enum {
            context: ProviderValueContext::Field(ProviderField::McpStatus),
            value: ProviderEnumValue::Completed,
        },
        callback,
    );
    scalar(
        &mut stager,
        ProviderField::McpArguments,
        ProviderScalar::Null,
        callback,
    )
    .unwrap();
    stager
}

fn open_worst_location(
    stager: &mut ProviderObservationStager,
    callback: &mut impl ProviderObservationStageCallback,
) {
    let result = ProviderValueContext::Field(ProviderField::McpResult);
    control(
        stager,
        ProviderObservationControl::BeginContainer {
            context: result,
            container: ProviderContainer::Object,
        },
        callback,
    );
    let contents = ProviderValueContext::Field(ProviderField::McpResultContents);
    control(
        stager,
        ProviderObservationControl::BeginContainer {
            context: contents,
            container: ProviderContainer::List,
        },
        callback,
    );
    control(
        stager,
        ProviderObservationControl::BeginElement {
            context: contents,
            index: 0,
        },
        callback,
    );
    control(
        stager,
        ProviderObservationControl::BeginContainer {
            context: container_context(1),
            container: ProviderContainer::Object,
        },
        callback,
    );
    for depth in 1..=MAX_DEPTH {
        control(
            stager,
            ProviderObservationControl::BeginObjectEntry {
                root: ROOT,
                depth,
                entry: 0,
            },
            callback,
        );
        let key = entry_context(depth, true);
        clean_stage(
            stager
                .control(ProviderObservationControl::BeginField(key), callback)
                .unwrap(),
        );
        clean_stage(
            stager
                .fragment(
                    ProviderObservationStagingBytes::new(key, b"k").unwrap(),
                    callback,
                )
                .unwrap(),
        );
        clean_stage(
            stager
                .control(ProviderObservationControl::EndField(key), callback)
                .unwrap(),
        );
        if depth < MAX_DEPTH {
            control(
                stager,
                ProviderObservationControl::BeginContainer {
                    context: entry_context(depth, false),
                    container: ProviderContainer::Object,
                },
                callback,
            );
        }
    }
}

fn close_worst_location(
    stager: &mut ProviderObservationStager,
    callback: &mut impl ProviderObservationStageCallback,
) {
    control(
        stager,
        ProviderObservationControl::Scalar {
            context: entry_context(MAX_DEPTH, false),
            value: ProviderScalar::Null,
        },
        callback,
    );
    for depth in (1..=MAX_DEPTH).rev() {
        control(
            stager,
            ProviderObservationControl::EndObjectEntry {
                root: ROOT,
                depth,
                entry: 0,
            },
            callback,
        );
        control(
            stager,
            ProviderObservationControl::EndContainer {
                context: container_context(depth),
                container: ProviderContainer::Object,
            },
            callback,
        );
    }
    let contents = ProviderValueContext::Field(ProviderField::McpResultContents);
    control(
        stager,
        ProviderObservationControl::EndElement {
            context: contents,
            index: 0,
        },
        callback,
    );
    control(
        stager,
        ProviderObservationControl::EndContainer {
            context: contents,
            container: ProviderContainer::List,
        },
        callback,
    );
    control(
        stager,
        ProviderObservationControl::EndContainer {
            context: ProviderValueContext::Field(ProviderField::McpResult),
            container: ProviderContainer::Object,
        },
        callback,
    );
}

#[test]
fn worst_location_accepts_exact_semantic_depth_128() {
    let home = TestHome::new("provider-observation-depth-128");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, &storage);
    let mut stager = begin_mcp(150, &mut callback);
    open_worst_location(&mut stager, &mut callback);
    close_worst_location(&mut stager, &mut callback);
    clean_seal(stager.seal(&mut callback).unwrap()).abandon();
    drop(callback);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn exact_depth_128_worst_location_resumes_with_complete_259_frame_stack() {
    let home = TestHome::new("provider-observation-depth-128-restart");
    let identity = ProviderObservationId::from_bytes([151; 16]);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    {
        let mut callback = commit_callback(&store, &storage);
        let mut stager = begin_mcp(151, &mut callback);
        open_worst_location(&mut stager, &mut callback);
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
    close_worst_location(&mut stager, &mut callback);
    clean_seal(stager.seal(&mut callback).unwrap()).abandon();
    drop(callback);
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn worst_location_rejects_semantic_depth_129() {
    let home = TestHome::new("provider-observation-depth-129");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = commit_callback(&store, &storage);
    let mut stager = begin_mcp(152, &mut callback);
    open_worst_location(&mut stager, &mut callback);
    assert!(matches!(
        stager.control(
            ProviderObservationControl::BeginContainer {
                context: entry_context(MAX_DEPTH, false),
                container: ProviderContainer::Object,
            },
            &mut callback,
        ),
        Err(ProviderObservationStagingError::Validation(
            ProviderObservationValidatorError::StructuredDepthExceeded
        ))
    ));
    stager.abandon();
    drop(callback);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
