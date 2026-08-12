use super::*;

#[test]
fn recursive_structured_values_match_materialized_provider_encoding() {
    let home = TestHome::new("structured-parity");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let bound = {
        let mut callback = observation_callback(&store, storage);
        let mut stager = committed_stage_value(
            ProviderObservationStager::begin(
                ProviderObservationId::from_bytes([4; 16]),
                ProviderObservationBegin::Item {
                    lifecycle: ProviderObservationItemLifecycle::Started,
                    kind: ProviderObservationItemKind::DynamicToolCall,
                },
                &mut callback,
            )
            .unwrap(),
        );
        field_text(
            &mut stager,
            ProviderField::ItemId,
            &[b"dynamic-item"],
            &mut callback,
        );
        scalar(
            &mut stager,
            ProviderField::LifecycleObservedAt,
            ProviderScalar::Unsigned(91),
            &mut callback,
        );
        field_text(
            &mut stager,
            ProviderField::DynamicTool,
            &[b"nested-tool"],
            &mut callback,
        );
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
        text(&mut stager, key, &[b"values"], &mut callback);
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
        bind_sealed(stager, &mut callback)
    };

    let prepared = prepare_first(
        &storage,
        &store,
        bound,
        "dynamic-item",
        ProviderItemKind::DynamicToolCall,
        11,
    );
    let expected = ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        CasItemId::new("dynamic-item").unwrap(),
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(91),
            item: ProviderItemV1::DynamicToolCall(ProviderDynamicToolCallV1 {
                namespace: None,
                tool: ProviderTextV1::inline("nested-tool"),
                arguments: ProviderStructuredValueV1::Object(vec![ProviderObjectEntryV1 {
                    key: "values".to_owned(),
                    value: ProviderStructuredValueV1::List(vec![
                        ProviderStructuredValueV1::Boolean(true),
                    ]),
                }]),
                status: ProviderToolCallStatusV1::Completed,
                content_items: None,
                success: None,
                duration_ms: None,
            }),
        },
    );
    let (materialized, expected_reference) = materialized(&expected);
    assert_eq!(prepared.target().frame(), &expected_reference);
    let (compiled, final_build) = stage_compiler(&storage, &store, &prepared);
    assert_eq!(compiled.bytes, materialized.bytes);
    assert_eq!(compiled.narrative_spans, 0);
    assert_eq!(final_build.lifecycle(), ProviderItemBuildLifecycle::Sealed);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn variant_fields_may_precede_their_discriminant_without_changing_encoding() {
    let home = TestHome::new("variant-order");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let bound = {
        let mut callback = observation_callback(&store, storage);
        let mut stager = committed_stage_value(
            ProviderObservationStager::begin(
                ProviderObservationId::from_bytes([5; 16]),
                ProviderObservationBegin::Item {
                    lifecycle: ProviderObservationItemLifecycle::Started,
                    kind: ProviderObservationItemKind::FileChange,
                },
                &mut callback,
            )
            .unwrap(),
        );
        field_text(
            &mut stager,
            ProviderField::ItemId,
            &[b"patch-item"],
            &mut callback,
        );
        scalar(
            &mut stager,
            ProviderField::LifecycleObservedAt,
            ProviderScalar::Unsigned(101),
            &mut callback,
        );
        enum_value(
            &mut stager,
            ProviderField::FileChangeStatus,
            ProviderEnumValue::InProgress,
            &mut callback,
        );
        let changes = ProviderValueContext::Field(ProviderField::FileChanges);
        begin_container(&mut stager, changes, ProviderContainer::List, &mut callback);
        control(
            &mut stager,
            ProviderObservationControl::BeginElement {
                context: changes,
                index: 0,
            },
            &mut callback,
        );
        begin_container(
            &mut stager,
            changes,
            ProviderContainer::Object,
            &mut callback,
        );
        field_text(
            &mut stager,
            ProviderField::FileChangePath,
            &[b"old.rs"],
            &mut callback,
        );
        field_text(
            &mut stager,
            ProviderField::FileChangeDiff,
            &[b"+new"],
            &mut callback,
        );
        let kind = ProviderValueContext::Field(ProviderField::FileChangeKind);
        begin_container(&mut stager, kind, ProviderContainer::Object, &mut callback);
        field_text(
            &mut stager,
            ProviderField::FileChangeMovePath,
            &[b"new.rs"],
            &mut callback,
        );
        enum_value(
            &mut stager,
            ProviderField::FileChangeKind,
            ProviderEnumValue::Update,
            &mut callback,
        );
        end_container(&mut stager, kind, ProviderContainer::Object, &mut callback);
        end_container(
            &mut stager,
            changes,
            ProviderContainer::Object,
            &mut callback,
        );
        control(
            &mut stager,
            ProviderObservationControl::EndElement {
                context: changes,
                index: 0,
            },
            &mut callback,
        );
        end_container(&mut stager, changes, ProviderContainer::List, &mut callback);
        bind_sealed(stager, &mut callback)
    };

    let prepared = prepare_first(
        &storage,
        &store,
        bound,
        "patch-item",
        ProviderItemKind::FileChange,
        12,
    );
    let expected = ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        CasItemId::new("patch-item").unwrap(),
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(101),
            item: ProviderItemV1::FileChange(ProviderFileChangeV1 {
                status: ProviderPatchStatusV1::InProgress,
                changes: vec![ProviderFileUpdateChangeV1 {
                    path: ProviderTextV1::inline("old.rs"),
                    diff: ProviderTextV1::inline("+new"),
                    kind: ProviderPatchChangeKindV1::Update {
                        move_path: Some(ProviderTextV1::inline("new.rs")),
                    },
                }],
            }),
        },
    );
    let (materialized, expected_reference) = materialized(&expected);
    assert_eq!(prepared.target().frame(), &expected_reference);
    let (compiled, final_build) = stage_compiler(&storage, &store, &prepared);
    assert_eq!(compiled.bytes, materialized.bytes);
    assert_eq!(compiled.narrative_spans, 0);
    assert_eq!(final_build.lifecycle(), ProviderItemBuildLifecycle::Sealed);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
