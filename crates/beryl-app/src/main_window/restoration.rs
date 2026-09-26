use super::*;
use crate::composer_host::ComposerHostActivationRequest;
use crate::theme_runtime::AppearanceGeneration;
use beryl_home_store::CommandCancellation;
use beryl_model::{SessionRevision, WindowId, WindowPlacement};
use beryl_state::{MAX_RESTORABLE_WINDOWS, SessionWindowRecord};
use std::sync::Arc;
use syndic_storage::DraftPieceOperationIdV1;

mod discovery;
mod disposal;
#[cfg(target_os = "windows")]
mod native;
mod preparation;
mod retained;
use discovery::{DiscoveredRestoreSet, RestoreSetDiscovery};
#[cfg(target_os = "windows")]
pub use native::*;
pub use retained::*;

#[cfg(target_os = "windows")]
pub(in crate::main_window) type NativeMemberValidation = Box<
    dyn Fn(&RestoredWindowPreparationAttempt, &MainWindowCreationServices) -> Result<(), String>
        + Send,
>;

pub type RestoredWindowActivationSource = Arc<
    dyn Fn(
            &SessionWindowRecord,
        ) -> Result<(ComposerHostActivationRequest, DraftPieceOperationIdV1), String>
        + Send
        + Sync,
>;

pub enum PreparedRestoreSetMember {
    Restored(Box<RestoredWindowShellPrepared>),
    Threadless(ThreadlessWindowShellPrepared),
    Replacement(Box<MainWindowShellPrepared>),
}

impl PreparedRestoreSetMember {
    pub fn window_id(&self) -> WindowId {
        match self {
            Self::Restored(prepared) => prepared.window_id(),
            Self::Threadless(prepared) => prepared.window_id(),
            Self::Replacement(prepared) => prepared.window_id(),
        }
    }
}

pub struct MainWindowRestoreSet {
    services: Arc<MainWindowCreationServices>,
    attempt: RestoredWindowPreparationAttempt,
    discovery: RestoreSetDiscovery,
    activation_source: RestoredWindowActivationSource,
    appearance: Arc<AppearanceGeneration>,
    cancellation: CommandCancellation,
    initial_window: WindowId,
    discovered: bool,
    expected_revision: Option<SessionRevision>,
    expected_windows: Vec<WindowId>,
    remaining: std::collections::VecDeque<SessionWindowRecord>,
    members: Vec<PreparedRestoreSetMember>,
    current: Option<Box<RestoredWindowComposer>>,
    current_ready: bool,
    retiring: Option<Box<RestoredWindowShellUnpublished>>,
    replacement: Option<Box<MainWindowCreation>>,
    disposing: bool,
    error: Option<String>,
    cleanup_error: Option<String>,
    #[cfg(feature = "test-faults")]
    before_claim_activation: Option<Box<dyn FnOnce() + Send>>,
}

pub enum MainWindowRestoreSetOutcome {
    Pending(MainWindowRestoreSet),
    Retained {
        custody: MainWindowRestoreSet,
        reason: RestoreSetRetainedReason,
    },
    Prepared(PreparedMainWindowRestoreSet),
    Failed {
        error: String,
    },
}

pub struct PreparedMainWindowRestoreSet {
    owner: MainWindowRestoreSet,
}

impl PreparedMainWindowRestoreSet {
    pub fn members(&self) -> &[PreparedRestoreSetMember] {
        &self.owner.members
    }

    pub fn revalidate(&self) -> Result<(), String> {
        self.owner.revalidate_complete()
    }

    pub fn dispose(mut self) -> MainWindowRestoreSet {
        self.owner
            .fail("prepared restore set was cancelled".to_owned());
        self.owner
    }
}

impl MainWindowRestoreSet {
    pub fn new(
        services: Arc<MainWindowCreationServices>,
        attempt: RestoredWindowPreparationAttempt,
        activation_source: RestoredWindowActivationSource,
        appearance: Arc<AppearanceGeneration>,
        initial_window: WindowId,
        initial_placement: WindowPlacement,
    ) -> Result<Self, String> {
        attempt.validate_home(&services.store)?;
        if !Arc::ptr_eq(&services.store, &services.acquisition.home_reference()) {
            return Err("restore acquisition services have different source custody".to_owned());
        }
        services
            .state
            .session()
            .revision(&services.store)
            .map_err(|e| e.to_string())?;
        services
            .state
            .runtime_roots()
            .revision(&services.store)
            .map_err(|e| e.to_string())?;
        services
            .storage
            .revision(&services.store)
            .map_err(|e| e.to_string())?;
        let home = appearance.prepared().home();
        if home.home_id() != services.store.home_id()
            || Some(home.home_generation()) != services.store.health().generation()
        {
            return Err("restore appearance belongs to another home generation".to_owned());
        }
        Ok(Self {
            services,
            attempt,
            discovery: RestoreSetDiscovery::new(initial_window, initial_placement),
            activation_source,
            appearance,
            cancellation: CommandCancellation::new(),
            initial_window,
            discovered: false,
            expected_revision: None,
            expected_windows: Vec::new(),
            remaining: std::collections::VecDeque::new(),
            members: Vec::new(),
            current: None,
            current_ready: false,
            retiring: None,
            replacement: None,
            disposing: false,
            error: None,
            cleanup_error: None,
            #[cfg(feature = "test-faults")]
            before_claim_activation: None,
        })
    }

