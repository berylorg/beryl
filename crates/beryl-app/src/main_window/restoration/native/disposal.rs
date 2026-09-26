use super::*;

enum NativeDrain {
    Prepared(Box<MainWindowStartupShellPrepared>),
    Native(futures_channel::oneshot::Receiver<MainWindowStartupDisposalCompletion>),
}

struct WorkerRetirementResult {
    custody: Option<WorkerRetainedCustody>,
    error: Option<String>,
}

impl WorkerRetirementResult {
    fn settled() -> Self {
        Self {
            custody: None,
            error: None,
        }
    }

    fn retained(custody: WorkerRetainedCustody, error: String) -> Self {
        Self {
            custody: Some(custody),
            error: Some(error),
        }
    }
}

enum WorkerRetainedCustody {
    AcquiredUnpublished(Box<MainWindowShellUnpublished>),
    AcquiredAbandonment(Box<MainWindowShellAbandonment>),
    AcquiredReconciliation(Box<MainWindowShellAbandonmentReconciliation>),
    CommittedAbandonment {
        receipt: beryl_home_store::CommitReceipt,
        later_failure: Option<beryl_home_store::CommandError>,
        local_finalization: beryl_home_store::CommittedLocalFinalization,
    },
    AcquiredPreserved(Box<MainWindowShellRecordPreservingRetirement>),
    Restored(Box<RestoredWindowShellUnpublished>),
}

impl WorkerRetainedCustody {
    fn into_native(self) -> MainWindowNativeRetainedCustody {
        match self {
            Self::AcquiredUnpublished(custody) => {
                MainWindowNativeRetainedCustody::AcquiredUnpublished(custody)
            }
            Self::AcquiredAbandonment(custody) => {
                MainWindowNativeRetainedCustody::AcquiredAbandonment(custody)
            }
            Self::AcquiredReconciliation(custody) => {
                MainWindowNativeRetainedCustody::AcquiredReconciliation(custody)
            }
            Self::CommittedAbandonment {
                receipt,
                later_failure,
                local_finalization,
            } => MainWindowNativeRetainedCustody::CommittedAbandonment {
                receipt,
                later_failure,
                local_finalization,
            },
            Self::AcquiredPreserved(custody) => {
                MainWindowNativeRetainedCustody::AcquiredPreserved(custody)
            }
            Self::Restored(custody) => MainWindowNativeRetainedCustody::Restored(custody),
        }
    }
}

