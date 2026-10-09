use super::*;

#[cfg(test)]
type NonfinalPageReleaseEvidence = Vec<(
    crate::main_window::MainWindowComposerSelectionIdentity,
    u64,
    u64,
    usize,
)>;

#[cfg(test)]
struct NonfinalPageReleaseObserver {
    window: beryl_model::WindowId,
    identity: std::rc::Weak<()>,
    evidence: std::rc::Weak<RefCell<NonfinalPageReleaseEvidence>>,
}

#[cfg(test)]
thread_local! {
    static NONFINAL_PAGE_RELEASE_OBSERVER: RefCell<Option<NonfinalPageReleaseObserver>> = const { RefCell::new(None) };
}

#[cfg(test)]
struct NonfinalPageReleaseObservationGuard {
    identity: std::rc::Rc<()>,
}

#[cfg(test)]
impl Drop for NonfinalPageReleaseObservationGuard {
    fn drop(&mut self) {
        NONFINAL_PAGE_RELEASE_OBSERVER.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.as_ref().is_some_and(|observer| {
                observer
                    .identity
                    .ptr_eq(&std::rc::Rc::downgrade(&self.identity))
            }) {
                slot.take();
            }
        });
    }
}

impl MainWindowShell {
    #[cfg(test)]
    pub(crate) fn test_observe_nonfinal_page_release(
        window: beryl_model::WindowId,
        evidence: std::rc::Rc<RefCell<NonfinalPageReleaseEvidence>>,
    ) -> Result<impl Drop, String> {
        let identity = std::rc::Rc::new(());
        NONFINAL_PAGE_RELEASE_OBSERVER.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot
                .as_ref()
                .is_some_and(|observer| observer.identity.upgrade().is_some())
            {
                return Err("nonfinal Page release observation is already active".into());
            }
            *slot = Some(NonfinalPageReleaseObserver {
                window,
                identity: std::rc::Rc::downgrade(&identity),
                evidence: std::rc::Rc::downgrade(&evidence),
            });
            Ok(NonfinalPageReleaseObservationGuard { identity })
        })
    }

    #[cfg(test)]
    fn test_record_nonfinal_page_release(
        window: beryl_model::WindowId,
        draft: &MainWindowShutdownDraft,
    ) {
        if draft.failed.is_some() {
            return;
        }
        let observer = NONFINAL_PAGE_RELEASE_OBSERVER.with(|slot| {
            slot.borrow()
                .as_ref()
                .filter(|observer| {
                    observer.window == window && observer.identity.upgrade().is_some()
                })
                .and_then(|observer| observer.evidence.upgrade())
        });
        let Some(observer) = observer else { return };
        let Ok(evidence) = draft.test_original_page_release_evidence() else {
            return;
        };
        if evidence
            .iter()
            .any(|(selection, _, _, _)| selection.window_id() != window)
        {
            return;
        }
        if let Ok(mut observed) = observer.try_borrow_mut() {
            *observed = evidence;
        }
    }

    #[cfg(test)]
    pub(crate) fn test_nonfinal_native_close_outcome(
        &self,
    ) -> Option<gpui::WindowsNativeWindowDestructionOutcome> {
        self.nonfinal_native_destruction
            .as_ref()
            .and_then(|attempt| attempt.outcome())
    }
    #[cfg(test)]
    pub(crate) fn test_fail_next_nonfinal_native_close(
        &mut self,
        fault: gpui::WindowsNativeWindowDestructionTestFault,
    ) {
        self.nonfinal_native_fault = Some(fault);
    }

    pub(crate) fn nonfinal_native_recovery_allowed(&self) -> bool {
        self.nonfinal_native_destruction.is_none()
            || self.require_nonfinal_native_survival().is_ok()
    }

    pub(crate) fn release_settled_nonfinal_native_if_present(&mut self) -> Result<(), String> {
        if self.nonfinal_native_destruction.is_some() {
            self.release_nonfinal_native_survival()?;
        }
        Ok(())
    }
    pub(crate) fn begin_nonfinal_native_cleanup(
        &mut self,
        request: &std::rc::Rc<()>,
        app: &mut App,
    ) -> Result<gpui::WindowsNativeWindowDestructionCompleted, String> {
        let root = self.root.read(app);
        if !self.published
            || !root.running_thread_reads_drained()
            || !root.ordinary_close_interaction_gated
            || !root.shutdown_interaction_gated
            || root.startup_interaction_gated()
            || !self.desktop_cleanup_allowed()
            || self.nonfinal_native_destruction.is_some()
            || root
                .controller
                .as_ref()
                .is_none_or(|controller| matches!(controller.content, ShellContent::Retired { .. }))
        {
            return Err(
                "recoverable native cleanup requires the retained exact nonfinal shell".into(),
            );
        }
        self.release_pre_native_close(request, app)?;
        #[cfg(test)]
        let fault = self.nonfinal_native_fault.take();
        let (control, completed) = self
            .window
            .update(app, |_, window, _| {
                #[cfg(test)]
                if let Some(fault) = fault {
                    return gpui::with_windows_window_destruction_fault_for_test(fault, || {
                        window.begin_windows_native_destruction()
                    });
                }
                window.begin_windows_native_destruction()
            })
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        self.nonfinal_native_destruction = Some(control);
        Ok(completed)
    }

    pub(crate) fn require_nonfinal_native_survival(&self) -> Result<(), String> {
        if self
            .nonfinal_native_destruction
            .as_ref()
            .and_then(|attempt| attempt.outcome())
            .is_some_and(|outcome| {
                matches!(
                    outcome,
                    gpui::WindowsNativeWindowDestructionOutcome::Survived { .. }
                )
            })
        {
            Ok(())
        } else {
            Err("exact surviving native close settlement is unavailable".into())
        }
    }

    pub(crate) fn release_nonfinal_native_survival(&mut self) -> Result<(), String> {
        self.require_nonfinal_native_survival()?;
        self.nonfinal_native_destruction = None;
        Ok(())
    }

    pub(crate) fn retire_destroyed_nonfinal_draft(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        app: &mut App,
    ) -> Result<bool, String> {
        if !self
            .nonfinal_native_destruction
            .as_ref()
            .and_then(|attempt| attempt.outcome())
            .is_some_and(|outcome| {
                matches!(
                    outcome,
                    gpui::WindowsNativeWindowDestructionOutcome::Destroyed
                )
            })
        {
            return Err("exact destroyed native close settlement is unavailable".into());
        }
        let window = self.retained_window_id(app)?;
        if !self
            .root
            .update(app, |root, cx| root.retire_final_shutdown_draft(draft, cx))?
        {
            return Ok(false);
        }
        self.root.update(app, |root, cx| {
            root.release_destroyed_final_shutdown_resident(draft, cx)
        })?;
        let result = draft.advance_destroyed_final_cleanup(window);
        #[cfg(test)]
        Self::test_record_nonfinal_page_release(window, draft);
        result
    }
}

