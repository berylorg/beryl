use std::sync::{
    Arc, Mutex, TryLockError, Weak,
    atomic::{AtomicBool, Ordering},
};

use beryl_home_store::{
    CommandError, CommandOutcome, HomeServiceReference, ReconciliationFailure,
    ReconciliationHandle, ReconciliationResolution,
};
use beryl_model::SyndicThreadId;
use syndic_storage::{
    AdvanceTranscriptBuild, ProjectionLifecycle, StartTranscriptBuild, SyndicMutationError,
    SyndicPointReadLimit, SyndicReadError, SyndicStorage, ThreadRecord, TranscriptBuildPhase,
    TranscriptBuildRecord, TranscriptViewHeadRecord,
};

use super::ProjectionConnectionService;

const COMMAND_BUDGET: usize = 8;

#[derive(Clone)]
pub(crate) struct SelectedTranscriptConstructionAuthority {
    home: Weak<HomeServiceReference>,
    storage: SyndicStorage,
    workers: super::ProjectionWorkerPool,
    authorizer: super::LiveCommandAuthorizer,
    retained: Arc<Mutex<RetainedBatch>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectedTranscriptConstructionProgress {
    Current,
    Pending,
    PeerProgress,
    WaitingForProjection,
}

#[derive(Clone, Debug, thiserror::Error)]
pub(crate) enum SelectedTranscriptConstructionError {
    #[error("selected transcript construction was cancelled")]
    Cancelled,
    #[error("selected transcript construction has no worker capacity")]
    Busy,
    #[error("selected transcript construction admission failed: {0}")]
    Admission(Arc<crate::cas_projection::LiveCommandAdmissionError>),
    #[error("selected transcript construction is unavailable")]
    Unavailable,
    #[error("selected transcript construction read failed: {0}")]
    Read(Arc<SyndicReadError>),
    #[error("selected transcript construction authority disagrees: {0}")]
    Invariant(&'static str),
    #[error("selected transcript construction command was not committed")]
    CommandNotCommitted,
    #[error("selected transcript construction committed with a later failure")]
    CommandCommittedFailure,
    #[error("selected transcript construction reconciliation is unresolved")]
    ReconciliationFailed,
    #[error("selected transcript construction reconciliation collided")]
    Collision,
}

#[derive(Default)]
struct RetainedBatch {
    outcomes: Vec<RetainedSelectedTranscriptOutcome>,
    terminal: Option<SelectedTranscriptConstructionError>,
}

pub(crate) enum RetainedSelectedTranscriptOutcome {
    Command(CommandOutcome),
    Reconciled {
        failure: CommandError,
        handle: ReconciliationHandle,
        resolution: Result<ReconciliationResolution, ReconciliationFailure>,
    },
}

impl std::fmt::Debug for RetainedSelectedTranscriptOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Command(outcome) => formatter.debug_tuple("Command").field(outcome).finish(),
            Self::Reconciled {
                failure,
                handle,
                resolution,
            } => formatter
                .debug_struct("Reconciled")
                .field("failure", failure)
                .field("handle", handle)
                .field("resolution", resolution)
                .finish(),
        }
    }
}

#[derive(Clone, PartialEq)]
struct Snapshot {
    thread: ThreadRecord,
    head: TranscriptViewHeadRecord,
    build: Option<TranscriptBuildRecord>,
}

impl ProjectionConnectionService {
    pub(crate) fn selected_transcript_construction_authority(
        &self,
    ) -> SelectedTranscriptConstructionAuthority {
        SelectedTranscriptConstructionAuthority {
            home: self.home.as_ref().map_or_else(Weak::new, Arc::downgrade),
            storage: self.storage.clone(),
            workers: self.workers.clone(),
            authorizer: self.command_authorizer.clone(),
            retained: Arc::new(Mutex::new(RetainedBatch::default())),
        }
    }
}

impl SelectedTranscriptConstructionAuthority {
    #[cfg(test)]
    pub(crate) fn step_after_observation(
        &self,
        thread: SyndicThreadId,
        cancelled: &AtomicBool,
        observed: impl FnOnce(),
    ) -> Result<SelectedTranscriptConstructionProgress, SelectedTranscriptConstructionError> {
        self.step_guarded(thread, cancelled, || true, observed)
    }

    #[cfg(test)]
    pub(crate) fn inspect_retained<T>(
        &self,
        inspect: impl FnOnce(
            &[RetainedSelectedTranscriptOutcome],
            Option<&SelectedTranscriptConstructionError>,
        ) -> T,
    ) -> T {
        let retained = self
            .retained
            .lock()
            .expect("construction custody mutex is healthy");
        inspect(&retained.outcomes, retained.terminal.as_ref())
    }

    pub(crate) fn step(
        &self,
        thread: SyndicThreadId,
        cancelled: &AtomicBool,
    ) -> Result<SelectedTranscriptConstructionProgress, SelectedTranscriptConstructionError> {
        self.step_guarded(thread, cancelled, || true, || ())
    }

