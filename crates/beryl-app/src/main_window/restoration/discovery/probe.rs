use beryl_home_store::{CursorReadLimits, HomeServiceReference};
use beryl_model::{DomainRevision, HomeRevision};
use beryl_state::{
    BerylState, MinimalSessionBootstrap, RememberedTarget, RuntimeRootState, ThreadClaimRecord,
};

pub(super) struct DiscoverySnapshot {
    pub(super) home_revision: HomeRevision,
    pub(super) domain_revision: DomainRevision,
    pub(super) session: Option<MinimalSessionBootstrap>,
    pub(super) claims: Vec<Option<ThreadClaimRecord>>,
    pub(super) no_runtimes: bool,
}

impl DiscoverySnapshot {
    pub(super) fn read(store: &HomeServiceReference, state: &BerylState) -> Result<Self, String> {
        let home_revision = store.home_revision().map_err(|e| e.to_string())?;
        let sessions = state.session();
        let domain_revision = sessions.revision(store).map_err(|e| e.to_string())?;
        let session = sessions
            .minimal_bootstrap(store)
            .map_err(|e| e.to_string())?;
        let roots = state.runtime_roots();
        let runtimes = roots
            .list_runtimes(store, None, CursorReadLimits::new(1, 256 * 1024).unwrap())
            .map_err(|e| e.to_string())?;
        let no_runtimes = runtimes.records().is_empty() && !runtimes.has_more();
        drop(runtimes);
        let mut claims = Vec::new();
        if let Some(snapshot) = &session {
            if let Some(target) = snapshot.header().fallback() {
                validate_target(store, &roots, target)?;
                if no_runtimes {
                    return Err("empty runtime registry retains a session fallback".to_owned());
                }
            } else if !no_runtimes {
                return Err("runtime registry has no session fallback".to_owned());
            }
            for window in snapshot.windows() {
                let claim = sessions
                    .window_claim_catalog_source(store, window.window_id())
                    .map_err(|e| e.to_string())?
                    .claim();
                match (window.selected_thread(), window.remembered_target(), claim) {
                    (None, None, None) if no_runtimes && snapshot.windows().len() == 1 => {}
                    (Some(selection), Some(target), Some(claim)) if !no_runtimes => {
                        if claim.window_id() != window.window_id()
                            || claim.thread_id() != selection.thread_id()
                            || claim.generation() != selection.generation()
                            || claim.revision() != selection.revision()
                            || claim.generation() > snapshot.header().revision()
                        {
                            return Err("restore window and paired claim disagree".to_owned());
                        }
                        validate_target(store, &roots, target)?;
                    }
                    _ => return Err("restore member is neither an exact claimed window nor a sole threadless window".to_owned()),
                }
                claims.push(claim);
            }
        } else if !no_runtimes {
            return Err("runtime registry has no session fallback".to_owned());
        }
        if store.home_revision().map_err(|e| e.to_string())? != home_revision {
            return Err("restore sources changed during discovery".to_owned());
        }
        Ok(Self {
            home_revision,
            domain_revision,
            session,
            claims,
            no_runtimes,
        })
    }
}

fn validate_target(
    store: &HomeServiceReference,
    roots: &RuntimeRootState,
    target: RememberedTarget,
) -> Result<(), String> {
    let runtime = roots
        .runtime(store, target.runtime_id())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "remembered runtime is missing".to_owned())?;
    let root = roots
        .root(store, target.root_id())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "remembered root is missing".to_owned())?;
    if runtime.runtime_id() != target.runtime_id()
        || root.root_id() != target.root_id()
        || root.runtime_id() != target.runtime_id()
    {
        return Err("remembered runtime and root identity disagree".to_owned());
    }
    Ok(())
}
