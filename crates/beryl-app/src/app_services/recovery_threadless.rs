use beryl_home_store::{HomeCandidateRecoveryAccess, HomeGeneration};
use beryl_model::{BerylHomeId, WindowId};
use beryl_state::{BerylState, SessionWindowRecord};

pub(crate) struct ThreadlessRecoveryWindow {
    home: BerylHomeId,
    generation: HomeGeneration,
    window: SessionWindowRecord,
}

impl ThreadlessRecoveryWindow {
    pub(crate) fn prepare(
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
        retired_home: BerylHomeId,
        retired_generation: HomeGeneration,
        window: WindowId,
    ) -> Result<Self, String> {
        if access.home_id() != retired_home || access.generation() == retired_generation {
            return Err(
                "threadless recovery requires the replacement generation of the same home".into(),
            );
        }
        Ok(Self {
            home: access.home_id(),
            generation: access.generation(),
            window: read_window(access, state, window)?,
        })
    }

    pub(crate) fn revalidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> Result<(), String> {
        if access.home_id() != self.home || access.generation() != self.generation {
            return Err("threadless recovery candidate identity changed".into());
        }
        if read_window(access, state, self.window.window_id())? != self.window {
            return Err("threadless recovery window facts changed".into());
        }
        Ok(())
    }

    pub(crate) fn home_id(&self) -> BerylHomeId {
        self.home
    }

    pub(crate) fn generation(&self) -> HomeGeneration {
        self.generation
    }

    pub(crate) fn window(&self) -> &SessionWindowRecord {
        &self.window
    }
}

fn read_window(
    access: &HomeCandidateRecoveryAccess<'_>,
    state: &BerylState,
    window_id: WindowId,
) -> Result<SessionWindowRecord, String> {
    let before = access.home_revision().map_err(|error| error.to_string())?;
    let session = state.session();
    let snapshot = session
        .minimal_bootstrap_candidate(access)
        .map_err(|error| error.to_string())?
        .ok_or("threadless recovery session is missing")?;
    if snapshot.windows().len() != 1 || snapshot.header().fallback().is_some() {
        return Err("threadless recovery requires a sole initial member".into());
    }
    let window = &snapshot.windows()[0];
    if window.window_id() != window_id
        || window.selected_thread().is_some()
        || window.remembered_target().is_some()
    {
        return Err("threadless recovery window identity or selection is invalid".into());
    }
    if session
        .window_claim_catalog_source_candidate(access, window_id)
        .map_err(|error| error.to_string())?
        .claim()
        .is_some()
    {
        return Err("threadless recovery window retains a claim".into());
    }
    if state
        .runtime_roots()
        .has_runtimes_candidate(access)
        .map_err(|error| error.to_string())?
    {
        return Err("threadless recovery requires an empty runtime registry".into());
    }
    if access.home_revision().map_err(|error| error.to_string())? != before {
        return Err("threadless recovery source changed during validation".into());
    }
    Ok(window.clone())
}

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../tests/unit/app_services/recovery_threadless.rs"]
mod tests;
