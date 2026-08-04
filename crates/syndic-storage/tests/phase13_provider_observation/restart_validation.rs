use super::*;

#[test]
fn restart_preserves_discriminant_and_duplicate_rejection_state() {
    let home = TestHome::new("provider-observation-restart-validation");
    let identity = ProviderObservationId::from_bytes([90; 16]);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    {
        let mut callback = commit_callback(&store, storage);
        let mut stager = ProviderObservationStager::begin(
            identity,
            ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::Completed,
                kind: ProviderObservationItemKind::WebSearch,
            },
            &mut callback,
        )
        .unwrap();
        common_item(&mut stager, &mut callback).unwrap();
        text(
            &mut stager,
            ProviderField::WebSearchQuery,
            &[b"query"],
            &mut callback,
        )
        .unwrap();
        let action = ProviderValueContext::Field(ProviderField::WebSearchAction);
        stager
            .control(
                ProviderObservationControl::BeginContainer {
                    context: action,
                    container: ProviderContainer::Object,
                },
                &mut callback,
            )
            .unwrap();
        stager
            .control(
                ProviderObservationControl::Enum {
                    context: ProviderValueContext::Field(ProviderField::WebSearchActionKind),
                    value: ProviderEnumValue::Search,
                },
                &mut callback,
            )
            .unwrap();
        stager.abandon();
    }
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let mut stager = storage
        .resume_provider_observation(&reopened, identity, limit())
        .unwrap()
        .unwrap();
    let mut callback = commit_callback(&reopened, storage);
    assert!(matches!(
        stager.control(
            ProviderObservationControl::Enum {
                context: ProviderValueContext::Field(ProviderField::WebSearchActionKind),
                value: ProviderEnumValue::Other,
            },
            &mut callback,
        ),
        Err(ProviderObservationStagingError::Validation(
            ProviderObservationValidatorError::DuplicateField
        ))
    ));
    text(
        &mut stager,
        ProviderField::WebSearchActionQuery,
        &[b"specific"],
        &mut callback,
    )
    .unwrap();
    let action = ProviderValueContext::Field(ProviderField::WebSearchAction);
    stager
        .control(
            ProviderObservationControl::EndContainer {
                context: action,
                container: ProviderContainer::Object,
            },
            &mut callback,
        )
        .unwrap();
    let sealed = stager.seal(&mut callback).unwrap();
    assert_eq!(
        sealed.history_support(),
        ProviderFrameHistorySupportV1::Supported
    );
    sealed.abandon();
    drop(callback);
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}
