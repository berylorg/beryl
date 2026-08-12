use super::{matrix::*, *};

fn assert_duplicate_after_restart(
    store: &HomeStore,
    storage: SyndicStorage,
    identity: ProviderObservationId,
) {
    let mut stager = storage
        .resume_provider_observation(store, identity, limit())
        .unwrap()
        .unwrap();
    let before = storage
        .provider_observation_build(store, identity, limit())
        .unwrap()
        .unwrap()
        .clone();
    let mut callback = commit_callback(store, storage);
    assert!(matches!(
        stager.control(
            ProviderObservationControl::BeginField(ProviderValueContext::Field(
                ProviderField::ItemId,
            )),
            &mut callback,
        ),
        Err(ProviderObservationStagingError::Validation(
            ProviderObservationValidatorError::DuplicateField
        ))
    ));
    drop(callback);
    assert_eq!(
        storage
            .provider_observation_build(store, identity, limit())
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

#[test]
fn all_17_item_and_nine_delta_duplicate_states_survive_restart_unpublished() {
    let home = TestHome::new("provider-observation-restart-matrix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    {
        let mut callback = commit_callback(&store, storage);
        for (index, kind) in ITEMS.into_iter().enumerate() {
            let mut stager = clean_stage(
                ProviderObservationStager::begin(
                    ProviderObservationId::from_bytes([90 + index as u8; 16]),
                    ProviderObservationBegin::Item {
                        lifecycle: ProviderObservationItemLifecycle::Completed,
                        kind,
                    },
                    &mut callback,
                )
                .unwrap(),
            );
            common_item(&mut stager, &mut callback).unwrap();
            stager.abandon();
        }
        for (index, kind) in DELTAS.into_iter().enumerate() {
            let mut stager = clean_stage(
                ProviderObservationStager::begin(
                    ProviderObservationId::from_bytes([110 + index as u8; 16]),
                    ProviderObservationBegin::Delta { kind },
                    &mut callback,
                )
                .unwrap(),
            );
            text(
                &mut stager,
                ProviderField::ItemId,
                &[b"item"],
                &mut callback,
            )
            .unwrap();
            stager.abandon();
        }
    }
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    for index in 0..ITEMS.len() {
        assert_duplicate_after_restart(
            &reopened,
            storage,
            ProviderObservationId::from_bytes([90 + index as u8; 16]),
        );
    }
    for index in 0..DELTAS.len() {
        assert_duplicate_after_restart(
            &reopened,
            storage,
            ProviderObservationId::from_bytes([110 + index as u8; 16]),
        );
    }
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}