impl MainWindowShellRoot {
    pub(crate) fn reattach_nonfinal_native_draft(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        restored: Option<(
            &beryl_state::SessionWindowRecord,
            Option<crate::main_window::MainWindowConversationComposerCloseTicket>,
        )>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.ordinary_close_interaction_gated {
            return Err("nonfinal native recovery lost its ordinary close gate".into());
        }
        self.reattach_retained_close_draft(draft, restored, cx)
    }

    fn reattach_retained_close_draft(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        restored: Option<(
            &beryl_state::SessionWindowRecord,
            Option<crate::main_window::MainWindowConversationComposerCloseTicket>,
        )>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if draft.root != cx.entity_id()
            || !draft.detached_installed
            || !self.shutdown_interaction_gated
        {
            return Err("nonfinal native recovery lost reversible shell custody".into());
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or("nonfinal native recovery controller is unavailable")?;
        if let Some((record, _)) = restored {
            if record.window_id() != controller.window_id()
                || record.placement() != controller.placement()
            {
                return Err("nonfinal native recovery restored another window or placement".into());
            }
        }
        if let Some((mount, editor, close)) = draft.composer.as_mut() {
            if controller.composer_mount.as_ref() != Some(mount)
                || mount
                    .read(cx)
                    .contribution()
                    .is_none_or(|resident| resident.entity_id() != *editor)
            {
                return Err("nonfinal native recovery resident identity changed".into());
            }
            let fresh = match restored {
                Some((record, Some(fresh)))
                    if record.selected_thread() == Some(fresh.selection().claim()) =>
                {
                    fresh
                }
                Some(_) => {
                    return Err("nonfinal native recovery restored claim is unavailable".into());
                }
                None => *close,
            };
            let Some(shutdown_draft::recovery::ResidentRetirement::Detached(resources)) =
                draft.retirement.as_mut()
            else {
                return Err("nonfinal native recovery reversible resources are unavailable".into());
            };
            mount.update(cx, |mount, cx| {
                mount.reattach_native_close_custody(resources, fresh, cx)
            })?;
            match &mut controller.content {
                ShellContent::Acquired { selection, .. }
                | ShellContent::Restored { selection, .. } => *selection = fresh.selection(),
                ShellContent::Selected {
                    window, selection, ..
                } => {
                    *selection = fresh.selection();
                    if let Some((record, _)) = restored {
                        *window = record.clone();
                    }
                }
                _ => {
                    return Err(
                        "nonfinal native recovery selected construction custody changed".into(),
                    );
                }
            }
            *close = fresh;
            draft.retirement = None;
        } else if !controller.is_threadless()
            || restored.is_some_and(|(record, fresh)| {
                record.selected_thread().is_some() || fresh.is_some()
            })
        {
            return Err("nonfinal native recovery threadless correspondence changed".into());
        }
        draft.detached_installed = false;
        Ok(())
    }
}

impl MainWindowShell {
    pub(crate) fn reattach_pre_native_close_draft(
        &mut self,
        request: &std::rc::Rc<()>,
        draft: &mut MainWindowShutdownDraft,
        restored: Option<(
            &beryl_state::SessionWindowRecord,
            Option<crate::main_window::MainWindowConversationComposerCloseTicket>,
        )>,
        app: &mut App,
    ) -> Result<(), String> {
        self.require_pre_native_close(request, app)?;
        self.window
            .update(app, |root, _, cx| {
                root.reattach_retained_close_draft(draft, restored, cx)
            })
            .map_err(|error| error.to_string())?
    }
}
