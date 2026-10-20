use std::{
    collections::{HashMap, VecDeque},
    num::NonZeroU64,
    ops::Range,
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    thread::JoinHandle,
};

use beryl_home_store::HomeServiceReference;
use beryl_state::{
    InstalledThemeId, InstalledThemeSelection, PreparedThemeAppearance, ThemeHomeIdentity,
    ThemeObservedDocument, ThemeReconciliation, ThemeReferenceSnapshotProvider,
    ThemeRepositoryCommand, ThemeRepositoryCommit, ThemeRepositoryObservation,
    ThemeRepositoryOperationOutcome, ThemeSettingsIdentity,
};

use super::{ThemeRuntimeFailureClass, ThemeSettingsSource};

mod worker;

const READ_SLOTS: usize = 16;
static NEXT_BROKER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct ThemeSettingsCatalog {
    pub observation: ThemeRepositoryObservation,
    pub logical_item_count: usize,
}

#[derive(Clone, Debug)]
pub struct ThemeSettingsPage {
    pub observation: ThemeRepositoryObservation,
    pub range: Range<usize>,
    pub selections: Vec<InstalledThemeSelection>,
    pub focus: Option<ThemeSettingsFocus>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeSettingsFocus {
    Found { position: usize },
    Removed,
}

pub enum ThemeSettingsWork {
    Observe,
    PageRange {
        observation: ThemeRepositoryObservation,
        range: Range<usize>,
        focus: Option<InstalledThemeId>,
    },
    LoadDocument {
        observation: ThemeRepositoryObservation,
        selection: InstalledThemeSelection,
    },
    Execute {
        observation: ThemeRepositoryObservation,
        command: ThemeRepositoryCommand,
        references: Arc<dyn ThemeReferenceSnapshotProvider + Send + Sync>,
    },
    Reconcile {
        operation: NonZeroU64,
    },
}

impl ThemeSettingsWork {
    fn is_command(&self) -> bool {
        matches!(self, Self::Execute { .. } | Self::Reconcile { .. })
    }
}

#[derive(Debug)]
pub enum ThemeSettingsResult {
    Catalog(ThemeSettingsCatalog),
    Page(ThemeSettingsPage),
    Document(ThemeObservedDocument),
    Repository(Arc<ThemeRepositoryOperationOutcome>),
    Reconciled(Arc<ThemeReconciliation>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeSettingsBridgeError {
    Retired,
    Capacity,
    WrongTicket,
    Cancelled,
    Worker,
    Theme(ThemeRuntimeFailureClass),
    NotStarted(ThemeRuntimeFailureClass),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThemeSettingsTicket {
    broker: u64,
    request: u64,
}

pub struct ThemeSettingsCompletion {
    pub ticket: ThemeSettingsTicket,
    pub not_started: Option<ThemeRuntimeFailureClass>,
    pub appearance_target: Option<InstalledThemeId>,
    pub home: ThemeHomeIdentity,
    pub publication: Option<ThemeRepositoryCommit>,
    pub unresolved: Option<NonZeroU64>,
    pub prepared: Option<PreparedThemeAppearance>,
    pub repository: Option<Arc<ThemeRepositoryOperationOutcome>>,
    pub reconciliation: Option<Arc<ThemeReconciliation>>,
    pub preparation_failure: Option<ThemeRuntimeFailureClass>,
}

impl ThemeSettingsCompletion {
    fn not_started(
        ticket: ThemeSettingsTicket,
        home: ThemeHomeIdentity,
        failure: ThemeRuntimeFailureClass,
    ) -> Self {
        Self {
            ticket,
            home,
            not_started: Some(failure),
            appearance_target: None,
            publication: None,
            unresolved: None,
            prepared: None,
            repository: None,
            reconciliation: None,
            preparation_failure: None,
        }
    }
}

#[derive(Default)]
pub struct ThemeSettingsRetirement {
    pub unresolved: Option<NonZeroU64>,
    pub completion: Option<ThemeSettingsCompletion>,
    pub worker_failed: bool,
}

enum SlotState {
    Queued(ThemeSettingsWork),
    Running,
    Ready(Result<ThemeSettingsResult, ThemeSettingsBridgeError>),
}

struct Slot {
    command: bool,
    cancelled: bool,
    state: SlotState,
}

struct State {
    closing: bool,
    stopped: bool,
    worker_failed: bool,
    next_request: u64,
    slots: HashMap<u64, Slot>,
    queue: VecDeque<u64>,
    completion: Option<ThemeSettingsCompletion>,
    unresolved: Option<NonZeroU64>,
    unresolved_ticket: Option<ThemeSettingsTicket>,
    unresolved_target: Option<InstalledThemeId>,
    selection: Option<(ThemeSettingsIdentity, Option<InstalledThemeId>)>,
}

struct Shared {
    id: u64,
    home: ThemeHomeIdentity,
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Clone)]
pub struct SettingsThemeReader {
    shared: Weak<Shared>,
    home: ThemeHomeIdentity,
}

impl SettingsThemeReader {
    pub fn home(&self) -> ThemeHomeIdentity {
        self.home
    }

    pub fn submit(
        &self,
        work: ThemeSettingsWork,
    ) -> Result<ThemeSettingsTicket, ThemeSettingsBridgeError> {
        let shared = self
            .shared
            .upgrade()
            .ok_or(ThemeSettingsBridgeError::Retired)?;
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closing || state.stopped {
            return Err(ThemeSettingsBridgeError::Retired);
        }
        let command = work.is_command();
        if matches!(&work, ThemeSettingsWork::Execute { .. }) && state.unresolved.is_some() {
            return Err(ThemeSettingsBridgeError::Theme(
                ThemeRuntimeFailureClass::Reconciliation,
            ));
        }
        if let ThemeSettingsWork::Reconcile { operation } = &work {
            if state.unresolved != Some(*operation) {
                return Err(ThemeSettingsBridgeError::WrongTicket);
            }
        }
        if if command {
            state.completion.is_some() || state.slots.values().any(|slot| slot.command)
        } else {
            state.slots.values().filter(|slot| !slot.command).count() >= READ_SLOTS
        } {
            return Err(ThemeSettingsBridgeError::Capacity);
        }
        let request = state.next_request;
        state.next_request = request
            .checked_add(1)
            .ok_or(ThemeSettingsBridgeError::Capacity)?;
        state.slots.insert(
            request,
            Slot {
                command,
                cancelled: false,
                state: SlotState::Queued(work),
            },
        );
        state.queue.push_back(request);
        shared.changed.notify_one();
        Ok(ThemeSettingsTicket {
            broker: shared.id,
            request,
        })
    }

    pub fn poll(
        &self,
        ticket: ThemeSettingsTicket,
    ) -> Result<Option<ThemeSettingsResult>, ThemeSettingsBridgeError> {
        let shared = self
            .shared
            .upgrade()
            .ok_or(ThemeSettingsBridgeError::Retired)?;
        if shared.id != ticket.broker {
            return Err(ThemeSettingsBridgeError::WrongTicket);
        }
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let Some(slot) = state.slots.get(&ticket.request) else {
            return Err(ThemeSettingsBridgeError::WrongTicket);
        };
        if !matches!(slot.state, SlotState::Ready(_)) {
            return Ok(None);
        }
        let slot = state.slots.remove(&ticket.request).expect("observed slot");
        match slot.state {
            SlotState::Ready(result) => result.map(Some),
            _ => unreachable!(),
        }
    }

    pub fn cancel(&self, ticket: ThemeSettingsTicket) -> Result<(), ThemeSettingsBridgeError> {
        let shared = self
            .shared
            .upgrade()
            .ok_or(ThemeSettingsBridgeError::Retired)?;
        if shared.id != ticket.broker {
            return Err(ThemeSettingsBridgeError::WrongTicket);
        }
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let slot = state
            .slots
            .get_mut(&ticket.request)
            .ok_or(ThemeSettingsBridgeError::WrongTicket)?;
        if slot.command {
            return Err(ThemeSettingsBridgeError::Capacity);
        }
        if matches!(slot.state, SlotState::Running) {
            slot.cancelled = true;
        } else {
            state.slots.remove(&ticket.request);
            state.queue.retain(|request| *request != ticket.request);
        }
        Ok(())
    }
}

pub struct SettingsThemeBroker {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl SettingsThemeBroker {
    pub fn start(
        home: HomeServiceReference,
        source: ThemeSettingsSource,
    ) -> Result<Self, ThemeSettingsBridgeError> {
        source
            .check(&home)
            .map_err(ThemeSettingsBridgeError::Theme)?;
        let id = NEXT_BROKER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| ThemeSettingsBridgeError::Capacity)?;
        let shared = Arc::new(Shared {
            id,
            home: source.home(),
            changed: Condvar::new(),
            state: Mutex::new(State {
                closing: false,
                stopped: false,
                worker_failed: false,
                next_request: 1,
                slots: HashMap::new(),
                queue: VecDeque::new(),
                completion: None,
                unresolved: None,
                unresolved_ticket: None,
                unresolved_target: None,
                selection: None,
            }),
        });
        let signal = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("settings-theme".into())
            .spawn(move || worker::run(home, source, signal))
            .map_err(|_| ThemeSettingsBridgeError::Worker)?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    pub fn reader(&self) -> SettingsThemeReader {
        SettingsThemeReader {
            shared: Arc::downgrade(&self.shared),
            home: self.shared.home,
        }
    }

    pub fn set_active_selection(
        &self,
        selection: Option<(ThemeSettingsIdentity, Option<InstalledThemeId>)>,
    ) -> Result<(), ThemeSettingsBridgeError> {
        if selection
            .as_ref()
            .is_some_and(|(settings, _)| settings.home() != self.shared.home)
        {
            return Err(ThemeSettingsBridgeError::Theme(
                ThemeRuntimeFailureClass::Identity,
            ));
        }
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .selection = selection;
        Ok(())
    }

    pub fn take_completion(&self) -> Option<ThemeSettingsCompletion> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .completion
            .take()
    }

    pub fn retire(&mut self) -> ThemeSettingsRetirement {
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(request) = state.slots.iter().find_map(|(request, slot)| {
                matches!(
                    slot.state,
                    SlotState::Queued(ThemeSettingsWork::Execute { .. })
                )
                .then_some(*request)
            }) {
                state.completion = Some(ThemeSettingsCompletion::not_started(
                    ThemeSettingsTicket {
                        broker: self.shared.id,
                        request,
                    },
                    self.shared.home,
                    ThemeRuntimeFailureClass::Retired,
                ));
            }
            state.closing = true;
            self.shared.changed.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.slots.clear();
        state.queue.clear();
        ThemeSettingsRetirement {
            unresolved: state.unresolved,
            completion: state.completion.take(),
            worker_failed: state.worker_failed,
        }
    }
}

impl Drop for SettingsThemeBroker {
    fn drop(&mut self) {
        self.retire();
    }
}
