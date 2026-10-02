use beryl_home_store::HomeRecoveryCandidate;
use beryl_model::{BerylHomeId, WindowId};
use beryl_state::{SessionExitIntent, SessionState, ThreadClaimState, WindowClaimSelection};

#[derive(Debug)]
pub(crate) struct UnremovedWindows {
    home: BerylHomeId,
    path: std::path::PathBuf,
    members: Vec<(WindowId, Option<WindowClaimSelection>)>,
}

impl UnremovedWindows {
    pub(crate) fn new(
        home: BerylHomeId,
        path: std::path::PathBuf,
        members: Vec<(WindowId, Option<WindowClaimSelection>)>,
    ) -> Self {
        Self {
            home,
            path,
            members,
        }
    }

    pub(crate) fn validate(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<(), String> {
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        if access.home_id() != self.home
            || access.canonical_path() != self.path
            || self.members.is_empty()
            || self.members.len() > beryl_state::MAX_RESTORABLE_WINDOWS
        {
            return Err(
                "unremoved live window recovery belongs to another home or window set".into(),
            );
        }
        let revision = access.home_revision().map_err(|e| e.to_string())?;
        let snapshot = session
            .minimal_bootstrap_candidate(&access)
            .map_err(|e| e.to_string())?
            .ok_or("live session is missing")?;
        if snapshot.header().exit_intent() != SessionExitIntent::Running
            || snapshot.windows().len() != self.members.len()
        {
            return Err("live membership changed before recovery".into());
        }
        for (id, selection) in &self.members {
            let window = snapshot
                .windows()
                .iter()
                .find(|window| window.window_id() == *id)
                .ok_or("live member is missing")?;
            if window.selected_thread() != *selection {
                return Err("live selection changed before recovery".into());
            }
            let claim = session
                .window_claim_catalog_source_candidate(&access, *id)
                .map_err(|e| e.to_string())?;
            let matches = match (claim.claim(), selection) {
                (None, None) => true,
                (Some(claim), Some(selection)) => {
                    claim.thread_id() == selection.thread_id()
                        && claim.generation() == selection.generation()
                        && claim.revision() == selection.revision()
                        && claim.state() == ThreadClaimState::Active
                }
                _ => false,
            };
            if !matches {
                return Err("live paired claim changed before recovery".into());
            }
        }
        if access.home_revision().map_err(|e| e.to_string())? != revision {
            return Err("live window recovery source changed".into());
        }
        Ok(())
    }
}
