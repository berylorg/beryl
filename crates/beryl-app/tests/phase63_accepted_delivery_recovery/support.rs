use std::path::Path;

use beryl_app::{
    cas_projection::{
        ProjectionConnectionService, ProjectionServiceConfig, ScheduledOrdinaryExecutionProvider,
    },
    input_admission::{accepted_input_promotion_command, accepted_input_promotion_status},
};
use beryl_home_store::{
    CursorReadLimits, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{SyndicItemId, SyndicTurnId};
use beryl_state::BerylState;
use syndic_storage::{
    ACCEPTED_NEXT_PAGE_MAX_BYTES, AcceptedInputPromotionStatus, PromoteAcceptedInput,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
};

use crate::phase62_support::{NextRecordIds, open_registered_home};

pub struct SeededHome {
    pub directory: tempfile::TempDir,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub state: BerylState,
}

pub struct PromotedTurn {
    pub ids: NextRecordIds,
    pub turn: SyndicTurnId,
}

pub fn seeded_home() -> SeededHome {
    let (directory, store, storage, state) = open_registered_home();
    SeededHome {
        directory,
        store,
        storage,
        state,
    }
}

pub fn close_seeded(home: SeededHome) -> tempfile::TempDir {
    let SeededHome {
        directory,
        store,
        storage: _,
        state: _,
    } = home;
    store.close().unwrap();
    directory
}

pub fn reopen_registered(path: &Path) -> (HomeStore, SyndicStorage, BerylState) {
    let mut store =
        HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let state = BerylState::register(&mut store).unwrap();
    (store, storage, state)
}

pub fn restart_service(
    path: &Path,
    provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
) -> (ProjectionConnectionService, SyndicStorage, BerylState) {
    let (store, storage, state) = reopen_registered(path);
    let service = ProjectionConnectionService::new(
        store,
        storage,
        ProjectionServiceConfig::try_new(128, 8).unwrap(),
        provider,
    )
    .unwrap();
    (service, storage, state)
}

pub fn restart_service_with(
    path: &Path,
    provider: impl FnOnce(&BerylState) -> Box<dyn ScheduledOrdinaryExecutionProvider>,
) -> (ProjectionConnectionService, SyndicStorage, BerylState) {
    restart_service_with_config(
        path,
        ProjectionServiceConfig::try_new(128, 8).unwrap(),
        provider,
    )
}

pub fn restart_service_with_config(
    path: &Path,
    config: ProjectionServiceConfig,
    provider: impl FnOnce(&BerylState) -> Box<dyn ScheduledOrdinaryExecutionProvider>,
) -> (ProjectionConnectionService, SyndicStorage, BerylState) {
    let (store, storage, state) = reopen_registered(path);
    let provider = provider(&state);
    let service = ProjectionConnectionService::new(store, storage, config, provider).unwrap();
    (service, storage, state)
}

pub fn promote_installed_next(
    store: &HomeStore,
    storage: SyndicStorage,
    state: &BerylState,
    ids: NextRecordIds,
    seed: u8,
) -> PromotedTurn {
    let revision = storage.revision(store).unwrap();
    let limits = CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
    let sources = storage
        .accepted_next_source_page(store, revision, None, limits)
        .unwrap();
    let source = sources
        .records()
        .iter()
        .find(|source| source.thread_id() == ids.thread)
        .expect("installed accepted-next source remains discoverable");
    let candidate = storage
        .accepted_next_candidate_page(store, *source, None, limits)
        .unwrap()
        .into_candidate()
        .expect("installed accepted-next source has one effective candidate");
    assert_eq!(candidate.input_id(), ids.accepted_input);

    let turn = SyndicTurnId::from_bytes([seed; 16]);
    let item = SyndicItemId::from_bytes([seed.wrapping_add(1); 16]);
    let promotion = PromoteAcceptedInput::new(candidate, turn, item, time(63_020));
    let command =
        accepted_input_promotion_command(store, storage, state.assets(), promotion.clone())
            .unwrap();
    store.execute(command).unwrap();
    assert_eq!(
        accepted_input_promotion_status(store, storage, state.assets(), &promotion, point_limit(),)
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    PromotedTurn { ids, turn }
}

pub fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

pub fn startup_source(
    store: &HomeStore,
    storage: SyndicStorage,
) -> syndic_storage::DeliveryRecoverySource {
    let page = storage
        .delivery_recovery_startup_page(
            store,
            None,
            CursorReadLimits::new(
                syndic_storage::DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS,
                syndic_storage::DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 1);
    assert!(page.next_cursor().is_none());
    page.records()[0].clone()
}

pub fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub const fn time(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}
