use super::ExitSessionPreparationError;
use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::{SessionRevision, WindowId, WindowPlacement};
use beryl_state::{MinimalSessionBootstrap, RecordRevision};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExitSessionPublication {
    pub(super) configured_home: PathBuf,
    pub(super) source: MinimalSessionBootstrap,
    pub(super) result_session_revision: SessionRevision,
    pub(super) result_windows: Vec<(WindowId, RecordRevision, WindowPlacement)>,
}

pub(crate) struct PreparedExitSession {
    pub(super) command: HomeCommand,
    pub(super) publication: Box<ExitSessionPublication>,
}

impl ExitSessionPublication {
    pub(super) fn prepare(
        home: &HomeStore,
        source: MinimalSessionBootstrap,
        placements: &[(WindowId, WindowPlacement)],
    ) -> Result<Box<Self>, ExitSessionPreparationError> {
        let overflow = || ExitSessionPreparationError::Command("Exit revision overflow".into());
        let result_session_revision = source
            .header()
            .revision()
            .checked_next()
            .map_err(|_| overflow())?;
        let result_windows = source
            .windows()
            .iter()
            .zip(placements)
            .map(|(record, (id, placement))| {
                let next = record
                    .revision()
                    .get()
                    .checked_add(1)
                    .ok_or_else(overflow)?;
                Ok((
                    *id,
                    RecordRevision::new(next).map_err(|_| overflow())?,
                    placement.clone(),
                ))
            })
            .collect::<Result<Vec<_>, ExitSessionPreparationError>>()?;
        Ok(Box::new(Self {
            configured_home: home.configured_path().to_owned(),
            source,
            result_session_revision,
            result_windows,
        }))
    }
}
