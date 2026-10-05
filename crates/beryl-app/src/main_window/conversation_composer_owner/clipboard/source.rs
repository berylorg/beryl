use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicU64, Ordering},
};

use gpui::{App, Global};
use gpui_text_input::{ClipboardProvenanceClosure, MutationKey, RangeSourceSelection};
use syndic_storage::{DraftPieceSettlementKeyV1, DraftPrivateClipboardSourceV1};

use super::super::{MainWindowComposerSelectionIdentity, MainWindowConversationComposerService};

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct MainWindowPrivateClipboardOwner(Arc<Mutex<State>>);

impl Global for MainWindowPrivateClipboardOwner {}

struct State {
    retired: bool,
    identity: u64,
    next_operation: u64,
    preparing: Option<u64>,
    source: Option<Source>,
}

struct Source {
    operation: u64,
    service: Weak<MainWindowConversationComposerService>,
    descriptor: MainWindowPrivateClipboardDescriptor,
    status: Status,
}

#[derive(Clone, Copy)]
enum Status {
    Eligible,
    Cutting(Option<MutationKey>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowPrivateClipboardDescriptor {
    pub clipboard_key: gpui_text_input::ClipboardKey,
    pub origin: MainWindowComposerSelectionIdentity,
    pub content_origin: MainWindowComposerSelectionIdentity,
    pub selection: RangeSourceSelection,
    pub closure: ClipboardProvenanceClosure,
    pub source: DraftPrivateClipboardSourceV1,
}

pub(in super::super) struct PrivateClipboardPreparation {
    owner: MainWindowPrivateClipboardOwner,
    operation: u64,
}

impl Default for MainWindowPrivateClipboardOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl MainWindowPrivateClipboardOwner {
    pub fn new() -> Self {
        let identity = NEXT_OWNER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("private clipboard owner identity exhausted");
        Self(Arc::new(Mutex::new(State {
            retired: false,
            identity,
            next_operation: 1,
            preparing: None,
            source: None,
        })))
    }

    pub fn install(&self, app: &mut App) {
        app.set_global(self.clone());
    }

    pub fn invalidate(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.source = None;
            state.preparing = None;
        }
    }

    pub fn observe_clipboard_metadata(&self, expected_token: &str, metadata: Option<&str>) {
        let Ok(mut state) = self.0.lock() else {
            return;
        };
        if state.source.as_ref().is_some_and(|source| {
            let current = token(state.identity, source.operation);
            current == expected_token && metadata != Some(current.as_str())
        }) {
            state.source = None;
        }
    }

    pub fn retire(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.retired = true;
            state.source = None;
            state.preparing = None;
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn is_retired(&self) -> bool {
        self.0.lock().map_or(true, |state| state.retired)
    }

    pub(in super::super) fn for_app(app: &mut App) -> Self {
        if !app.has_global::<Self>() {
            Self::new().install(app);
        }
        app.global::<Self>().clone()
    }

    pub(in super::super) fn prepare(&self) -> Result<PrivateClipboardPreparation, String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "private clipboard owner is unavailable")?;
        if state.retired
            || state.preparing.is_some()
            || state
                .source
                .as_ref()
                .is_some_and(|source| matches!(source.status, Status::Cutting(_)))
        {
            return Err("private clipboard capacity is temporarily unavailable".into());
        }
        let operation = state.next_operation;
        state.next_operation = operation
            .checked_add(1)
            .ok_or("private clipboard identity exhausted")?;
        state.preparing = Some(operation);
        Ok(PrivateClipboardPreparation {
            owner: self.clone(),
            operation,
        })
    }

    pub fn descriptor(&self, metadata: &str) -> Option<MainWindowPrivateClipboardDescriptor> {
        let (descriptor, service, operation, identity) = {
            let state = self.0.lock().ok()?;
            let source = state.source.as_ref()?;
            if !matches!(source.status, Status::Eligible) {
                return None;
            }
            (
                source.descriptor,
                source.service.clone(),
                source.operation,
                state.identity,
            )
        };
        if metadata != token(identity, operation) {
            return None;
        }
        let valid = service.upgrade().is_some_and(|service| {
            let health = service.store.health();
            health.state() == beryl_home_store::HomeHealthState::Healthy
                && health.generation() == Some(descriptor.origin.binding().home_generation())
                && service
                    .selected_identity()
                    .is_some_and(|current| same_candidate(current, descriptor.origin))
        });
        if !valid {
            let mut state = self.0.lock().ok()?;
            if state
                .source
                .as_ref()
                .is_some_and(|source| source.operation == operation)
            {
                state.source = None;
            }
            return None;
        }
        let state = self.0.lock().ok()?;
        state
            .source
            .as_ref()
            .filter(|source| {
                source.operation == operation && matches!(source.status, Status::Eligible)
            })
            .map(|source| source.descriptor)
    }

