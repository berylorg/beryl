use super::*;
use crate::app_services::{
    PublishedSameWindowThreadCurrent, PublishedSameWindowThreadOperation,
    PublishedSameWindowThreadPreparation, PublishedSameWindowThreadPreparationFailure,
};
use crate::composer_host::ComposerHostFlushAdvance as SaveAdvance;
use crate::same_window_thread_acquisition::{SameWindowThreadOutcome, SameWindowThreadRequest};

mod admission;
mod presentation;

pub(super) struct ThreadCreationSource {
    request: Option<SameWindowThreadRequest>,
    pub(super) operation: Option<PublishedSameWindowThreadOperation>,
    current: Option<PublishedSameWindowThreadCurrent>,
    failure: Option<PublishedSameWindowThreadPreparationFailure>,
    advance: Option<(SaveAdvance, Selection)>,
    fenced: bool,
}

impl ThreadCreationSource {
    pub(super) fn retire_failed_home(
        &mut self,
        reader: &PublishedRunningThreadsReader,
        lease: Arc<crate::window_acquisition::WindowSelectionLease>,
    ) -> Result<crate::app_services::RetiredSameWindowThreadOperation, String> {
        if let Some(operation) = self.operation.take() {
            return operation
                .retire_failed_home()
                .map_err(|(operation, error)| {
                    self.operation = Some(operation);
                    error
                });
        }
        if let Some(current) = self.current.take() {
            return current.retire_failed_home().map_err(|(current, error)| {
                self.current = Some(current);
                error
            });
        }
        if let Some(failure) = self.failure.take() {
            return failure.retire_failed_home().map_err(|(failure, error)| {
                self.failure = Some(failure);
                error
            });
        }
        let home = reader.failed_retirement_home();
        let generation = home
            .health()
            .generation()
            .ok_or("original failed generation is missing")?;
        crate::app_services::RetiredSameWindowThreadOperation::before_preparation(
            home, generation, lease,
        )
    }

    pub(super) fn irreversible(&self) -> bool {
        self.operation.as_ref().is_some_and(|operation| {
            matches!(
                operation.outcome(),
                Some(
                    SameWindowThreadOutcome::Settled(_)
                        | SameWindowThreadOutcome::Pending(_)
                        | SameWindowThreadOutcome::Unavailable(_)
                )
            )
        })
    }

    pub(super) fn has_save_custody(&self) -> bool {
        self.operation.is_some() || self.current.is_some() || self.failure.is_some()
    }
}

impl ActivationSource {
    pub(super) fn committed_window(&self) -> Option<&beryl_state::SessionWindowRecord> {
        if let Some(commit) = &self.committed {
            return Some(&commit.window);
        }
        match self.creation.as_ref()?.operation.as_ref()?.outcome()? {
            SameWindowThreadOutcome::Settled(commit) => Some(&commit.window),
            _ => None,
        }
    }

    pub(super) fn committed_selection(&self) -> Option<beryl_state::WindowClaimSelection> {
        if let Some(commit) = &self.committed {
            return Some(commit.selection);
        }
        match self.creation.as_ref()?.operation.as_ref()?.outcome()? {
            SameWindowThreadOutcome::Settled(commit) => Some(commit.selection),
            _ => None,
        }
    }

    fn accept_thread_outcome(&mut self) -> Result<(), String> {
        match self
            .creation
            .as_ref()
            .unwrap()
            .operation
            .as_ref()
            .unwrap()
            .outcome()
        {
            Some(SameWindowThreadOutcome::Settled(commit)) => {
                self.target = commit.selection;
                self.request.thread_id = commit.selection.thread_id();
                self.error = None;
                self.stage = Stage::Begin;
            }
            Some(SameWindowThreadOutcome::Pending(_)) => {
                self.error =
                    Some("Thread confirmation is reconciling its original request.".into());
                self.stage = Stage::ThreadReconcile;
            }
            Some(SameWindowThreadOutcome::Unavailable(_)) => {
                self.terminal_release_failure = true;
                return Err("Thread confirmation is unavailable; its original outcome and custody are retained. Use same-home recovery.".into());
            }
            Some(SameWindowThreadOutcome::NotCommitted(error)) => {
                self.error = Some(error.to_string());
                self.stage = Stage::ThreadRelease;
            }
            None => return Err("Thread confirmation original outcome is missing".into()),
        }
        Ok(())
    }

