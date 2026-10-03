use beryl_home_store::HomeRecoveryCandidate;
use beryl_model::{BerylHomeId, WindowId, WindowPlacement};
use beryl_state::{
    MinimalSessionBootstrap, RecordRevision, SessionExitIntent, SessionState, ThreadClaimState,
    WindowClaimSelection,
};

#[derive(Debug)]
pub(crate) struct RunningWindowFacts {
    pub(crate) window: WindowId,
    pub(crate) revision: RecordRevision,
    pub(crate) placement: WindowPlacement,
    pub(crate) selection: Option<WindowClaimSelection>,
}

#[derive(Debug)]
pub(crate) struct UnchangedRunning {
    home: BerylHomeId,
    path: std::path::PathBuf,
    members: Vec<RunningWindowFacts>,
    pinned: Option<MinimalSessionBootstrap>,
}

impl UnchangedRunning {
    pub(crate) fn new(
        home: BerylHomeId,
        path: std::path::PathBuf,
        members: Vec<RunningWindowFacts>,
    ) -> Self {
        Self {
            home,
            path,
            members,
            pinned: None,
        }
    }

    fn read(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<MinimalSessionBootstrap, String> {
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        if access.home_id() != self.home
            || access.canonical_path() != self.path
            || self.members.is_empty()
            || self.members.len() > beryl_state::MAX_RESTORABLE_WINDOWS
        {
            return Err("unchanged Running capture belongs to another home or window set".into());
        }
        let revision = access.home_revision().map_err(|e| e.to_string())?;
        let snapshot = session
            .minimal_bootstrap_candidate(&access)
            .map_err(|e| e.to_string())?
            .ok_or("captured Running session is missing")?;
        if snapshot.header().exit_intent() != SessionExitIntent::Running
            || snapshot.windows().len() != self.members.len()
        {
            return Err("captured Running membership changed".into());
        }
        for captured in &self.members {
            let window = snapshot
                .windows()
                .iter()
                .find(|w| w.window_id() == captured.window)
                .ok_or("captured Running window is missing")?;
            if window.revision() != captured.revision
                || window.placement() != &captured.placement
                || window.selected_thread() != captured.selection
            {
                return Err(
                    "captured Running window revision, placement or selection changed".into(),
                );
            }
            let source = session
                .window_claim_catalog_source_candidate(&access, captured.window)
                .map_err(|e| e.to_string())?;
            let exact = match (source.claim(), captured.selection) {
                (None, None) => self.members.len() == 1 && snapshot.header().fallback().is_none(),
                (Some(claim), Some(selection)) => {
                    claim.thread_id() == selection.thread_id()
                        && claim.generation() == selection.generation()
                        && claim.revision() == selection.revision()
                        && claim.state() == ThreadClaimState::Active
                }
                _ => false,
            };
            if !exact {
                return Err("captured Running paired Active claim changed".into());
            }
        }
        if self
            .pinned
            .as_ref()
            .is_some_and(|pinned| pinned != &snapshot)
        {
            return Err("pinned unchanged Running session changed".into());
        }
        if access.home_revision().map_err(|e| e.to_string())? != revision {
            return Err("unchanged Running candidate source changed during validation".into());
        }
        Ok(snapshot)
    }

    pub(crate) fn converge(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<(), String> {
        let snapshot = self.read(candidate, session)?;
        self.pinned = Some(snapshot);
        Ok(())
    }

    pub(crate) fn revalidate(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<(), String> {
        if self.pinned.is_none() {
            return Err("unchanged Running session has not been authenticated".into());
        }
        self.read(candidate, session).map(|_| ())
    }
}
