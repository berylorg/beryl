use std::{error::Error, fmt};

use beryl_home_store::{CommandCancellation, CursorReadLimits, HomeStore, ReadError};
use beryl_model::{ClaimRevision, SessionRevision, SyndicThreadId, WindowId, WindowPlacement};

use crate::{
    BerylState, CatalogClaimKind, CatalogReadError, CatalogRecencyCursor, CatalogRevision,
    CatalogRow, RememberedTarget, SessionReadError, ThreadClaimState,
    session::SessionAcquisitionSource,
};

const CATALOG_AUDIT_PAGE_ITEMS: usize = 16;
const CATALOG_AUDIT_PAGE_BYTES: usize = 512 * 1024;

#[cfg(feature = "test-faults")]
pub type WindowAcquisitionAuditPageHook =
    std::sync::Arc<dyn Fn(WindowId, usize) + Send + Sync + 'static>;

#[cfg(feature = "test-faults")]
static CATALOG_AUDIT_PAGE_HOOK: std::sync::OnceLock<
    std::sync::Mutex<Option<WindowAcquisitionAuditPageHook>>,
> = std::sync::OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowAcquisitionThreadOrigin {
    Reused,
    CreatedFallback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowAcquisitionCommittedFacts {
    window_id: WindowId,
    thread_id: SyndicThreadId,
    target: RememberedTarget,
    placement: WindowPlacement,
    session_revision: SessionRevision,
    claim_generation: SessionRevision,
    claim_revision: ClaimRevision,
    fallback_target: RememberedTarget,
    origin: WindowAcquisitionThreadOrigin,
}

impl WindowAcquisitionCommittedFacts {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn target(&self) -> RememberedTarget {
        self.target
    }

    #[must_use]
    pub fn placement(&self) -> &WindowPlacement {
        &self.placement
    }

    #[must_use]
    pub const fn session_revision(&self) -> SessionRevision {
        self.session_revision
    }

    #[must_use]
    pub const fn claim_generation(&self) -> SessionRevision {
        self.claim_generation
    }

    #[must_use]
    pub const fn claim_revision(&self) -> ClaimRevision {
        self.claim_revision
    }

    #[must_use]
    pub const fn fallback_target(&self) -> RememberedTarget {
        self.fallback_target
    }

    #[must_use]
    pub const fn origin(&self) -> WindowAcquisitionThreadOrigin {
        self.origin
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WindowAcquisitionNaturalState {
    Missing,
    Committed(WindowAcquisitionCommittedFacts),
    Collision,
}

#[derive(Debug)]
pub enum WindowAcquisitionAuditError {
    Store(ReadError),
    Session(SessionReadError),
    Catalog(CatalogReadError),
    RepairNeeded { thread_id: SyndicThreadId },
    Cancelled,
    ConcurrentPublication,
}

impl fmt::Display for WindowAcquisitionAuditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(source) => source.fmt(formatter),
            Self::Session(source) => source.fmt(formatter),
            Self::Catalog(source) => source.fmt(formatter),
            Self::RepairNeeded { thread_id } => {
                write!(
                    formatter,
                    "catalog row {thread_id} requires repair before window audit"
                )
            }
            Self::Cancelled => formatter.write_str("window acquisition audit was cancelled"),
            Self::ConcurrentPublication => {
                formatter.write_str("window acquisition state changed during exact audit")
            }
        }
    }
}

impl Error for WindowAcquisitionAuditError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(source) => Some(source),
            Self::Session(source) => Some(source),
            Self::Catalog(source) => Some(source),
            Self::RepairNeeded { .. } => None,
            Self::Cancelled => None,
            Self::ConcurrentPublication => None,
        }
    }
}

impl BerylState {
    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn set_window_acquisition_audit_page_hook_for_test(
        &self,
        hook: Option<WindowAcquisitionAuditPageHook>,
    ) {
        *CATALOG_AUDIT_PAGE_HOOK
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = hook;
    }

