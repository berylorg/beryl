use std::{
    num::NonZeroU64,
    sync::{Arc, Mutex, Weak, atomic::Ordering},
};

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, HomeCandidateRecoveryAccess, HomeCommand,
    HomeHealthState, ReconciliationHandle, ReconciliationResolution,
};
use beryl_model::{BerylHomeId, DomainRevision};
use beryl_state::{
    ApplySettings, ExpectedSettingRevision, InstalledThemeId, RecordRevision, SettingUpdate,
    SettingsState, ThemeDraftIdentity, ThemeDraftRevision,
};

use super::{
    draft::{SettingsDraft, validate_value},
    model::*,
    source::*,
};
use crate::theme_runtime::ConfirmedSettingsTheme;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsApplyAction {
    Apply,
    Ok,
}

#[derive(Clone, Debug)]
pub enum SettingsApplyObservation {
    Committed {
        operation: u64,
        binding: SettingsDraftBinding,
        action: SettingsApplyAction,
        baseline: Arc<SettingsBaseline>,
        theme: Option<ConfirmedSettingsTheme>,
    },
    NotCommitted {
        operation: u64,
        binding: SettingsDraftBinding,
        action: SettingsApplyAction,
        issue: SettingsIssue,
    },
    Reconciling {
        operation: u64,
        binding: SettingsDraftBinding,
        action: SettingsApplyAction,
    },
    ScopedUnresolved {
        operation: u64,
        binding: SettingsDraftBinding,
        action: SettingsApplyAction,
        baseline: Arc<SettingsBaseline>,
        issue: SettingsIssue,
    },
}
impl SettingsApplyObservation {
    pub fn operation(&self) -> u64 {
        match self {
            Self::Committed { operation, .. }
            | Self::NotCommitted { operation, .. }
            | Self::Reconciling { operation, .. }
            | Self::ScopedUnresolved { operation, .. } => *operation,
        }
    }
    pub fn binding(&self) -> SettingsDraftBinding {
        match self {
            Self::Committed { binding, .. }
            | Self::NotCommitted { binding, .. }
            | Self::Reconciling { binding, .. }
            | Self::ScopedUnresolved { binding, .. } => *binding,
        }
    }
    pub fn action(&self) -> SettingsApplyAction {
        match self {
            Self::Committed { action, .. }
            | Self::NotCommitted { action, .. }
            | Self::Reconciling { action, .. }
            | Self::ScopedUnresolved { action, .. } => *action,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsThemeRequest {
    pub binding: SettingsDraftBinding,
    pub operation: u64,
    pub intended_domain_revision: DomainRevision,
    pub intended_active_record_revision: RecordRevision,
    pub active: InstalledThemeId,
}
impl SettingsThemeRequest {
    pub fn draft(&self) -> ThemeDraftIdentity {
        ThemeDraftIdentity::new(
            NonZeroU64::new(self.binding.window_instance).expect("validated window identity"),
        )
    }
    pub fn revision(&self) -> ThemeDraftRevision {
        ThemeDraftRevision::new(
            NonZeroU64::new(self.binding.draft_revision).expect("validated draft revision"),
        )
    }
}
#[derive(Clone, Debug)]
pub struct SettingsThemePreparation {
    request: SettingsThemeRequest,
    confirmed: ConfirmedSettingsTheme,
}
impl SettingsThemePreparation {
    pub fn new(
        request: SettingsThemeRequest,
        confirmed: ConfirmedSettingsTheme,
    ) -> Result<Self, SettingsServiceError> {
        if request.binding.window_instance == 0 || request.binding.draft_revision == 0 {
            return Err(SettingsServiceError::Theme);
        }
        let intended = confirmed.committed();
        let appearance_matches = matches!(confirmed.prepared().source(), beryl_state::ThemeAppearanceSource::Installed(document) if document.theme_id() == &request.active);
        if confirmed.draft() != request.draft()
            || confirmed.revision() != request.revision()
            || intended.home().home_id() != request.binding.source.home_id
            || intended.home().home_generation() != request.binding.source.home_generation
            || intended.domain_revision() != request.intended_domain_revision
            || intended.active_record_revision() != Some(request.intended_active_record_revision)
            || confirmed.active() != Some(&request.active)
            || confirmed.prepared().settings() != intended
            || !appearance_matches
        {
            return Err(SettingsServiceError::Theme);
        }
        Ok(Self { request, confirmed })
    }
}

enum OriginalOutcome {
    Unexecuted,
    Direct(CommandOutcome),
    Installed {
        failure: CommandError,
        handle: ReconciliationHandle,
        resolution: Option<ReconciliationResolution>,
    },
}
struct OperationState {
    original: OriginalOutcome,
    observation: Option<SettingsApplyObservation>,
    theme: Option<SettingsThemePreparation>,
}
struct SettingsOperation {
    id: u64,
    binding: SettingsDraftBinding,
    action: SettingsApplyAction,
    before: Arc<SettingsBaseline>,
    after: [Option<SettingsRecordFacts>; 5],
    modified: u8,
    target: DomainRevision,
    theme_request: Option<SettingsThemeRequest>,
    state: Mutex<OperationState>,
}
struct ProcessFacts {
    home: BerylHomeId,
    excluded: u8,
    active: Option<Arc<SettingsOperation>>,
    latest: Option<Arc<SettingsOperation>>,
    unresolved: Vec<Arc<SettingsOperation>>,
    last_operation: u64,
    allocated_operation: u64,
}
#[derive(Clone)]
pub struct SettingsProcessOwner {
    facts: Arc<Mutex<ProcessFacts>>,
}

pub struct PreparedSettingsApply {
    service: Weak<ServiceInner>,
    operation: Arc<SettingsOperation>,
    consumed: bool,
}
impl PreparedSettingsApply {
    pub fn binding(&self) -> SettingsDraftBinding {
        self.operation.binding
    }
    pub fn operation(&self) -> u64 {
        self.operation.id
    }
    pub fn theme_request(&self) -> Option<&SettingsThemeRequest> {
        self.operation.theme_request.as_ref()
    }
    pub fn set_theme_preparation(
        &mut self,
        prepared: SettingsThemePreparation,
    ) -> Result<(), SettingsServiceError> {
        if self.operation.theme_request.as_ref() != Some(&prepared.request) {
            return Err(SettingsServiceError::Theme);
        }
        self.operation
            .state
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?
            .theme = Some(prepared);
        Ok(())
    }
}
impl Drop for PreparedSettingsApply {
    fn drop(&mut self) {
        if self.consumed {
            return;
        }
        if let Some(service) = self.service.upgrade() {
            let unexecuted = self
                .operation
                .state
                .lock()
                .map(|mut state| {
                    if matches!(state.original, OriginalOutcome::Unexecuted) {
                        state.observation = Some(not_committed_observation(&self.operation));
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if unexecuted {
                let _ = service.owner.release(&self.operation, false);
            }
        }
    }
}

mod custody;
mod process;
mod service;
use custody::*;
