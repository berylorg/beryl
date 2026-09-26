use super::*;
use crate::theme_runtime::GpuiAppearanceWindowSet;
use gpui::{App, AppContext, AsyncApp, Entity};

mod disposal;
mod flight;
mod preparation;
mod signal;
#[cfg(feature = "test-faults")]
mod test_faults;

pub struct MainWindowNativePreparationFailure {
    pub prepared: PreparedMainWindowRestoreSet,
    pub error: String,
}

pub struct PreparedNativeMainWindowRestoreSet {
    owner: Box<MainWindowRestoreSet>,
    members: Vec<PreparedNativeMember>,
    validation: Vec<NativeValidation>,
    #[cfg(feature = "test-faults")]
    faults: test_faults::NativeRestoreSetFaults,
}

pub enum MainWindowNativeRestoreSetCompletion {
    Published(PublishedMainWindowRestoreSet),
    Failed(MainWindowNativeRestoreSetFailure),
}

pub struct PublishedMainWindowRestoreSet {
    owner: Box<MainWindowRestoreSet>,
    shells: Vec<MainWindowShell>,
    #[cfg(feature = "test-faults")]
    appearance: Entity<GpuiAppearanceWindowSet>,
}

impl PublishedMainWindowRestoreSet {
    pub fn window_ids(&self) -> &[WindowId] {
        &self.owner.expected_windows
    }

    pub fn shells(&self) -> &[MainWindowShell] {
        &self.shells
    }
}

pub struct MainWindowNativeRestoreSetFailure {
    pub error: String,
    pub diagnostics: Vec<MainWindowNativeRestoreSetMemberFailure>,
    pub retained: Option<RetainedNativeMainWindowRestoreSet>,
}

pub struct MainWindowNativeRestoreSetMemberFailure {
    pub window_id: WindowId,
    pub error: String,
}

pub struct RetainedNativeMainWindowRestoreSet {
    owner: Box<MainWindowRestoreSet>,
    members: Vec<MainWindowNativeRetainedMember>,
}

impl RetainedNativeMainWindowRestoreSet {
    pub fn window_ids(&self) -> &[WindowId] {
        &self.owner.expected_windows
    }

    pub fn members(&self) -> &[MainWindowNativeRetainedMember] {
        &self.members
    }

    pub fn shells(&self) -> impl Iterator<Item = &MainWindowShell> {
        self.members
            .iter()
            .filter_map(|member| match &member.custody {
                MainWindowNativeRetainedCustody::Native(shell) => Some(shell.as_ref()),
                _ => None,
            })
    }
}

pub struct MainWindowNativeRetainedMember {
    pub window_id: WindowId,
    pub custody: MainWindowNativeRetainedCustody,
}

pub enum MainWindowNativeRetainedCustody {
    Native(Box<MainWindowShell>),
    Prepared(Box<MainWindowStartupShellPrepared>),
    AcquiredUnpublished(Box<MainWindowShellUnpublished>),
    AcquiredAbandonment(Box<MainWindowShellAbandonment>),
    AcquiredReconciliation(Box<MainWindowShellAbandonmentReconciliation>),
    CommittedAbandonment {
        receipt: beryl_home_store::CommitReceipt,
        later_failure: Option<beryl_home_store::CommandError>,
        local_finalization: beryl_home_store::CommittedLocalFinalization,
    },
    AcquiredPreserved(Box<MainWindowShellRecordPreservingRetirement>),
    Restored(Box<RestoredWindowShellUnpublished>),
}

#[derive(Clone)]
pub struct MainWindowNativeRestoreSetCancellation {
    cancellation: CommandCancellation,
    wake: Arc<signal::StartupWake>,
}

impl MainWindowNativeRestoreSetCancellation {
    pub fn cancel(&self) {
        self.cancellation.cancel();
        self.wake.notify();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
}

struct NativeValidation {
    window_id: WindowId,
    check: NativeMemberValidation,
}

struct NativeMember {
    window_id: WindowId,
    state: NativeMemberState,
}

struct PreparedNativeMember {
    window_id: WindowId,
    prepared: Box<MainWindowStartupShellPrepared>,
    placement: PreparedWindowsWindowPlacement,
}

enum NativeMemberState {
    Prepared {
        prepared: Box<MainWindowStartupShellPrepared>,
        placement: PreparedWindowsWindowPlacement,
    },
    Hidden(MainWindowShell),
    Unconstructed(Box<MainWindowStartupShellPrepared>),
    #[cfg(feature = "test-faults")]
    Retained {
        shell: MainWindowShell,
        error: String,
    },
    InFlight,
}

struct NativeRestoreSetFlight {
    owner: Option<Box<MainWindowRestoreSet>>,
    members: Vec<NativeMember>,
    validation: Vec<NativeValidation>,
    appearance: Entity<GpuiAppearanceWindowSet>,
    cancellation: MainWindowNativeRestoreSetCancellation,
    #[cfg(feature = "test-faults")]
    faults: test_faults::NativeRestoreSetFaults,
}

impl PreparedNativeMainWindowRestoreSet {
    pub fn window_ids(&self) -> &[WindowId] {
        &self.owner.expected_windows
    }

    pub fn start(
        self,
        appearance: Entity<GpuiAppearanceWindowSet>,
        completion: impl FnOnce(MainWindowNativeRestoreSetCompletion, &mut App) + 'static,
        app: &mut App,
    ) -> MainWindowNativeRestoreSetCancellation {
        let cancellation = MainWindowNativeRestoreSetCancellation {
            cancellation: self.owner.cancellation.clone(),
            wake: Arc::new(signal::StartupWake::default()),
        };
        let mut flight = NativeRestoreSetFlight {
            owner: Some(self.owner),
            members: self
                .members
                .into_iter()
                .map(|member| NativeMember {
                    window_id: member.window_id,
                    state: NativeMemberState::Prepared {
                        prepared: member.prepared,
                        placement: member.placement,
                    },
                })
                .collect(),
            validation: self.validation,
            appearance,
            cancellation: cancellation.clone(),
            #[cfg(feature = "test-faults")]
            faults: self.faults,
        };
        app.spawn(async move |cx| {
            let result = flight.run(cx).await;
            let completion_value = match result {
                Ok(shells) => {
                    MainWindowNativeRestoreSetCompletion::Published(PublishedMainWindowRestoreSet {
                        owner: flight.owner.take().expect("native set retains its attempt"),
                        shells,
                        #[cfg(feature = "test-faults")]
                        appearance: flight.appearance.clone(),
                    })
                }
                Err(error) => {
                    MainWindowNativeRestoreSetCompletion::Failed(flight.dispose(error, cx).await)
                }
            };
            cx.update(|app| completion(completion_value, app))
                .expect("native startup retains a live GUI executor through completion");
        })
        .detach();
        cancellation
    }
}