    pub fn audit_window_acquisition(
        &self,
        store: &HomeStore,
        window_id: WindowId,
    ) -> Result<WindowAcquisitionNaturalState, WindowAcquisitionAuditError> {
        self.audit_window_acquisition_with_cancellation(
            store,
            window_id,
            &CommandCancellation::new(),
        )
    }

    pub fn audit_window_acquisition_with_cancellation(
        &self,
        store: &HomeStore,
        window_id: WindowId,
        cancellation: &CommandCancellation,
    ) -> Result<WindowAcquisitionNaturalState, WindowAcquisitionAuditError> {
        if cancellation.is_cancelled() {
            return Err(WindowAcquisitionAuditError::Cancelled);
        }
        let home_revision = store
            .home_revision()
            .map_err(WindowAcquisitionAuditError::Store)?;
        let session_revision = self
            .session()
            .revision(store)
            .map_err(WindowAcquisitionAuditError::Store)?;
        let catalog_scan = self
            .catalog()
            .begin_current_scan(store)
            .map_err(map_catalog_start)?;

        let session = match self.session().acquisition_source(store, window_id) {
            Ok(source) => classify_session(window_id, source),
            Err(SessionReadError::ConcurrentPublication) => {
                return Err(WindowAcquisitionAuditError::ConcurrentPublication);
            }
            Err(error @ SessionReadError::Read(_)) => {
                return Err(WindowAcquisitionAuditError::Session(error));
            }
            Err(_) => SessionClassification::Collision,
        };
        let catalog = scan_catalog(self, store, window_id, catalog_scan, cancellation)?;

        let current_session_revision = self
            .session()
            .revision(store)
            .map_err(WindowAcquisitionAuditError::Store)?;
        let current_catalog_revision = self
            .catalog()
            .revision(store)
            .map_err(WindowAcquisitionAuditError::Store)?;
        let current_home_revision = store
            .home_revision()
            .map_err(WindowAcquisitionAuditError::Store)?;
        if current_home_revision != home_revision
            || current_session_revision != session_revision
            || current_catalog_revision != catalog_scan.revision()
        {
            return Err(WindowAcquisitionAuditError::ConcurrentPublication);
        }

        Ok(combine(session, catalog))
    }
}

enum SessionClassification {
    Missing,
    Exact(SessionExactFacts),
    Collision,
}

struct SessionExactFacts {
    window_id: WindowId,
    thread_id: SyndicThreadId,
    target: RememberedTarget,
    placement: WindowPlacement,
    session_revision: SessionRevision,
    claim_generation: SessionRevision,
    claim_revision: ClaimRevision,
    fallback_target: RememberedTarget,
}

enum CatalogClassification {
    Missing,
    Exact(CatalogRow),
    Collision,
}