impl NativeRestoreSetFlight {
    pub(super) async fn dispose(
        mut self,
        error: String,
        cx: &mut AsyncApp,
    ) -> MainWindowNativeRestoreSetFailure {
        self.cancellation.cancel();
        let owner = self
            .owner
            .take()
            .expect("startup disposal retains its original attempt");
        let mut retained = Vec::new();
        let mut diagnostics = Vec::new();
        let drains = cx
            .update(|app| {
                let mut drains = Vec::with_capacity(self.members.len());
                for member in &mut self.members {
                    let state = std::mem::replace(&mut member.state, NativeMemberState::InFlight);
                    let drain = match state {
                        NativeMemberState::Prepared { prepared, .. }
                        | NativeMemberState::Unconstructed(prepared) => {
                            Some(NativeDrain::Prepared(prepared))
                        }
                        NativeMemberState::Hidden(shell) => {
                            let (sender, receipt) = futures_channel::oneshot::channel();
                            match shell.start_startup_disposal(
                                move |completion, _| {
                                    let _ = sender.send(completion);
                                },
                                app,
                            ) {
                                Ok(()) => Some(NativeDrain::Native(receipt)),
                                Err(failure) => {
                                    retain_native(
                                        member.window_id,
                                        failure,
                                        &mut retained,
                                        &mut diagnostics,
                                    );
                                    None
                                }
                            }
                        }
                        #[cfg(feature = "test-faults")]
                        NativeMemberState::Retained { shell, error } => {
                            retain_native(
                                member.window_id,
                                MainWindowStartupDisposalFailure { shell, error },
                                &mut retained,
                                &mut diagnostics,
                            );
                            None
                        }
                        NativeMemberState::InFlight => {
                            unreachable!("startup drains only joined native flights")
                        }
                    };
                    if let Some(drain) = drain {
                        drains.push((member.window_id, drain));
                    }
                }
                drains
            })
            .expect("startup disposal retains a live GUI executor through native admission");
        for (window_id, drain) in drains {
            let retirement = match drain {
                NativeDrain::Prepared(prepared) => retirement_from_prepared(*prepared),
                NativeDrain::Native(receipt) => match receipt
                    .await
                    .expect("native disposal invokes its owned completion")
                {
                    MainWindowStartupDisposalCompletion::Ready { retirement } => retirement,
                    MainWindowStartupDisposalCompletion::Rejected(failure) => {
                        retain_native(window_id, failure, &mut retained, &mut diagnostics);
                        continue;
                    }
                },
            };
            let services = owner.services.clone();
            #[cfg(feature = "test-faults")]
            let mut hook = self.faults.before_retirement.take();
            let task = cx.background_executor().spawn(async move {
                let mut retirement = retirement;
                #[cfg(feature = "test-faults")]
                if let Some(hook) = &mut hook {
                    hook(window_id, &mut retirement);
                }
                let result = retire_member(retirement, &services);
                (result, {
                    #[cfg(feature = "test-faults")]
                    {
                        hook
                    }
                    #[cfg(not(feature = "test-faults"))]
                    {
                        ()
                    }
                })
            });
            let (result, hook) = task.await;
            #[cfg(feature = "test-faults")]
            {
                self.faults.before_retirement = hook;
            }
            #[cfg(not(feature = "test-faults"))]
            let _ = hook;
            if let Some(error) = result.error {
                diagnostics.push(MainWindowNativeRestoreSetMemberFailure { window_id, error });
            }
            if let Some(custody) = result.custody {
                retained.push(MainWindowNativeRetainedMember {
                    window_id,
                    custody: custody.into_native(),
                });
            }
        }
        // Native admission failures can complete before earlier workers; restore the fixed order.
        retained.sort_by_key(|member| {
            owner
                .expected_windows
                .iter()
                .position(|id| *id == member.window_id)
        });
        diagnostics.sort_by_key(|member| {
            owner
                .expected_windows
                .iter()
                .position(|id| *id == member.window_id)
        });
        MainWindowNativeRestoreSetFailure {
            error,
            diagnostics,
            retained: if retained.is_empty() {
                None
            } else {
                Some(RetainedNativeMainWindowRestoreSet {
                    owner,
                    members: retained,
                })
            },
        }
    }
}

fn retain_native(
    window_id: WindowId,
    failure: MainWindowStartupDisposalFailure,
    retained: &mut Vec<MainWindowNativeRetainedMember>,
    diagnostics: &mut Vec<MainWindowNativeRestoreSetMemberFailure>,
) {
    diagnostics.push(MainWindowNativeRestoreSetMemberFailure {
        window_id,
        error: failure.error,
    });
    retained.push(MainWindowNativeRetainedMember {
        window_id,
        custody: MainWindowNativeRetainedCustody::Native(Box::new(failure.shell)),
    });
}

fn retirement_from_prepared(
    prepared: MainWindowStartupShellPrepared,
) -> MainWindowStartupRetirement {
    match prepared {
        MainWindowStartupShellPrepared::Acquired(prepared) => {
            MainWindowStartupRetirement::AcquiredUnpublished(prepared.into_unpublished())
        }
        MainWindowStartupShellPrepared::Restored(prepared) => {
            MainWindowStartupRetirement::Restored(prepared.into_unpublished())
        }
        MainWindowStartupShellPrepared::Threadless(prepared) => {
            drop(prepared);
            MainWindowStartupRetirement::Threadless
        }
    }
}

