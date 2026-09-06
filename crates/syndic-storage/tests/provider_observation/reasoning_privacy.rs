use super::*;

fn begin(
    byte: u8,
    callback: &mut impl ProviderObservationStageCallback,
) -> ProviderObservationStager {
    clean_stage(
        ProviderObservationStager::begin(
            ProviderObservationId::from_bytes([byte; 16]),
            ProviderObservationBegin::Delta {
                kind: ProviderDeltaKind::ReasoningTextObserved,
            },
            callback,
        )
        .unwrap(),
    )
}

fn assert_validation(
    result: Result<(), ProviderObservationStagingError>,
    expected: ProviderObservationValidatorError,
) {
    assert!(matches!(
        result,
        Err(ProviderObservationStagingError::Validation(error)) if error == expected
    ));
}

#[test]
fn reasoning_text_observed_seals_only_identity_and_content_index() {
    let home = TestHome::new("provider-observation-private-reasoning");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let identity = ProviderObservationId::from_bytes([120; 16]);
    let sealed = {
        let mut callback = commit_callback(&store, &storage);
        let mut stager = begin(120, &mut callback);
        text(
            &mut stager,
            ProviderField::ItemId,
            &[b"reasoning-item"],
            &mut callback,
        )
        .unwrap();
        scalar(
            &mut stager,
            ProviderField::DeltaContentIndex,
            ProviderScalar::Unsigned(7),
            &mut callback,
        )
        .unwrap();
        clean_seal(stager.seal(&mut callback).unwrap())
    };

    let bound = sealed.bind(route(), route()).unwrap();
    let mut cursor = storage
        .open_provider_observation_cursor(&store, bound, limit())
        .unwrap();
    let mut payloads = Vec::new();
    while let Some(page) = storage
        .read_provider_observation_cursor_page(&store, &mut cursor, limit())
        .unwrap()
    {
        payloads.push(page.into_payload());
    }
    assert_eq!(payloads.len(), 4);
    assert!(payloads.iter().all(|payload| match payload {
        ProviderObservationChunkPayload::Fragment { context, bytes } => {
            *context == ProviderValueContext::Field(ProviderField::ItemId)
                && bytes.as_ref() == b"reasoning-item"
        }
        ProviderObservationChunkPayload::Control(
            ProviderObservationControl::BeginField(ProviderValueContext::Field(
                ProviderField::ItemId,
            ))
            | ProviderObservationControl::EndField(ProviderValueContext::Field(
                ProviderField::ItemId,
            )),
        ) => true,
        ProviderObservationChunkPayload::Control(ProviderObservationControl::Scalar {
            context: ProviderValueContext::Field(ProviderField::DeltaContentIndex),
            value: ProviderScalar::Unsigned(7),
        }) => true,
        _ => false,
    }));
    let build = storage
        .provider_observation_build(&store, identity, limit())
        .unwrap()
        .unwrap();
    assert_eq!(build.lifecycle(), ProviderObservationBuildLifecycle::Sealed);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn reasoning_text_observed_rejects_text_substitution_duplicate_and_missing_index() {
    let home = TestHome::new("provider-observation-private-reasoning-negative");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();

    let mut callback = commit_callback(&store, &storage);
    let mut exact = begin(121, &mut callback);
    text(
        &mut exact,
        ProviderField::ItemId,
        &[b"reasoning-item"],
        &mut callback,
    )
    .unwrap();
    assert_validation(
        exact
            .control(
                ProviderObservationControl::BeginField(ProviderValueContext::Field(
                    ProviderField::DeltaText,
                )),
                &mut callback,
            )
            .map(clean_stage),
        ProviderObservationValidatorError::FieldNotAllowed,
    );
    assert_validation(
        scalar(
            &mut exact,
            ProviderField::DeltaContentIndex,
            ProviderScalar::Signed(0),
            &mut callback,
        ),
        ProviderObservationValidatorError::ValueMismatch,
    );
    scalar(
        &mut exact,
        ProviderField::DeltaContentIndex,
        ProviderScalar::Unsigned(0),
        &mut callback,
    )
    .unwrap();
    assert_validation(
        scalar(
            &mut exact,
            ProviderField::DeltaContentIndex,
            ProviderScalar::Unsigned(1),
            &mut callback,
        ),
        ProviderObservationValidatorError::DuplicateField,
    );
    clean_seal(exact.seal(&mut callback).unwrap()).abandon();

    let missing_identity = ProviderObservationId::from_bytes([122; 16]);
    let mut missing = begin(122, &mut callback);
    text(
        &mut missing,
        ProviderField::ItemId,
        &[b"reasoning-item"],
        &mut callback,
    )
    .unwrap();
    assert!(matches!(
        missing.seal(&mut callback),
        Err(ProviderObservationStagingError::Validation(
            ProviderObservationValidatorError::MissingRequiredField
        ))
    ));
    drop(callback);
    assert_eq!(
        storage
            .provider_observation_build(&store, missing_identity, limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        ProviderObservationBuildLifecycle::Building
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