    pub(in super::super) fn expire_origin(&self, origin: MainWindowComposerSelectionIdentity) {
        if let Ok(mut state) = self.0.lock()
            && state
                .source
                .as_ref()
                .is_some_and(|source| same_session(source.descriptor.origin, origin))
        {
            state.source = None;
        }
    }

    pub(in super::super) fn expire_operation(&self, operation: gpui_text_input::ClipboardId) {
        if let Ok(mut state) = self.0.lock()
            && state
                .source
                .as_ref()
                .is_some_and(|source| source.descriptor.clipboard_key.id() == operation)
        {
            state.source = None;
        }
    }

    pub(in super::super) fn bind_cut(
        &self,
        origin: MainWindowComposerSelectionIdentity,
        key: MutationKey,
    ) {
        if let Ok(mut state) = self.0.lock()
            && let Some(source) = state.source.as_mut()
            && same_candidate(source.descriptor.origin, origin)
            && matches!(source.status, Status::Cutting(None))
        {
            source.status = Status::Cutting(Some(key));
        }
    }

    pub(in super::super) fn noncommit(
        &self,
        origin: MainWindowComposerSelectionIdentity,
        key: Option<MutationKey>,
    ) {
        if let Ok(mut state) = self.0.lock()
            && let Some(source) = state.source.as_mut()
            && same_candidate(source.descriptor.origin, origin)
            && matches!(source.status, Status::Cutting(actual) if actual == key)
        {
            source.descriptor.origin = origin;
            source.status = Status::Eligible;
        }
    }

    pub(in super::super) fn adopted(
        &self,
        previous: MainWindowComposerSelectionIdentity,
        successor: MainWindowComposerSelectionIdentity,
        cut: Option<(MutationKey, DraftPieceSettlementKeyV1)>,
    ) {
        let Ok(mut state) = self.0.lock() else {
            return;
        };
        let Some(source) = state.source.as_mut() else {
            return;
        };
        if !same_session(source.descriptor.origin, previous) {
            return;
        }
        if let Some((key, settlement)) = cut
            && same_candidate(source.descriptor.origin, previous)
            && matches!(source.status, Status::Cutting(Some(actual)) if actual == key)
        {
            let candidate = successor.binding().candidate();
            let Some(selector) = DraftPrivateClipboardSourceV1::from_committed_cut(
                settlement,
                candidate.candidate_generation(),
                candidate.root(),
                source.descriptor.content_origin.binding().root(),
            ) else {
                state.source = None;
                return;
            };
            source.descriptor.source = selector;
            source.descriptor.origin = successor;
            source.status = Status::Eligible;
        } else if !same_candidate(source.descriptor.origin, successor) {
            state.source = None;
        } else {
            source.descriptor.origin = successor;
        }
    }
}

impl PrivateClipboardPreparation {
    pub(in super::super) fn begin_write(&self) -> Result<String, String> {
        let mut state = self
            .owner
            .0
            .lock()
            .map_err(|_| "private clipboard owner is unavailable")?;
        if state.preparing != Some(self.operation) {
            return Err("private clipboard preparation expired".into());
        }
        state.source = None;
        Ok(token(state.identity, self.operation))
    }

    pub(in super::super) fn publish(
        &self,
        service: &Arc<MainWindowConversationComposerService>,
        descriptor: MainWindowPrivateClipboardDescriptor,
        cut: bool,
    ) -> Result<(), String> {
        let mut state = self
            .owner
            .0
            .lock()
            .map_err(|_| "private clipboard owner is unavailable")?;
        if state.preparing != Some(self.operation) {
            return Err("private clipboard publication expired".into());
        }
        state.source = Some(Source {
            operation: self.operation,
            service: Arc::downgrade(service),
            descriptor,
            status: if cut {
                Status::Cutting(None)
            } else {
                Status::Eligible
            },
        });
        Ok(())
    }
}

impl Drop for PrivateClipboardPreparation {
    fn drop(&mut self) {
        if let Ok(mut state) = self.owner.0.lock()
            && state.preparing == Some(self.operation)
        {
            state.preparing = None;
        }
    }
}

fn token(identity: u64, operation: u64) -> String {
    static PROCESS: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
    let process = PROCESS.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("process clock predates epoch")
            .as_nanos()
    });
    format!(
        "beryl.private-composer.v1:{process}:{}:{identity}:{operation}",
        std::process::id()
    )
}

fn same_session(
    left: MainWindowComposerSelectionIdentity,
    right: MainWindowComposerSelectionIdentity,
) -> bool {
    let a = left.binding();
    let b = right.binding();
    a.home_id() == b.home_id()
        && a.home_generation() == b.home_generation()
        && a.candidate().draft_id() == b.candidate().draft_id()
        && a.candidate().session_id() == b.candidate().session_id()
}

fn same_candidate(
    left: MainWindowComposerSelectionIdentity,
    right: MainWindowComposerSelectionIdentity,
) -> bool {
    same_session(left, right)
        && left.binding().candidate().candidate_generation()
            == right.binding().candidate().candidate_generation()
        && left.binding().root() == right.binding().root()
        && left.binding().history() == right.binding().history()
}