#[inline(never)]
fn retire_member(
    retirement: MainWindowStartupRetirement,
    services: &MainWindowCreationServices,
) -> WorkerRetirementResult {
    match retirement {
        MainWindowStartupRetirement::Threadless => WorkerRetirementResult::settled(),
        MainWindowStartupRetirement::Restored(unpublished) => retire_restored(unpublished),
        MainWindowStartupRetirement::AcquiredPreserved(custody) => retire_preserved(custody),
        MainWindowStartupRetirement::AcquiredUnpublished(unpublished) => {
            abandon_unpublished(unpublished, services)
        }
    }
}

#[inline(never)]
fn retire_restored(unpublished: RestoredWindowShellUnpublished) -> WorkerRetirementResult {
    match unpublished.retire(CommandCancellation::new()) {
        RestoredWindowShellRetirement::Retired => WorkerRetirementResult::settled(),
        RestoredWindowShellRetirement::Pending { unpublished, error } => {
            WorkerRetirementResult::retained(
                WorkerRetainedCustody::Restored(Box::new(unpublished)),
                error,
            )
        }
    }
}

#[inline(never)]
fn retire_preserved(custody: MainWindowShellRecordPreservingRetirement) -> WorkerRetirementResult {
    match custody.retire(CommandCancellation::new()) {
        MainWindowShellRecordPreservingRetirementOutcome::Retired => {
            WorkerRetirementResult::settled()
        }
        MainWindowShellRecordPreservingRetirementOutcome::Pending { custody, error } => {
            WorkerRetirementResult::retained(
                WorkerRetainedCustody::AcquiredPreserved(Box::new(custody)),
                error,
            )
        }
    }
}

#[inline(never)]
fn abandon_unpublished(
    unpublished: MainWindowShellUnpublished,
    services: &MainWindowCreationServices,
) -> WorkerRetirementResult {
    match unpublished.prepare_abandonment(&services.acquisition, CommandCancellation::new()) {
        MainWindowShellAbandonmentPreparationOutcome::InitialComposerPending {
            unpublished,
            error,
        } => WorkerRetirementResult::retained(
            WorkerRetainedCustody::AcquiredUnpublished(Box::new(unpublished)),
            error,
        ),
        MainWindowShellAbandonmentPreparationOutcome::NotCommitted {
            unpublished,
            evidence,
        } => WorkerRetirementResult::retained(
            WorkerRetainedCustody::AcquiredUnpublished(Box::new(unpublished)),
            format!("startup abandonment preparation rejected: {evidence:?}"),
        ),
        MainWindowShellAbandonmentPreparationOutcome::Collision { .. } => WorkerRetirementResult {
            custody: None,
            error: Some("startup abandonment preparation found a collision".to_owned()),
        },
        MainWindowShellAbandonmentPreparationOutcome::ExactAcquired { abandonment } => {
            abandon_prepared(abandonment, services)
        }
    }
}

#[inline(never)]
fn abandon_prepared(
    abandonment: MainWindowShellAbandonment,
    services: &MainWindowCreationServices,
) -> WorkerRetirementResult {
    match abandonment.abandon(&services.acquisition, CommandCancellation::new()) {
        MainWindowShellAbandonmentOutcome::NotCommitted {
            abandonment,
            evidence,
        } => WorkerRetirementResult::retained(
            WorkerRetainedCustody::AcquiredAbandonment(Box::new(abandonment)),
            format!("startup abandonment rejected: {evidence:?}"),
        ),
        MainWindowShellAbandonmentOutcome::Indeterminate {
            reconciliation,
            failure,
            ..
        } => WorkerRetirementResult::retained(
            WorkerRetainedCustody::AcquiredReconciliation(Box::new(reconciliation)),
            failure.to_string(),
        ),
        MainWindowShellAbandonmentOutcome::Committed {
            receipt,
            later_failure,
            local_finalization: Some(local_finalization),
            ..
        } => WorkerRetirementResult::retained(
            WorkerRetainedCustody::CommittedAbandonment {
                receipt,
                later_failure,
                local_finalization,
            },
            "startup abandonment retains committed local finalization".to_owned(),
        ),
        MainWindowShellAbandonmentOutcome::Committed { later_failure, .. } => {
            WorkerRetirementResult {
                custody: None,
                error: later_failure.map(|error| error.to_string()),
            }
        }
    }
}
