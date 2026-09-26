use super::*;
use beryl_home_store::CursorReadLimits;
use beryl_model::{BerylHomeId, DomainRevision, WindowPlacement};
use beryl_state::RuntimeRootState;

pub struct ThreadlessWindowSource {
    attempt: Arc<RestoreAttemptIdentity>,
    service: Weak<()>,
    store: Arc<HomeServiceReference>,
    session: SessionState,
    runtime_roots: RuntimeRootState,
    generation: HomeGeneration,
    session_revision: SessionRevision,
    runtime_revision: DomainRevision,
    window: SessionWindowRecord,
}

impl RestoredWindowPreparationAttempt {
    pub fn begin_threadless(
        &self,
        revision: SessionRevision,
        window_id: WindowId,
        runtime_roots: RuntimeRootState,
    ) -> Result<ThreadlessWindowSource, String> {
        self.validate_lifetime()?;
        let before = self
            .store
            .home_revision()
            .map_err(|error| error.to_string())?;
        let runtime_revision = runtime_roots
            .revision(&self.store)
            .map_err(|error| error.to_string())?;
        let window = read_threadless(
            &self.session,
            &runtime_roots,
            &self.store,
            revision,
            window_id,
        )?;
        let source = ThreadlessWindowSource {
            attempt: self.identity.clone(),
            service: self.service.clone(),
            store: self.store.clone(),
            session: self.session.clone(),
            runtime_roots,
            generation: self.generation,
            session_revision: revision,
            runtime_revision,
            window,
        };
        source.revalidate()?;
        if self
            .store
            .home_revision()
            .map_err(|error| error.to_string())?
            != before
        {
            return Err("threadless source changed during discovery".to_owned());
        }
        let mut session = self
            .identity
            .session
            .lock()
            .map_err(|_| "restore session fence is poisoned".to_owned())?;
        if session.activation.is_some()
            || session.revision.is_some_and(|current| current != revision)
        {
            return Err("restore attempt session revision is stale or unsettled".to_owned());
        }
        session.revision = Some(revision);
        Ok(source)
    }
}

impl ThreadlessWindowSource {
    #[cfg(target_os = "windows")]
    pub(in crate::main_window) fn native_validation(
        &self,
    ) -> crate::main_window::restoration::NativeMemberValidation {
        let source = Self {
            attempt: self.attempt.clone(),
            service: self.service.clone(),
            store: self.store.clone(),
            session: self.session.clone(),
            runtime_roots: self.runtime_roots.clone(),
            generation: self.generation,
            session_revision: self.session_revision,
            runtime_revision: self.runtime_revision,
            window: self.window.clone(),
        };
        Box::new(move |attempt, _| {
            if !Arc::ptr_eq(&source.attempt, &attempt.identity)
                || !Arc::ptr_eq(&source.store, &attempt.store)
            {
                return Err("threadless source belongs to another startup attempt".to_owned());
            }
            source.revalidate()
        })
    }

    pub fn window_id(&self) -> WindowId {
        self.window.window_id()
    }

    pub fn placement(&self) -> &WindowPlacement {
        self.window.placement()
    }

    pub fn home_id(&self) -> BerylHomeId {
        self.store.home_id()
    }

    pub fn home_generation(&self) -> HomeGeneration {
        self.generation
    }

    pub(crate) fn validate_lifetime(&self) -> Result<(), String> {
        if !self.attempt.live.load(Ordering::Acquire)
            || self.service.upgrade().is_none()
            || self.store.health().generation() != Some(self.generation)
        {
            return Err("threadless attempt or service generation is retired".to_owned());
        }
        Ok(())
    }

    pub fn revalidate(&self) -> Result<(), String> {
        self.validate_lifetime()?;
        let before = self
            .store
            .home_revision()
            .map_err(|error| error.to_string())?;
        if self
            .runtime_roots
            .revision(&self.store)
            .map_err(|error| error.to_string())?
            != self.runtime_revision
        {
            return Err("threadless runtime registry changed".to_owned());
        }
        let window = read_threadless(
            &self.session,
            &self.runtime_roots,
            &self.store,
            self.session_revision,
            self.window_id(),
        )?;
        if window != self.window
            || self
                .store
                .home_revision()
                .map_err(|error| error.to_string())?
                != before
        {
            return Err("threadless source changed before publication".to_owned());
        }
        self.validate_lifetime()
    }
}

fn read_threadless(
    session: &SessionState,
    runtime_roots: &RuntimeRootState,
    store: &HomeStore,
    revision: SessionRevision,
    window_id: WindowId,
) -> Result<SessionWindowRecord, String> {
    let snapshot = session
        .minimal_bootstrap(store)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "threadless session is missing".to_owned())?;
    if snapshot.header().revision() != revision
        || snapshot.windows().len() != 1
        || snapshot.header().fallback().is_some()
    {
        return Err("threadless session is stale or not a sole initial member".to_owned());
    }
    let window = &snapshot.windows()[0];
    if window.window_id() != window_id
        || window.selected_thread().is_some()
        || window.remembered_target().is_some()
    {
        return Err("threadless window identity or selection is invalid".to_owned());
    }
    if session
        .window_claim_catalog_source(store, window_id)
        .map_err(|error| error.to_string())?
        .claim()
        .is_some()
    {
        return Err("threadless window retains a claim".to_owned());
    }
    let runtimes = runtime_roots
        .list_runtimes(store, None, CursorReadLimits::new(1, 256 * 1024).unwrap())
        .map_err(|error| error.to_string())?;
    if !runtimes.records().is_empty() || runtimes.has_more() {
        return Err("threadless window requires an empty runtime registry".to_owned());
    }
    Ok(window.clone())
}
