use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::{WindowId, WindowPlacement};
use beryl_state::{
    ExitWindowPlacement, MAX_RESTORABLE_WINDOWS, PublishExitSession, SessionExitIntent,
    SessionState,
};

mod execution;
pub(crate) use execution::*;
mod evidence;
pub(crate) use evidence::{ExitSessionPublication, PreparedExitSession};
mod validation;
pub(crate) use validation::{ExitSessionValidation, ExitSessionValidationError};
mod resume;
pub(crate) use resume::{
    InterruptedExit, InterruptedExitResume, ResumeSessionOutcome, ResumeSessionValidation,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExitSessionPreparationError {
    #[error("Exit placements do not match the complete session window set")]
    WindowSet,
    #[error("Exit session state is missing")]
    MissingSession,
    #[error("Exit session is already marked for orderly exit")]
    NotRunning,
    #[error("Exit session read failed: {0}")]
    Read(String),
    #[error("Home revision changed during Exit session preparation")]
    Changed,
    #[error("Exit session command preparation failed: {0}")]
    Command(String),
}

pub(crate) fn prepare_exit_session_command(
    home: &HomeStore,
    session: &SessionState,
    mut placements: Vec<(WindowId, WindowPlacement)>,
) -> Result<PreparedExitSession, ExitSessionPreparationError> {
    use ExitSessionPreparationError as Error;

    if placements.is_empty() || placements.len() > MAX_RESTORABLE_WINDOWS {
        return Err(Error::WindowSet);
    }
    placements.sort_unstable_by_key(|(id, _)| *id);
    if placements.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::WindowSet);
    }
    let home_revision = home
        .home_revision()
        .map_err(|error| Error::Read(error.to_string()))?;
    let domain_revision = session
        .revision(home)
        .map_err(|error| Error::Read(error.to_string()))?;
    let snapshot = session
        .minimal_bootstrap(home)
        .map_err(|error| Error::Read(error.to_string()))?
        .ok_or(Error::MissingSession)?;
    if snapshot.header().exit_intent() != SessionExitIntent::Running {
        return Err(Error::NotRunning);
    }
    if snapshot.windows().len() != placements.len()
        || snapshot
            .windows()
            .iter()
            .zip(&placements)
            .any(|(record, (id, _))| record.window_id() != *id)
    {
        return Err(Error::WindowSet);
    }
    let publication = ExitSessionPublication::prepare(home, snapshot.clone(), &placements)?;
    let windows = snapshot
        .windows()
        .iter()
        .zip(placements)
        .map(|(record, (id, placement))| ExitWindowPlacement::new(id, record.revision(), placement))
        .collect();
    if home
        .home_revision()
        .map_err(|error| Error::Read(error.to_string()))?
        != home_revision
    {
        return Err(Error::Changed);
    }
    let mutation = PublishExitSession::new(snapshot.header().revision(), windows)
        .map_err(|error| Error::Command(error.to_string()))?;
    let mut command = HomeCommand::new(home_revision);
    command
        .add(session.publish_exit(domain_revision, mutation))
        .map_err(|error| Error::Command(error.to_string()))?;
    Ok(PreparedExitSession {
        command,
        publication,
    })
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/exit_session.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/exit_session_execution.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/exit_session_readiness.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/exit_session_validation.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/exit_session_resume.rs"
    ));
}