    pub(crate) fn step_guarded(
        &self,
        thread: SyndicThreadId,
        cancelled: &AtomicBool,
        current: impl Fn() -> bool,
        observed: impl FnOnce(),
    ) -> Result<SelectedTranscriptConstructionProgress, SelectedTranscriptConstructionError> {
        let mut retained = match self.retained.try_lock() {
            Ok(retained) => retained,
            Err(TryLockError::WouldBlock) => return Err(SelectedTranscriptConstructionError::Busy),
            Err(TryLockError::Poisoned(_)) => {
                return Err(SelectedTranscriptConstructionError::Unavailable);
            }
        };
        if let Some(error) = &retained.terminal {
            return Err(error.clone());
        }
        check_live(cancelled, &current)?;
        let _admitted = match self.authorizer.authorize() {
            Ok(admitted) => admitted,
            Err(error) => {
                return close(
                    &mut retained,
                    SelectedTranscriptConstructionError::Admission(Arc::new(error)),
                );
            }
        };
        let Some(home) = self.home.upgrade() else {
            return close(
                &mut retained,
                SelectedTranscriptConstructionError::Unavailable,
            );
        };
        let _worker = match self.workers.try_acquire_ordinary() {
            Ok(worker) => worker,
            Err(super::ProjectionWorkerPermitError::CapacityFull { .. }) => {
                return Err(SelectedTranscriptConstructionError::Busy);
            }
            Err(super::ProjectionWorkerPermitError::Poisoned) => {
                return close(
                    &mut retained,
                    SelectedTranscriptConstructionError::Unavailable,
                );
            }
        };
        retained.outcomes.clear();
        let mut observed = Some(observed);
        for _ in 0..COMMAND_BUDGET {
            check_live(cancelled, &current)?;
            let before = match self.observe(&home, thread) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return Ok(SelectedTranscriptConstructionProgress::PeerProgress),
                Err(error) => return close(&mut retained, error),
            };
            match validate(&before) {
                Ok(true) => return Ok(SelectedTranscriptConstructionProgress::Current),
                Ok(false) => (),
                Err(error) => return close(&mut retained, error),
            }
            if let Some(observed) = observed.take() {
                observed();
            }
            let command = match before.build.as_ref() {
                Some(build) => {
                    self.storage
                        .current_advance_transcript_build(AdvanceTranscriptBuild::new(
                            thread,
                            before.head.generation(),
                            build.revision(),
                        ))
                }
                None => self
                    .storage
                    .current_start_transcript_build(StartTranscriptBuild::new(
                        thread,
                        before.thread.revision(),
                        before.head.revision(),
                    )),
            };
            check_live(cancelled, &current)?;
            let outcome = home.execute_current(command);
            match outcome {
                CommandOutcome::Indeterminate {
                    failure,
                    reconciliation,
                } => {
                    let handle = reconciliation.install_and_handle();
                    let resolution = home.reconcile(&handle);
                    let error = match &resolution {
                        Ok(ReconciliationResolution::Collision) => {
                            Some(SelectedTranscriptConstructionError::Collision)
                        }
                        Err(_) => Some(SelectedTranscriptConstructionError::ReconciliationFailed),
                        _ => None,
                    };
                    retained
                        .outcomes
                        .push(RetainedSelectedTranscriptOutcome::Reconciled {
                            failure,
                            handle,
                            resolution,
                        });
                    return match error {
                        Some(error) => close(&mut retained, error),
                        None => Ok(SelectedTranscriptConstructionProgress::Pending),
                    };
                }
                outcome => {
                    let refused = matches!(outcome, CommandOutcome::NotCommitted { .. });
                    let conflict = matches!(&outcome,
                        CommandOutcome::NotCommitted { evidence }
                        if selected_transcript_refusal_is_conflict(evidence));
                    let failed_after_commit = matches!(
                        outcome,
                        CommandOutcome::Committed {
                            later_failure: Some(_),
                            ..
                        }
                    );
                    let projection_unavailable = matches!(&outcome,
                        CommandOutcome::NotCommitted { evidence: CommandError::ContributorValidation { source, .. } }
                        if matches!(source.downcast_ref::<SyndicMutationError>(),
                            Some(SyndicMutationError::TranscriptProjectionUnavailable)));
                    retained
                        .outcomes
                        .push(RetainedSelectedTranscriptOutcome::Command(outcome));
                    if failed_after_commit {
                        return close(
                            &mut retained,
                            SelectedTranscriptConstructionError::CommandCommittedFailure,
                        );
                    }
                    if refused {
                        if projection_unavailable {
                            return Ok(
                                SelectedTranscriptConstructionProgress::WaitingForProjection,
                            );
                        }
                        if !conflict {
                            return close(
                                &mut retained,
                                SelectedTranscriptConstructionError::CommandNotCommitted,
                            );
                        }
                        match self.observe(&home, thread) {
                            Ok(None) => {
                                return Ok(SelectedTranscriptConstructionProgress::PeerProgress);
                            }
                            Ok(Some(after)) => {
                                if let Err(error) = validate(&after) {
                                    return close(&mut retained, error);
                                }
                                if relevant_progress(&before, &after) {
                                    return Ok(
                                        SelectedTranscriptConstructionProgress::PeerProgress,
                                    );
                                }
                            }
                            Err(error) => return close(&mut retained, error),
                        }
                        return close(
                            &mut retained,
                            SelectedTranscriptConstructionError::CommandNotCommitted,
                        );
                    }
                }
            }
        }
        Ok(SelectedTranscriptConstructionProgress::Pending)
    }

    fn observe(
        &self,
        home: &HomeServiceReference,
        thread: SyndicThreadId,
    ) -> Result<Option<Snapshot>, SelectedTranscriptConstructionError> {
        let limit = SyndicPointReadLimit::new(65_536).expect("fixed nonzero point-read limit");
        let read = || {
            let thread_record = self.storage.thread(home, thread, limit)?;
            let head = self.storage.transcript_view_head(home, thread, limit)?;
            let build = match head.as_ref() {
                Some(head) => {
                    self.storage
                        .transcript_build(home, thread, head.generation(), limit)?
                }
                None => None,
            };
            Ok::<_, SyndicReadError>((thread_record, head, build))
        };
        let first =
            read().map_err(|error| SelectedTranscriptConstructionError::Read(Arc::new(error)))?;
        let second =
            read().map_err(|error| SelectedTranscriptConstructionError::Read(Arc::new(error)))?;
        if first != second {
            return Ok(None);
        }
        let (thread_record, head, build) = first;
        let thread_record = thread_record.ok_or(SelectedTranscriptConstructionError::Invariant(
            "thread is missing",
        ))?;
        let head = head.ok_or(SelectedTranscriptConstructionError::Invariant(
            "head is missing",
        ))?;
        if thread_record.id() != thread || head.thread_id() != thread {
            return Err(SelectedTranscriptConstructionError::Invariant(
                "thread identity differs",
            ));
        }
        Ok(Some(Snapshot {
            thread: thread_record,
            head,
            build,
        }))
    }
}

