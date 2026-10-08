use super::*;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host::running_threads::activation::attachment) fn accept_thread_creation_stage(
        &mut self,
        operation: &UnviewedRunningActivation,
        source: &mut ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match source.stage {
            Stage::ThreadFence => {
                source.expected = source
                    .service
                    .selected_identity()
                    .ok_or("Thread predecessor is missing")?;
                let ready = operation.mount.update(cx, |mount, mount_cx| {
                    mount.fence_thread_predecessor(source.expected, window, mount_cx)
                })?;
                source.creation.as_mut().unwrap().fenced = true;
                self.sync_running_selected_cache(source.prior, source.expected, cx)?;
                source.prior = source.expected;
                if ready {
                    source.stage = Stage::ThreadSave;
                }
            }
            Stage::ThreadAcceptSave => {
                let (advance, selected) = source
                    .creation
                    .as_ref()
                    .unwrap()
                    .advance
                    .as_ref()
                    .ok_or("Thread predecessor advance is missing")?;
                operation.mount.update(cx, |mount, mount_cx| {
                    mount.accept_thread_predecessor_selection(*selected, mount_cx)
                })?;
                self.sync_running_selected_cache(source.prior, *selected, cx)?;
                source.prior = *selected;
                source.expected = *selected;
                source.stage = match advance {
                    SaveAdvance::Progress(FlushState::DisposalRequired) => Stage::ThreadPrepare,
                    SaveAdvance::Progress(_) => Stage::ThreadSave,
                    SaveAdvance::ReconciliationPending => {
                        source.error = Some(
                            "Thread predecessor save is reconciling its original publication."
                                .into(),
                        );
                        Stage::ThreadSave
                    }
                    SaveAdvance::Unsatisfied(failure) => {
                        source.error = Some(format!("Thread predecessor save failed: {failure:?}"));
                        Stage::RestoreAutosave
                    }
                    SaveAdvance::Stale | SaveAdvance::Satisfied(_) => {
                        source.terminal_release_failure = true;
                        operation.suspend();
                        return Err(
                            "Thread predecessor original save custody is unavailable".into()
                        );
                    }
                };
            }
            Stage::ThreadRestore => {
                if source.creation.as_ref().unwrap().fenced {
                    operation.mount.update(cx, |mount, mount_cx| {
                        mount.resume_thread_predecessor(source.expected, window, mount_cx)
                    })?;
                }
                self.sync_running_selected_cache(source.prior, source.expected, cx)?;
                source.prior = source.expected;
                let prepared = source
                    .autosave
                    .take()
                    .ok_or("Thread predecessor autosave is missing")?;
                operation.mount.update(cx, |mount, mount_cx| {
                    mount.finalize_claim_autosave(source.expected, prepared, window, mount_cx)
                })?;
                source.stage = Stage::Finished;
            }
            _ => {}
        }
        Ok(())
    }
}