    pub fn cancellation(&self) -> CommandCancellation {
        self.cancellation.clone()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn cleanup_error(&self) -> Option<&str> {
        self.cleanup_error.as_deref()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_before_session_command(&mut self, fault: impl FnOnce() + Send + 'static) {
        self.discovery.test_arm_before_command(fault);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_before_claim_activation(&mut self, fault: impl FnOnce() + Send + 'static) {
        self.before_claim_activation = Some(Box::new(fault));
    }

    pub fn advance(mut self) -> MainWindowRestoreSetOutcome {
        for _ in 0..16 {
            if self.cancellation.is_cancelled() && !self.disposing {
                self.fail("restore set was cancelled".to_owned());
            }
            if self.disposing {
                match self.dispose_step() {
                    Ok(true) => {
                        return MainWindowRestoreSetOutcome::Failed {
                            error: self
                                .error
                                .take()
                                .unwrap_or_else(|| "restore set failed".to_owned()),
                        };
                    }
                    Ok(false) => return self.pending(),
                    Err(error) => {
                        self.cleanup_error = Some(error);
                        return self.pending();
                    }
                }
            }
            if let Err(error) = self.attempt.validate_home(&self.services.store) {
                self.fail(error);
                continue;
            }
            match self.prepare_step() {
                Ok(PreparationStep::Continue) => {}
                Ok(PreparationStep::Pending) => return self.pending(),
                Ok(PreparationStep::Complete) => match self.revalidate_complete() {
                    Ok(()) => {
                        return MainWindowRestoreSetOutcome::Prepared(
                            PreparedMainWindowRestoreSet { owner: self },
                        );
                    }
                    Err(error) => self.fail(error),
                },
                Err(error) => self.fail(error),
            }
        }
        self.pending()
    }

    fn fail(&mut self, error: String) {
        self.error.get_or_insert(error);
        self.disposing = true;
        self.cancellation.cancel();
        self.remaining.clear();
    }

    fn revalidate_complete(&self) -> Result<(), String> {
        self.revalidate_members(self.members.len(), |index, expected| {
            let member = &self.members[index];
            if member.window_id() != expected {
                return Err("prepared restore member identity changed".to_owned());
            }
            match member {
                PreparedRestoreSetMember::Restored(prepared) => prepared.revalidate(&self.attempt),
                PreparedRestoreSetMember::Threadless(prepared) => prepared.revalidate(),
                PreparedRestoreSetMember::Replacement(prepared) => {
                    prepared.revalidate(&self.services.acquisition)
                }
            }
        })
    }

    fn revalidate_members(
        &self,
        count: usize,
        mut validate: impl FnMut(usize, WindowId) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.disposing
            || self.cancellation.is_cancelled()
            || !self.discovered
            || !self.remaining.is_empty()
            || self.current.is_some()
            || self.replacement.is_some()
            || count == 0
            || count != self.expected_windows.len()
            || count > MAX_RESTORABLE_WINDOWS
        {
            return Err("restore set is incomplete or cancelled".to_owned());
        }
        self.attempt.validate_home(&self.services.store)?;
        let before = self
            .services
            .store
            .home_revision()
            .map_err(|e| e.to_string())?;
        let session = self
            .services
            .state
            .session()
            .minimal_bootstrap(&self.services.store)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "restore session is missing".to_owned())?;
        if session.windows().len() != self.expected_windows.len()
            || session
                .windows()
                .iter()
                .map(|w| w.window_id())
                .ne(self.expected_windows.iter().copied())
            || self
                .expected_revision
                .is_some_and(|revision| revision != session.header().revision())
        {
            return Err("complete restore membership or session revision changed".to_owned());
        }
        for (index, expected) in self.expected_windows.iter().copied().enumerate() {
            validate(index, expected)?;
        }
        if self
            .services
            .store
            .home_revision()
            .map_err(|e| e.to_string())?
            != before
        {
            return Err("restore sources changed during complete-set validation".to_owned());
        }
        if self.cancellation.is_cancelled() {
            return Err("restore set was cancelled during complete-set validation".to_owned());
        }
        self.attempt.validate_home(&self.services.store)?;
        Ok(())
    }
}

enum PreparationStep {
    Continue,
    Pending,
    Complete,
}
