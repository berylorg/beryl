use super::*;

pub(super) fn prepare_home(path: &std::path::Path) {
    eprintln!(
        "original ordinary claim activation Home fixture: {}",
        path.display()
    );
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let state = beryl_state::BerylState::register(&mut candidate).unwrap();
    let storage = syndic_storage::SyndicStorage::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(
            beryl_state::BerylState::required_domains()
                .unwrap()
                .merge(syndic_storage::SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let selected = state.session().minimal_bootstrap(&home).unwrap().unwrap();
    let execution = storage
        .thread_execution(
            &home,
            selected.windows()[0].selected_thread().unwrap().thread_id(),
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let target = SyndicThreadId::from_bytes([242; 16]);
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(storage.create_thread(
            storage.revision(&home).unwrap(),
            CreateThread::ordinary(
                target,
                SyndicDraftId::from_bytes([243; 16]),
                execution,
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    super::transcript::prepare_history(&home, &storage, target);
    assert_eq!(
        resident_fixture::composer_support::seed_published_draft_chunks(&storage, &home, target, 1),
        768
    );
    drop((storage, state));
    home.close().unwrap();
}
