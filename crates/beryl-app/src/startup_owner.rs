use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
};

use beryl_home_store::{CommandCancellation, HomeCloseError, HomeOpenPublication};
use beryl_model::{WindowId, WindowPlacement};
use beryl_state::BerylState;
use syndic_storage::SyndicStorage;

use crate::app_services::{AppServiceConfiguration, MainWindowServiceInputs, ProcessServiceOwner};

mod controller;
mod surface;
mod worker;

pub(crate) use controller::{RunningExitCommands, RunningExitRequest, RunningWindowExit};
pub(crate) use controller::{StartedProcess, StartupCommands, StartupCompletion, start};
pub(crate) use surface::OwnedStartupSurface;

pub(crate) enum StartupHomeOpen {
    Ready {
        candidate: HomeOpenPublication,
        state: BerylState,
        syndic: SyndicStorage,
    },
    Busy,
    Failed {
        detail: String,
        retained: Option<HomeCloseError>,
    },
}

pub(crate) struct StartupConfiguration {
    pub(crate) home: PathBuf,
    pub(crate) open: Arc<dyn Fn(&Path, CommandCancellation) -> StartupHomeOpen + Send + Sync>,
    pub(crate) services: AppServiceConfiguration,
    pub(crate) windows: MainWindowServiceInputs,
    pub(crate) enrollment_slots: NonZeroUsize,
    pub(crate) settlement_slots: NonZeroUsize,
    pub(crate) initial_window: WindowId,
    pub(crate) initial_placement: WindowPlacement,
    #[cfg(test)]
    pub(crate) native_hook: Option<
        Arc<dyn Fn(&mut crate::main_window::PreparedNativeMainWindowRestoreSet) + Send + Sync>,
    >,
    #[cfg(test)]
    pub(crate) service_hook: Option<Arc<dyn Fn(&mut ProcessServiceOwner) + Send + Sync>>,
}
