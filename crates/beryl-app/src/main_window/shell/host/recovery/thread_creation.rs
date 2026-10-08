use super::*;

impl MainWindowShellRoot {
    pub(crate) fn adopt_recovered_thread_creation_shell(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        preparation: &mut crate::main_window::MainWindowFreshComposerPreparation,
        retirement: &mut crate::main_window::MainWindowFailedThreadCreationRetirement,
        adapters: PreparedComposerRecoveryAdapters,
        mut configure: crate::main_window::MainWindowShellComposerConfigurator,
        transcript: crate::syndic_transcript::PreparedTranscriptActivation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerCloseTicket, String> {
        if draft.root != cx.entity_id()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
            || draft.retirement.is_some()
            || draft.failed.is_some()
        {
            return Err("New Thread recovery lost its exact gated draft".into());
        }
        let failed = draft
            .thread_creation
            .as_mut()
            .ok_or("original New Thread capture is missing")?;
        if failed.attachment.is_some() {
            return Err("original New Thread candidate mount still retains cleanup custody".into());
        }
        if !failed.retired {
            return Err("original New Thread owners have not retired".into());
        }
        let capture = failed
            .capture
            .as_mut()
            .ok_or("original New Thread widget capture is missing")?;
        let (old_mount, old_editor, _) = draft
            .composer
            .as_ref()
            .ok_or("original New Thread mount custody is missing")?;
        let controller = self
            .controller
            .as_ref()
            .ok_or("New Thread recovery controller is missing")?;
        if controller.composer_mount.as_ref() != Some(old_mount)
            || capture.selected.entity_id() != *old_editor
        {
            return Err("original New Thread mount identity changed".into());
        }
        if !matches!(&controller.content, ShellContent::Retired { window_id, placement, threadless: false, reservation: Some(_) } if *window_id == preparation.window().window_id() && placement == preparation.window().placement())
        {
            return Err("New Thread recovery native reservation changed".into());
        }
        old_mount.update(cx, |mount, cx| {
            mount.release_failed_thread_creation_widgets(capture, retirement, window, cx)
        })?;
        #[cfg(all(test, feature = "test-faults"))]
        {
            self.running_threads.fixture_creation_release_counts = capture.widget_release_counts();
        }
        retirement.validate_complete_retirement()?;
        let prepared = preparation.prepare(&mut configure)?;
        let original = draft.composer.clone().unwrap();
        let result = MainWindowConversationComposerMount::from_fresh_recovery(
            prepared,
            configure,
            adapters,
            preparation,
            window,
            cx,
        );
        let (mount, close) = match result {
            Ok(ready) => ready,
            Err((error, Some(mount))) => {
                self.retain_recovered_thread_creation_mount(draft, original, mount, cx)?;
                return Err(error);
            }
            Err((error, None)) => return Err(error),
        };
        self.retain_recovered_thread_creation_mount(draft, original, mount.clone(), cx)?;
        #[cfg(all(test, feature = "test-faults"))]
        if std::mem::take(&mut self.running_threads.fixture_reject_creation_recovery_mount) {
            return Err("retained fresh New Thread mount was refused".into());
        }
        let resident = mount
            .read(cx)
            .contribution()
            .ok_or("fresh New Thread editor is missing")?;
        let selection = resident.read(cx).selection_identity();
        if preparation.window().selected_thread() != Some(selection.claim()) {
            return Err("fresh New Thread claim and editor differ".into());
        }
        let controller = self.controller.as_mut().unwrap();
        let ShellContent::Retired { reservation, .. } = &mut controller.content else {
            unreachable!()
        };
        controller.content = ShellContent::Selected {
            window: preparation.window().clone(),
            selection,
            reservation: reservation.take().unwrap(),
        };
        controller.composer_mount = Some(mount.clone());
        draft.composer = Some((mount, resident.entity_id(), close));
        let input = resident.read(cx).gpui_input();
        self.composer_observer = Some(cx.observe(&input, |_, _, cx| cx.notify()));
        self.running_threads.transcript =
            cx.new(crate::syndic_transcript::SyndicTranscriptPanel::new);
        self.running_threads
            .transcript
            .update(cx, |panel, panel_cx| {
                panel.publish_coherent_activation(transcript);
                panel_cx.notify();
            });
        self.running_threads.transcript_claim = Some(selection.claim());
        cx.notify();
        Ok(close)
    }

    fn retain_recovered_thread_creation_mount(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        original: (
            Entity<MainWindowConversationComposerMount>,
            gpui::EntityId,
            MainWindowConversationComposerCloseTicket,
        ),
        mount: Entity<MainWindowConversationComposerMount>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let close = mount
            .read(cx)
            .fresh_recovery_ticket()
            .expect("fresh recovery construction retains its issued close ticket");
        let editor = mount
            .read(cx)
            .contribution()
            .expect("fresh recovery construction retains its mounted editor")
            .entity_id();
        let fresh = (mount.clone(), editor, close);
        draft.thread_creation.as_mut().unwrap().attachment =
            Some(super::super::shutdown_draft::RetainedThreadCreationMount {
                original,
                fresh: fresh.clone(),
                transcript: self.running_threads.transcript.clone(),
                transcript_claim: self.running_threads.transcript_claim,
            });
        draft.composer = Some(fresh);
        self.controller.as_mut().unwrap().composer_mount = Some(mount);
        Ok(())
    }

    pub(crate) fn detach_recovered_thread_creation_shell(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if draft.root != cx.entity_id()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
        {
            return Err("New Thread candidate cleanup lost its exact shell fence".into());
        }
        let creation = draft
            .thread_creation
            .as_mut()
            .ok_or("original New Thread cleanup custody is missing")?;
        if !creation.retired {
            return Err("original New Thread owners are not retired".into());
        }
        let Some(attachment) = creation.attachment.as_ref() else {
            return Ok(true);
        };
        let (mount, editor, close) = &attachment.fresh;
        let controller = self
            .controller
            .as_ref()
            .ok_or("New Thread candidate cleanup controller is missing")?;
        if close.selection().binding().home_id() != home
            || close.selection().binding().home_generation() != generation
            || draft.composer.as_ref() != Some(&attachment.fresh)
            || controller.composer_mount.as_ref() != Some(mount)
            || mount.read(cx).fresh_recovery_ticket() != Some(*close)
            || mount
                .read(cx)
                .contribution()
                .is_none_or(|resident| resident.entity_id() != *editor)
            || controller.window_id() != close.selection().window_id()
            || creation.capture.as_ref().is_none_or(|capture| {
                !capture.widgets_released() || capture.selected.entity_id() != attachment.original.1
            })
        {
            return Err("New Thread candidate cleanup mount identity changed".into());
        }
        if !mount.update(cx, |mount, cx| {
            mount.detach_fresh_recovery(*close, window, cx)
        })? {
            return Ok(false);
        }
        let controller = self.controller.as_mut().unwrap();
        controller.retire_construction()?;
        if !matches!(
            &controller.content,
            ShellContent::Retired {
                threadless: false,
                reservation: Some(_),
                ..
            }
        ) {
            return Err(
                "New Thread candidate cleanup lost its surviving native reservation".into(),
            );
        }
        let attachment = creation.attachment.take().unwrap();
        controller.composer_mount = Some(attachment.original.0.clone());
        draft.composer = Some(attachment.original);
        self.composer_observer = None;
        self.running_threads.transcript = attachment.transcript;
        self.running_threads.transcript_claim = attachment.transcript_claim;
        cx.notify();
        Ok(true)
    }
}