fn classify_session(
    window_id: WindowId,
    source: SessionAcquisitionSource,
) -> SessionClassification {
    if !source.claims_bounded {
        return SessionClassification::Collision;
    }
    let relevant_by_window: Vec<_> = source
        .claims_by_window
        .iter()
        .filter(|(key, claim)| *key == window_id || claim.window_id() == window_id)
        .collect();
    let relevant_by_thread: Vec<_> = source
        .claims_by_thread
        .iter()
        .filter(|(_, claim)| claim.window_id() == window_id)
        .collect();
    let Some(bootstrap) = source.bootstrap else {
        return if source.window.is_none()
            && relevant_by_window.is_empty()
            && relevant_by_thread.is_empty()
        {
            SessionClassification::Missing
        } else {
            SessionClassification::Collision
        };
    };
    let header = bootstrap.header();
    let references: Vec<_> = header
        .windows()
        .iter()
        .filter(|reference| reference.window_id() == window_id)
        .collect();
    let windows: Vec<_> = bootstrap
        .windows()
        .iter()
        .filter(|window| window.window_id() == window_id)
        .collect();
    let has_session_part = source.window.is_some()
        || !references.is_empty()
        || !windows.is_empty()
        || !relevant_by_window.is_empty()
        || !relevant_by_thread.is_empty();
    if !has_session_part {
        return SessionClassification::Missing;
    }
    if references.len() != 1
        || windows.len() != 1
        || relevant_by_window.len() != 1
        || relevant_by_thread.len() != 1
    {
        return SessionClassification::Collision;
    }
    let window = windows[0];
    if source.window.as_ref() != Some(window) {
        return SessionClassification::Collision;
    }
    let Some(target) = window.remembered_target() else {
        return SessionClassification::Collision;
    };
    let Some(selection) = window.selected_thread() else {
        return SessionClassification::Collision;
    };
    let (window_key, by_window) = relevant_by_window[0];
    let (thread_key, by_thread) = relevant_by_thread[0];
    let same_thread_by_window = source
        .claims_by_window
        .iter()
        .filter(|(_, claim)| claim.thread_id() == selection.thread_id())
        .count();
    let same_thread_by_thread = source
        .claims_by_thread
        .iter()
        .filter(|(key, claim)| {
            *key == selection.thread_id() || claim.thread_id() == selection.thread_id()
        })
        .count();
    if *window_key != window_id
        || *thread_key != selection.thread_id()
        || *by_window != *by_thread
        || by_window.window_id() != window_id
        || by_window.thread_id() != selection.thread_id()
        || by_window.selection() != selection
        || by_window.state() != ThreadClaimState::Active
        || same_thread_by_window != 1
        || same_thread_by_thread != 1
        || by_window.generation() != header.revision()
        || header.fallback() != Some(target)
    {
        return SessionClassification::Collision;
    }
    SessionClassification::Exact(SessionExactFacts {
        window_id,
        thread_id: selection.thread_id(),
        target,
        placement: window.placement().clone(),
        session_revision: header.revision(),
        claim_generation: by_window.generation(),
        claim_revision: by_window.revision(),
        fallback_target: target,
    })
}

fn scan_catalog(
    state: &BerylState,
    store: &HomeStore,
    window_id: WindowId,
    scan: crate::CatalogCurrentScan,
    cancellation: &CommandCancellation,
) -> Result<CatalogClassification, WindowAcquisitionAuditError> {
    let limits = CursorReadLimits::new(CATALOG_AUDIT_PAGE_ITEMS, CATALOG_AUDIT_PAGE_BYTES)
        .expect("catalog acquisition audit limits are nonzero");
    let mut after: Option<CatalogRecencyCursor> = None;
    let mut matched = None;
    let mut duplicate = false;
    let mut page_ordinal = 0_usize;
    loop {
        if cancellation.is_cancelled() {
            return Err(WindowAcquisitionAuditError::Cancelled);
        }
        let page = match state.catalog().current_page(store, scan, after, limits) {
            Ok(page) => page,
            Err(CatalogReadError::RevisionChanged { .. }) => {
                return Err(WindowAcquisitionAuditError::ConcurrentPublication);
            }
            Err(CatalogReadError::StaleRow { thread_id }) => {
                return Err(WindowAcquisitionAuditError::RepairNeeded { thread_id });
            }
            Err(CatalogReadError::Invariant(_)) => {
                return Ok(CatalogClassification::Collision);
            }
            Err(error) => return Err(WindowAcquisitionAuditError::Catalog(error)),
        };
        for row in page.rows().iter().map(|current| current.row()) {
            if row.facts().claim().window_id() != Some(window_id) {
                continue;
            }
            if matched.is_some() {
                duplicate = true;
            } else {
                matched = Some(row.clone());
            }
        }
        page_ordinal += 1;
        observe_catalog_page(window_id, page_ordinal, usize::from(matched.is_some()));
        if !page.has_more() {
            break;
        }
        if cancellation.is_cancelled() {
            return Err(WindowAcquisitionAuditError::Cancelled);
        }
        let Some(next) = page.next_after() else {
            return Ok(CatalogClassification::Collision);
        };
        after = Some(next);
    }
    Ok(if duplicate {
        CatalogClassification::Collision
    } else if let Some(row) = matched {
        CatalogClassification::Exact(row)
    } else {
        CatalogClassification::Missing
    })
}

