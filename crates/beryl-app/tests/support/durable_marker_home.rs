use super::*;

pub(super) fn fixture_with_state(
    name: &str,
    seed: u8,
) -> (
    TestHome,
    HomeStore,
    BerylState,
    SyndicStorage,
    SyndicThreadId,
) {
    let home = TestHome::new(name);
    let candidate = beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
        &home.0,
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    compose(home, candidate, seed, "C:\\syndic-")
}

#[cfg(feature = "test-faults")]
pub(super) fn fixture_with_state_and_faults(
    name: &str,
    seed: u8,
    faults: FaultController,
) -> (
    TestHome,
    HomeStore,
    BerylState,
    SyndicStorage,
    SyndicThreadId,
) {
    let home = TestHome::new(name);
    let candidate = beryl_home_store::HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    compose(home, candidate, seed, "C:\\syndic-faults")
}

fn compose(
    home: TestHome,
    mut candidate: beryl_home_store::HomeOpenCandidate,
    seed: u8,
    root_path: &str,
) -> (
    TestHome,
    HomeStore,
    BerylState,
    SyndicStorage,
    SyndicThreadId,
) {
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        root_path,
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    (home, store, state, storage, thread)
}
