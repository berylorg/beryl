use super::*;
use crate::main_window::MainWindowFailedThreadCreationGuiAuthority;

pub(in crate::main_window::shell::host) struct CapturedThreadCreationOperation {
    original: Option<UnviewedRunningActivation>,
    pub(in crate::main_window::shell::host) retired:
        Option<Box<crate::app_services::RetiredSameWindowThreadOperation>>,
}

pub(in crate::main_window::shell::host) struct RetiringThreadCreationOperation {
    pub(in crate::main_window::shell::host) operation:
        Box<crate::app_services::RetiredSameWindowThreadOperation>,
    pub(in crate::main_window::shell::host) service: Option<Arc<Service>>,
    pub(in crate::main_window::shell::host) source:
        Option<Box<crate::main_window::MainWindowThreadCreationRetirementSource>>,
}

impl UnviewedRunningActivation {
    pub(in crate::main_window::shell::host) fn is_thread_creation(&self) -> bool {
        self.creation
    }
}

impl CapturedThreadCreationOperation {
    pub(in crate::main_window::shell::host) fn stop_transcript_reads(
        &self,
    ) -> Result<bool, String> {
        let original = self
            .original
            .as_ref()
            .ok_or("original thread creation is missing")?;
        let mut source = match original.source.try_lock() {
            Ok(source) => source,
            Err(_) => return Ok(false),
        };
        if source.reader.failed_retirement_home().health().state()
            != beryl_home_store::HomeHealthState::Failed
            || !original.suspended.load(Ordering::Acquire)
            || original.active.load(Ordering::Acquire)
        {
            return Err(
                "original thread creation transcript retirement lost its failed source".into(),
            );
        }
        source.transcript_cancel.store(true, Ordering::Release);
        source.transcript.take();
        Ok(source
            .provider
            .as_ref()
            .is_none_or(|provider| provider.retire()))
    }
    pub(in crate::main_window::shell::host) fn mount(&self) -> Entity<Mount> {
        self.original
            .as_ref()
            .expect("captured original creation")
            .mount
            .clone()
    }

    pub(in crate::main_window::shell::host) fn authority(
        &self,
    ) -> Result<MainWindowFailedThreadCreationGuiAuthority, String> {
        let source = self
            .original
            .as_ref()
            .ok_or("original thread creation already retired")?
            .source
            .try_lock()
            .map_err(|_| "original thread creation source is busy")?;
        Ok(MainWindowFailedThreadCreationGuiAuthority {
            prior: source.prior,
            receipt: source.receipt,
            successor: source.pending_selection.or_else(|| {
                source
                    .publication
                    .as_ref()
                    .map(|publication| publication.selection())
            }),
            committed: source.committed_selection().is_some()
                || self
                    .retired
                    .as_ref()
                    .is_some_and(|retired| retired.committed().is_some()),
            release: source.release,
            widget_disposal_requested: source.widget_work.is_some() || source.release.is_some(),
        })
    }

    pub(in crate::main_window::shell::host) fn service(&self) -> Result<Arc<Service>, String> {
        Ok(self
            .original
            .as_ref()
            .ok_or("original thread creation already retired")?
            .source
            .try_lock()
            .map_err(|_| "original thread creation source is busy")?
            .service
            .clone())
    }

    pub(in crate::main_window::shell::host) fn export_operation(&mut self) -> Result<(), String> {
        if self.retired.is_some() {
            return Ok(());
        }
        let original = self
            .original
            .as_ref()
            .ok_or("original thread creation is missing")?;
        let mut source = original
            .source
            .try_lock()
            .map_err(|_| "original thread creation source is busy")?;
        if source
            .provider
            .as_ref()
            .is_some_and(|provider| !provider.retire())
        {
            return Err("original thread creation transcript reads are still draining".into());
        }
        let reader = source.reader.clone();
        let lease = source.lease.clone();
        let retired = source
            .creation
            .as_mut()
            .ok_or("original thread creation entrance changed")?
            .retire_failed_home(&reader, lease)?;
        self.retired = Some(Box::new(retired));
        Ok(())
    }

    pub(in crate::main_window::shell::host) fn into_retirement_parts(
        mut self,
        mounted_successor: Option<Selection>,
    ) -> Result<RetiringThreadCreationOperation, (Self, String)> {
        if let Err(error) = self.export_operation() {
            return Err((self, error));
        }
        let original_source = self.original.as_ref().unwrap().source.clone();
        let mut source = match original_source.try_lock() {
            Ok(source) => source,
            Err(_) => return Err((self, "original thread creation source is busy".into())),
        };
        let selected = match source.service.selected_identity() {
            Some(selected) => selected,
            None => {
                drop(source);
                return Err((
                    self,
                    "original failed thread creation selection is missing".into(),
                ));
            }
        };
        if let Err(error) = source
            .service
            .authenticate_failed_thread_creation_selection(selected, source.receipt, source.prior)
        {
            drop(source);
            return Err((self, error));
        }
        let operation = self.retired.as_mut().unwrap();
        let retired_source = Box::new(
            crate::main_window::MainWindowThreadCreationRetirementSource {
                prior: source.prior,
                selected,
                saved: operation.saved.take(),
                committed_target: operation.committed().map(|committed| committed.selection),
                receipt: source.receipt,
                completed_predecessor: source.completed_predecessor.take(),
                completed_successor: source.completed_successor.take(),
                completed_progress: source.completed_successor_progress.take(),
                mounted_successor,
                widget_work: source.widget_work.take(),
                release: source.release,
            },
        );
        let service = source.service.clone();
        drop(source);
        self.original.take();
        Ok(RetiringThreadCreationOperation {
            operation: self.retired.take().unwrap(),
            service: Some(service),
            source: Some(retired_source),
        })
    }
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn failed_thread_creation_mount_matches(
        &self,
        mount: &Entity<Mount>,
    ) -> bool {
        self.running_threads
            .activation_operation
            .as_ref()
            .is_some_and(|original| original.is_thread_creation() && original.mount == *mount)
    }
    pub(in crate::main_window::shell::host) fn capture_failed_thread_creation_operation(
        &mut self,
    ) -> Result<Option<CapturedThreadCreationOperation>, String> {
        let Some(original) = self.running_threads.activation_operation.as_ref() else {
            return Ok(None);
        };
        if !original.is_thread_creation() {
            return Ok(None);
        }
        original.suspended.store(true, Ordering::Release);
        self.running_threads.activation_cancel.cancel();
        self.running_threads.activation_task.take();
        if original.active.load(Ordering::Acquire) || self.running_threads.workers.retained() != 0 {
            return Err("original thread creation workers are still draining".into());
        }
        {
            let source = original
                .source
                .try_lock()
                .map_err(|_| "original thread creation source is busy")?;
            if source.reader.failed_retirement_home().health().state()
                != beryl_home_store::HomeHealthState::Failed
            {
                return Err("original thread creation capture requires its failed Home".into());
            }
        }
        let original = self.running_threads.activation_operation.take().unwrap();
        self.running_threads.selection_lease.take();
        self.running_threads.pending_activation.take();
        self.running_threads.prepared_activation.take();
        Ok(Some(CapturedThreadCreationOperation {
            original: Some(original),
            retired: None,
        }))
    }

    pub(crate) fn has_failed_thread_creation_entrance(&self) -> bool {
        self.running_threads
            .activation_operation
            .as_ref()
            .is_some_and(|operation| operation.is_thread_creation())
    }
}
