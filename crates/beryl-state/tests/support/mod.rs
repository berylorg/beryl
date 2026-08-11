#![allow(dead_code)]

pub mod phase9;

use std::path::Path;

use beryl_home_store::{
    CommandError, CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
use beryl_model::{
    AdmittedHostPath, Availability, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
};
use beryl_state::{
    AvailabilitySnapshot, BerylState, CreateRuntimeWithHomeRoot, RootRegistration,
    RuntimeRegistration, UnixMillis,
};

pub fn open(path: &Path) -> (HomeStore, BerylState) {
    let mut store =
        HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let state = BerylState::register(&mut store).unwrap();
    (store, state)
}

pub fn host_runtime(
    runtime_byte: u8,
    root_byte: u8,
    executable: &str,
    root: &str,
) -> CreateRuntimeWithHomeRoot {
    let mode = RuntimeMode::host();
    let runtime = RuntimeRegistration::new(
        RuntimeId::from_bytes([runtime_byte; 16]),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, executable).unwrap(),
        mode.clone(),
        RuntimeNativePath::from_admitted(mode.clone(), PathFlavor::Windows, executable).unwrap(),
        UnixMillis::new(10),
        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(11)).unwrap(),
    )
    .unwrap();
    let root = RootRegistration::new(
        RootId::from_bytes([root_byte; 16]),
        RuntimeNativePath::from_admitted(mode, PathFlavor::Windows, root).unwrap(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, root).unwrap(),
        UnixMillis::new(10),
        AvailabilitySnapshot::unknown(),
    );
    CreateRuntimeWithHomeRoot::new(runtime, root).unwrap()
}

pub fn wsl_runtime(
    runtime_byte: u8,
    root_byte: u8,
    distro: &str,
    executable_host: &str,
    executable_native: &str,
    root_host: &str,
    root_native: &str,
) -> CreateRuntimeWithHomeRoot {
    let mode = RuntimeMode::wsl(distro).unwrap();
    let runtime = RuntimeRegistration::new(
        RuntimeId::from_bytes([runtime_byte; 16]),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, executable_host).unwrap(),
        mode.clone(),
        RuntimeNativePath::from_admitted(mode.clone(), PathFlavor::Posix, executable_native)
            .unwrap(),
        UnixMillis::new(20),
        AvailabilitySnapshot::unknown(),
    )
    .unwrap();
    let root = RootRegistration::new(
        RootId::from_bytes([root_byte; 16]),
        RuntimeNativePath::from_admitted(mode, PathFlavor::Posix, root_native).unwrap(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, root_host).unwrap(),
        UnixMillis::new(20),
        AvailabilitySnapshot::unknown(),
    );
    CreateRuntimeWithHomeRoot::new(runtime, root).unwrap()
}

pub fn execute(store: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

pub fn create_host_runtime(
    store: &HomeStore,
    state: BerylState,
    runtime_byte: u8,
    root_byte: u8,
    executable: &str,
    root: &str,
) {
    let contribution = state.runtime_roots().create_runtime_with_home_root(
        state.runtime_roots().revision(store).unwrap(),
        host_runtime(runtime_byte, root_byte, executable, root),
    );
    match execute(store, contribution) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed runtime creation, got {outcome:?}"),
    }
}

pub fn contributor_source<T: std::error::Error + 'static>(error: &CommandError) -> Option<&T> {
    match error {
        CommandError::ContributorValidation { source, .. }
        | CommandError::ContributorAssembly { source, .. } => source.downcast_ref::<T>(),
        _ => None,
    }
}
