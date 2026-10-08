use super::*;
use crate::catalog_projection::{
    CatalogProjectionBuildError, ThreadCatalogProjectionPreparation,
    prepare_thread_catalog_projection,
};

struct CapturedRead<'a> {
    home: &'a HomeStore,
    read: Option<FrozenHomeRead>,
}

impl Drop for CapturedRead<'_> {
    fn drop(&mut self) {
        if let Some(read) = self.read.take() {
            let _ = self.home.release_frozen_read(&read);
        }
    }
}

pub(super) fn run(
    home: Arc<HomeServiceReference>,
    syndic: SyndicStorage,
    state: BerylState,
    signal: Arc<SourceSignal>,
    cancellation: CommandCancellation,
) -> Result<(), CatalogSourceCoordinatorError> {
    let result = maintain(&home, &syndic, &state, &signal, &cancellation);
    let published = {
        let mut state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
        state.stopped = true;
        state.published.take()
    };
    if let Some(published) = published {
        let _ = home.release_frozen_read(&published);
    }
    result
}

fn maintain(
    home: &HomeStore,
    syndic: &SyndicStorage,
    state: &BerylState,
    signal: &SourceSignal,
    cancellation: &CommandCancellation,
) -> Result<(), CatalogSourceCoordinatorError> {
    let mut retained_noncommit = None;
    while signal.wait() {
        loop {
            if cancellation.is_cancelled() {
                return Ok(());
            }
            let mut capture = CapturedRead {
                home,
                read: Some(
                    home.capture_frozen_read(cancellation)
                        .map_err(CatalogSourceReadError::from)?,
                ),
            };
            let certification = match certification::certify(
                home,
                syndic,
                state,
                capture.read.as_ref().expect("owned capture"),
                cancellation,
            ) {
                Err(CatalogSourceReadError::Cancelled) => return Ok(()),
                other => other?,
            };
            match certification {
                certification::Certification::Ready { threads } => {
                    let old = {
                        let mut published = signal.state.lock().unwrap_or_else(|e| e.into_inner());
                        if published.stopped || cancellation.is_cancelled() {
                            return Ok(());
                        }
                        published.threads = threads;
                        published
                            .published
                            .replace(capture.read.take().expect("owned capture"))
                    };
                    if let Some(old) = old {
                        home.release_frozen_read(&old)
                            .map_err(CatalogSourceReadError::from)?;
                    }
                    break;
                }
                certification::Certification::Repair(thread) => {
                    let audited_revision = capture
                        .read
                        .as_ref()
                        .expect("owned capture")
                        .home_revision();
                    drop(capture);
                    if home.home_revision().map_err(CatalogSourceReadError::from)?
                        != audited_revision
                    {
                        break;
                    }
                    let prepared =
                        match prepare_thread_catalog_projection(home, syndic, state, thread) {
                            Err(CatalogProjectionBuildError::ConcurrentPreparation) => break,
                            other => other.map_err(CatalogSourceReadError::from)?,
                        };
                    if home.home_revision().map_err(CatalogSourceReadError::from)?
                        != audited_revision
                    {
                        break;
                    }
                    let command = match prepared {
                        ThreadCatalogProjectionPreparation::ThreadMissing => {
                            return Err(CatalogSourceReadError::CanonicalSourceMissing.into());
                        }
                        ThreadCatalogProjectionPreparation::ExactCurrent => continue,
                        ThreadCatalogProjectionPreparation::Publish(command) => {
                            command.with_cancellation(cancellation.clone())
                        }
                    };
                    match home.execute(command) {
                        CommandOutcome::NotCommitted { evidence }
                            if evidence.conflicts().is_some() =>
                        {
                            retained_noncommit =
                                Some(RetainedCatalogRepair::NotCommitted(evidence));
                            break;
                        }
                        CommandOutcome::NotCommitted { evidence } => {
                            return Err(CatalogSourceCoordinatorError::Repair(Box::new(
                                RetainedCatalogRepair::NotCommitted(evidence),
                            )));
                        }
                        committed @ CommandOutcome::Committed {
                            later_failure: Some(_),
                            ..
                        } => {
                            return Err(CatalogSourceCoordinatorError::Repair(Box::new(
                                RetainedCatalogRepair::Committed(committed),
                            )));
                        }
                        CommandOutcome::Committed { .. } => {
                            retained_noncommit = None;
                        }
                        CommandOutcome::Indeterminate {
                            failure,
                            reconciliation,
                        } => {
                            return Err(CatalogSourceCoordinatorError::Repair(Box::new(
                                RetainedCatalogRepair::Indeterminate {
                                    failure,
                                    reconciliation: reconciliation.install_and_handle(),
                                },
                            )));
                        }
                    }
                }
            }
        }
    }
    drop(retained_noncommit);
    Ok(())
}