    pub(super) fn run_thread_creation_source(&mut self) -> Result<(), String> {
        match self.stage {
            Stage::ThreadSave => {
                if self.flush.is_none() {
                    if self.cancellation.is_cancelled() {
                        self.error = Some("Thread confirmation cancelled".into());
                        self.stage = Stage::RestoreAutosave;
                        return Ok(());
                    }
                    self.flush = Some(self.service.begin_thread_predecessor_save(self.expected)?);
                }
                let ticket = match self.flush.unwrap() {
                    FlushAdmission::Started { ticket, .. }
                    | FlushAdmission::Joined { ticket, .. } => ticket,
                    FlushAdmission::Satisfied(_) => {
                        return Err("Thread confirmation saved barrier is missing".into());
                    }
                };
                if matches!(
                    self.flush,
                    Some(
                        FlushAdmission::Started {
                            state: FlushState::CaptureRequired,
                            ..
                        } | FlushAdmission::Joined {
                            state: FlushState::CaptureRequired,
                            ..
                        }
                    )
                ) {
                    #[cfg(all(test, feature = "test-faults"))]
                    if let Some(hook) = self.before_save.take() {
                        hook(&self.cancellation);
                    }
                    let authority = match self.service.autosave_capture_requirement(self.expected)? {
                        crate::main_window::MainWindowComposerAutosaveCaptureRequirement::ChangedMarkers => Some(fresh_marker_authority()?),
                        _ => None,
                    };
                    let captured = self.service.capture_thread_predecessor_save(
                        self.expected,
                        ticket,
                        self.assets.clone(),
                        &self.marker_seals,
                        fresh_piece_operation_id()?,
                        authority,
                        current_timestamp()?,
                        &self.cancellation,
                    )?;
                    match captured {
                        FlushCapture::Unsatisfied(failure) => {
                            self.error = Some(format!(
                                "Thread confirmation predecessor save failed: {failure:?}"
                            ));
                            self.stage = Stage::RestoreAutosave;
                            return Ok(());
                        }
                        FlushCapture::Stale => {
                            self.terminal_release_failure = true;
                            return Err("Thread confirmation predecessor save became stale; original custody is retained".into());
                        }
                        _ => {}
                    }
                    self.flush = Some(FlushAdmission::Joined {
                        ticket,
                        state: FlushState::PublicationPending,
                    });
                }
                let advance = self
                    .service
                    .advance_thread_predecessor_save(self.expected, ticket)?;
                if let SaveAdvance::Progress(state) = advance.0 {
                    self.flush = Some(FlushAdmission::Joined { ticket, state });
                }
                self.creation.as_mut().unwrap().advance = Some(advance);
                self.stage = Stage::ThreadAcceptSave;
            }
            Stage::ThreadPrepare => {
                let ticket = match self.flush.unwrap() {
                    FlushAdmission::Started { ticket, .. }
                    | FlushAdmission::Joined { ticket, .. } => ticket,
                    _ => return Err("Thread confirmation saved barrier is missing".into()),
                };
                let saved = self
                    .service
                    .qualify_thread_predecessor_save(self.expected, ticket)?;
                let creation = self.creation.as_mut().unwrap();
                let request = creation
                    .request
                    .take()
                    .ok_or("Thread confirmation request is missing")?;
                match self.reader.prepare_thread_creation(
                    request,
                    self.lease.clone(),
                    saved,
                    self.cancellation.clone(),
                ) {
                    Ok(PublishedSameWindowThreadPreparation::Current(current)) => {
                        creation.current = Some(current);
                        self.stage = Stage::ThreadRelease;
                    }
                    Ok(PublishedSameWindowThreadPreparation::Prepared(operation)) => {
                        creation.operation = Some(operation);
                        self.stage = if self.cancellation.is_cancelled() {
                            self.error = Some("Thread confirmation cancelled".into());
                            Stage::ThreadRelease
                        } else {
                            Stage::ThreadCommit
                        };
                    }
                    Err(failure) => {
                        self.error = Some(failure.error().to_owned());
                        creation.failure = Some(failure);
                        self.stage = Stage::ThreadRelease;
                    }
                }
            }
            Stage::ThreadCommit => {
                #[cfg(all(test, feature = "test-faults"))]
                if let Some(hook) = self.before_commit.take() {
                    hook(&self.cancellation);
                }
                if self.cancellation.is_cancelled() {
                    self.error = Some("Thread confirmation cancelled".into());
                    self.stage = Stage::ThreadRelease;
                    return Ok(());
                }
                self.creation
                    .as_mut()
                    .unwrap()
                    .operation
                    .as_mut()
                    .unwrap()
                    .commit()?;
                self.accept_thread_outcome()?;
            }
            Stage::ThreadReconcile => {
                self.creation
                    .as_mut()
                    .unwrap()
                    .operation
                    .as_mut()
                    .unwrap()
                    .reconcile()?;
                self.accept_thread_outcome()?;
            }
            Stage::ThreadAdopt => {
                let receipt = self.receipt()?;
                self.creation
                    .as_mut()
                    .unwrap()
                    .operation
                    .as_mut()
                    .unwrap()
                    .adopt_predecessor_save(receipt)?;
                self.flush_settled = true;
                self.stage = Stage::DisposePrior;
            }
            Stage::ThreadRelease => {
                let creation = self.creation.as_mut().unwrap();
                if let Some(current) = creation.current.take() {
                    if let Err(failure) = current.release() {
                        let error = failure.error().to_owned();
                        creation.failure = Some(failure);
                        self.terminal_release_failure = true;
                        return Err(error);
                    }
                }
                if let Some(failure) = creation.failure.take()
                    && let Err(failure) = failure.release()
                {
                    let error = failure.error().to_owned();
                    creation.failure = Some(failure);
                    self.terminal_release_failure = true;
                    return Err(error);
                }
                if let Some(operation) = creation.operation.take()
                    && let Err((operation, error)) = operation.release_noncommit()
                {
                    creation.operation = Some(operation);
                    self.terminal_release_failure = true;
                    return Err(error);
                }
                self.stage = Stage::RestoreAutosave;
            }
            _ => {}
        }
        Ok(())
    }
}