fn check_live(
    cancelled: &AtomicBool,
    current: &impl Fn() -> bool,
) -> Result<(), SelectedTranscriptConstructionError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(SelectedTranscriptConstructionError::Cancelled);
    }
    if !current() {
        return Err(SelectedTranscriptConstructionError::Unavailable);
    }
    Ok(())
}

fn close<T>(
    retained: &mut RetainedBatch,
    error: SelectedTranscriptConstructionError,
) -> Result<T, SelectedTranscriptConstructionError> {
    retained.terminal = Some(error.clone());
    Err(error)
}

fn relevant_progress(before: &Snapshot, after: &Snapshot) -> bool {
    before.head != after.head
        || before.build != after.build
        || before.thread.committed_tail() != after.thread.committed_tail()
        || before.thread.selected_path_digest() != after.thread.selected_path_digest()
}

pub(in crate::cas_projection) fn selected_transcript_refusal_is_conflict(
    evidence: &CommandError,
) -> bool {
    matches!(evidence,
        CommandError::ContributorValidation { source, .. }
        if matches!(source.downcast_ref::<SyndicMutationError>(),
            Some(SyndicMutationError::TranscriptBuildConflict
                | SyndicMutationError::TranscriptAlreadyCurrent)))
}

fn validate(snapshot: &Snapshot) -> Result<bool, SelectedTranscriptConstructionError> {
    let fail = || SelectedTranscriptConstructionError::Invariant("selected head/build disagrees");
    let thread = &snapshot.thread;
    let head = &snapshot.head;
    if head.committed_tail() != thread.committed_tail()
        || head.selected_path_digest() != thread.selected_path_digest()
    {
        return Err(fail());
    }
    let current = head.lifecycle() == ProjectionLifecycle::Current;
    if !current && head.entry_count() != 0 {
        return Err(fail());
    }
    match snapshot.build.as_ref() {
        Some(build) => {
            if build.thread_id() != thread.id()
                || build.generation() != head.generation()
                || build.revision() != head.revision()
                || build.source_thread_revision() > thread.revision()
                || build.committed_tail() != thread.committed_tail()
                || build.selected_path_digest() != thread.selected_path_digest()
                || (current
                    && (build.phase() != TranscriptBuildPhase::Complete
                        || build.entry_count() != head.entry_count()))
                || (!current
                    && !matches!(
                        build.phase(),
                        TranscriptBuildPhase::Collecting { .. }
                            | TranscriptBuildPhase::Publishing { .. }
                    ))
            {
                return Err(fail());
            }
        }
        None if current && (thread.committed_tail().is_some() || head.entry_count() != 0) => {
            return Err(fail());
        }
        None => (),
    }
    Ok(current)
}
