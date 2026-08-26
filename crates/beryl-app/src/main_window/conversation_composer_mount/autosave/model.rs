use beryl_state::AssetState;
use gpui::Task;

use crate::composer_host::{
    ComposerHostAutosaveInterval, ComposerHostAutosaveTimer, ComposerHostPublicationTicket,
};
use crate::composer_marker_seal::DraftMarkerSealService;

use super::super::super::MainWindowComposerSelectionIdentity;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowConversationComposerAutosavePhase {
    Idle,
    Waiting,
    Publishing,
    Ready,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MainWindowConversationComposerAutosaveDiagnostics {
    phase: MainWindowConversationComposerAutosavePhase,
    generation: u64,
    retained_tasks: usize,
    fenced: bool,
    last_error: Option<String>,
}

impl MainWindowConversationComposerAutosaveDiagnostics {
    pub const fn phase(&self) -> MainWindowConversationComposerAutosavePhase {
        self.phase
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub const fn retained_tasks(&self) -> usize {
        self.retained_tasks
    }

    pub const fn fenced(&self) -> bool {
        self.fenced
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

#[derive(Clone, Copy)]
pub(in crate::main_window) enum AutosaveState {
    Idle,
    Waiting {
        selection: MainWindowComposerSelectionIdentity,
        timer: ComposerHostAutosaveTimer,
    },
    Publishing {
        selection: MainWindowComposerSelectionIdentity,
        ticket: ComposerHostPublicationTicket,
    },
    Ready {
        selection: MainWindowComposerSelectionIdentity,
        ticket: ComposerHostPublicationTicket,
    },
}

pub(in crate::main_window) struct MainWindowConversationComposerAutosave {
    pub(super) state: AutosaveState,
    pub(super) generation: u64,
    pub(super) task: Option<Task<()>>,
    pub(super) fenced: bool,
    pub(super) assets: AssetState,
    pub(super) marker_seals: DraftMarkerSealService,
    pub(super) settings: Option<(u64, ComposerHostAutosaveInterval)>,
    pub(super) last_error: Option<String>,
    #[cfg(feature = "test-faults")]
    pub(super) hold_ready_once: bool,
}

impl MainWindowConversationComposerAutosave {
    pub(in crate::main_window) fn new(
        assets: AssetState,
        marker_seals: DraftMarkerSealService,
    ) -> Self {
        Self {
            state: AutosaveState::Idle,
            generation: 0,
            task: None,
            fenced: false,
            assets,
            marker_seals,
            settings: None,
            last_error: None,
            #[cfg(feature = "test-faults")]
            hold_ready_once: false,
        }
    }

    pub(super) fn diagnostics(&self) -> MainWindowConversationComposerAutosaveDiagnostics {
        MainWindowConversationComposerAutosaveDiagnostics {
            phase: match self.state {
                AutosaveState::Idle => MainWindowConversationComposerAutosavePhase::Idle,
                AutosaveState::Waiting { .. } => {
                    MainWindowConversationComposerAutosavePhase::Waiting
                }
                AutosaveState::Publishing { .. } => {
                    MainWindowConversationComposerAutosavePhase::Publishing
                }
                AutosaveState::Ready { .. } => MainWindowConversationComposerAutosavePhase::Ready,
            },
            generation: self.generation,
            retained_tasks: usize::from(self.task.is_some()),
            fenced: self.fenced,
            last_error: self.last_error.clone(),
        }
    }

    pub(in crate::main_window) fn record_error(&mut self, error: String) {
        self.last_error = Some(error);
    }

    pub(super) fn advance_generation(&mut self) -> Result<u64, String> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| "conversation composer autosave generation exhausted".to_owned())?;
        Ok(self.generation)
    }

    pub(super) fn suspend(&mut self) -> Result<(), String> {
        self.advance_generation()?;
        self.task = None;
        self.state = AutosaveState::Idle;
        Ok(())
    }

    pub(super) fn selection_advanced(
        &mut self,
        previous: MainWindowComposerSelectionIdentity,
        current: MainWindowComposerSelectionIdentity,
    ) -> bool {
        match &mut self.state {
            AutosaveState::Waiting { selection, .. }
            | AutosaveState::Publishing { selection, .. }
            | AutosaveState::Ready { selection, .. }
                if *selection == previous =>
            {
                *selection = current;
                true
            }
            AutosaveState::Idle => false,
            _ => false,
        }
    }
}