fn observe_catalog_page(window_id: WindowId, page_ordinal: usize, retained_rows: usize) {
    #[cfg(feature = "test-faults")]
    {
        let hook = CATALOG_AUDIT_PAGE_HOOK
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(hook) = hook {
            hook(window_id, page_ordinal);
        }
    }
    #[cfg(test)]
    observe_catalog_page_for_test(retained_rows);
    #[cfg(not(test))]
    let _ = retained_rows;
    #[cfg(not(feature = "test-faults"))]
    let _ = (window_id, page_ordinal);
}

#[cfg(test)]
thread_local! {
    static CATALOG_SCAN_TEST_STATE: std::cell::RefCell<CatalogScanTestState> =
        std::cell::RefCell::new(CatalogScanTestState::default());
}

#[cfg(test)]
#[derive(Default)]
struct CatalogScanTestState {
    max_retained_rows: usize,
    page_hook: Option<Box<dyn FnMut()>>,
}

#[cfg(test)]
fn observe_catalog_page_for_test(retained_rows: usize) {
    CATALOG_SCAN_TEST_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.max_retained_rows = state.max_retained_rows.max(retained_rows);
        if let Some(hook) = &mut state.page_hook {
            hook();
        }
    });
}

#[cfg(test)]
fn reset_catalog_scan_test_state() {
    CATALOG_SCAN_TEST_STATE.with(|state| *state.borrow_mut() = CatalogScanTestState::default());
}

#[cfg(test)]
fn catalog_scan_max_retained_rows_for_test() -> usize {
    CATALOG_SCAN_TEST_STATE.with(|state| state.borrow().max_retained_rows)
}

#[cfg(test)]
fn set_catalog_scan_page_hook_for_test(hook: impl FnMut() + 'static) {
    CATALOG_SCAN_TEST_STATE.with(|state| state.borrow_mut().page_hook = Some(Box::new(hook)));
}

fn combine(
    session: SessionClassification,
    catalog: CatalogClassification,
) -> WindowAcquisitionNaturalState {
    let (session, row) = match (session, catalog) {
        (SessionClassification::Missing, CatalogClassification::Missing) => {
            return WindowAcquisitionNaturalState::Missing;
        }
        (SessionClassification::Exact(session), CatalogClassification::Exact(row)) => {
            (session, row)
        }
        _ => return WindowAcquisitionNaturalState::Collision,
    };
    let claim = row.facts().claim();
    if row.thread_id() != session.thread_id
        || claim.window_id() != Some(session.window_id)
        || claim.kind() != Some(CatalogClaimKind::Active)
        || row.sources().claim() != Some(session.claim_revision)
    {
        return WindowAcquisitionNaturalState::Collision;
    }
    let origin = if row.revision() == CatalogRevision::INITIAL {
        WindowAcquisitionThreadOrigin::CreatedFallback
    } else {
        WindowAcquisitionThreadOrigin::Reused
    };
    WindowAcquisitionNaturalState::Committed(WindowAcquisitionCommittedFacts {
        window_id: session.window_id,
        thread_id: session.thread_id,
        target: session.target,
        placement: session.placement,
        session_revision: session.session_revision,
        claim_generation: session.claim_generation,
        claim_revision: session.claim_revision,
        fallback_target: session.fallback_target,
        origin,
    })
}

fn map_catalog_start(error: CatalogReadError) -> WindowAcquisitionAuditError {
    match error {
        CatalogReadError::RevisionChanged { .. } => {
            WindowAcquisitionAuditError::ConcurrentPublication
        }
        error => WindowAcquisitionAuditError::Catalog(error),
    }
}

#[cfg(test)]
#[path = "window_acquisition/tests.rs"]
mod tests;
