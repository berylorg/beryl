use super::*;
#[cfg(test)]
use crate::main_window::MainWindowConversationComposerCloseAdvance;
use crate::{
    main_window::{MainWindowFreshComposerPreparation, MainWindowShellComposerConfigurator},
    runtime_admission::recovery::FirstConversationFacts,
    syndic_transcript::PreparedTranscriptActivation,
};

impl MainWindowShellRoot {
    #[cfg(test)]
    pub(crate) fn test_first_conversation_transcript_claim(
        &self,
        claim: beryl_state::WindowClaimSelection,
        cx: &App,
    ) -> bool {
        self.running_threads.transcript_claim == Some(claim)
            && self
                .running_threads
                .transcript
                .read(cx)
                .snapshot()
                .activation_revision
                != 0
            && self
                .running_threads
                .transcript
                .read(cx)
                .snapshot()
                .is_empty()
            && !self.shutdown_interaction_gated
            && self
                .controller
                .as_ref()
                .and_then(|controller| controller.composer_mount.as_ref())
                .and_then(|mount| mount.read(cx).contribution())
                .is_some_and(|resident| !resident.read(cx).mutation_gated())
    }

    #[cfg(test)]
    pub(crate) fn test_remove_retired_first_conversation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.shutdown_interaction_gated
            || self.controller.as_ref().is_none_or(|controller| {
                !matches!(
                    controller.content,
                    ShellContent::Retired {
                        reservation: Some(_),
                        ..
                    }
                ) || controller.composer_mount.is_some()
            })
        {
            return Err(
                "virtual refused first conversation teardown requires retired shell custody".into(),
            );
        }
        self.retire_notices(window, cx);
        self.controller.take();
        self.composer_observer.take();
        self.running_threads
            .transcript
            .update(cx, |panel, _| panel.retire());
        window.remove_window();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_dispose_and_remove_published_first_conversation(
        &mut self,
        draft: &MainWindowShutdownDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if draft.root != cx.entity_id() || !self.shutdown_interaction_gated {
            return Err(
                "virtual published first conversation disposal lost its exact draft".into(),
            );
        }
        let (mount, editor, close) = draft
            .composer
            .as_ref()
            .ok_or("virtual published first conversation disposal lost its editor")?;
        if self
            .controller
            .as_ref()
            .and_then(|controller| controller.composer_mount.as_ref())
            != Some(mount)
            || mount
                .read(cx)
                .contribution()
                .is_some_and(|resident| resident.entity_id() != *editor)
        {
            return Err("virtual published first conversation disposal mount changed".into());
        }
        let disposed = mount.update(cx, |mount, cx| {
            match mount.advance_window_close(*close, window, cx)? {
                MainWindowConversationComposerCloseAdvance::Ready => {
                    mount.authorize_window_close_disposal(*close, window, cx)?;
                    Ok(false)
                }
                MainWindowConversationComposerCloseAdvance::Disposed => {
                    if mount.contribution().is_some() {
                        return Err(
                            "virtual published first conversation disposal retained its resident"
                                .into(),
                        );
                    }
                    Ok(true)
                }
                MainWindowConversationComposerCloseAdvance::Stale => {
                    Err("virtual published first conversation disposal close changed".into())
                }
                MainWindowConversationComposerCloseAdvance::Unsatisfied(error) => Err(format!(
                    "virtual published first conversation disposal refused: {error:?}"
                )),
                _ => Ok(false),
            }
        })?;
        if !disposed {
            return Ok(false);
        }
        self.retire_notices(window, cx);
        self.controller.take();
        self.composer_observer.take();
        self.running_threads
            .transcript
            .update(cx, |panel, _| panel.retire());
        window.remove_window();
        Ok(true)
    }
    pub(crate) fn adopt_first_conversation_shell(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        facts: &FirstConversationFacts,
        preparation: &mut MainWindowFreshComposerPreparation,
        adapters: PreparedComposerRecoveryAdapters,
        mut configure: MainWindowShellComposerConfigurator,
        transcript: PreparedTranscriptActivation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if draft.root != cx.entity_id()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
            || draft.retirement.is_some()
            || draft.failed.is_some()
        {
            return Err("first conversation lost its exact recovery shell draft".into());
        }
        let controller = self
            .controller
            .as_ref()
            .ok_or("first conversation shell controller is unavailable")?;
        if let Some((mount, editor, close)) = &draft.composer {
            if controller.composer_mount.as_ref() == Some(mount)
                && mount
                    .read(cx)
                    .contribution()
                    .is_some_and(|r| r.entity_id() == *editor)
                && mount.read(cx).recovery_binding_current(*close)
                && matches!(&controller.content, ShellContent::Selected { window, selection, .. }
                    if window == facts.window() && Some(*selection) == preparation.selection())
            {
                return Ok(());
            }
            return Err(
                "first conversation attachment retains incomplete original mount custody".into(),
            );
        }
        if !matches!(&controller.content, ShellContent::Retired { window_id, placement, threadless: true, reservation: Some(_) }
            if *window_id == facts.window().window_id() && placement == facts.window().placement())
            || controller.composer_mount.is_some()
            || preparation.window() != facts.window()
            || preparation
                .selection()
                .is_none_or(|s| facts.window().selected_thread() != Some(s.claim()))
        {
            return Err(
                "first conversation does not match its retired threadless reservation".into(),
            );
        }
        let prepared = preparation.prepare(&mut configure)?;
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
                if let Some(close) = mount.read(cx).fresh_recovery_ticket() {
                    let editor = mount
                        .read(cx)
                        .contribution()
                        .ok_or("partial first conversation resident is missing")?
                        .entity_id();
                    draft.composer = Some((mount.clone(), editor, close));
                }
                self.controller.as_mut().unwrap().composer_mount = Some(mount);
                return Err(error);
            }
            Err((error, None)) => return Err(error),
        };
        let resident = mount
            .read(cx)
            .contribution()
            .ok_or("first conversation resident is missing")?;
        let selection = resident.read(cx).selection_identity();
        let controller = self.controller.as_mut().unwrap();
        let ShellContent::Retired { reservation, .. } = &mut controller.content else {
            unreachable!()
        };
        controller.content = ShellContent::Selected {
            window: facts.window().clone(),
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
        Ok(())
    }

    pub(crate) fn advance_first_conversation_shell(
        &mut self,
        draft: &MainWindowShutdownDraft,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if draft.root != cx.entity_id()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
        {
            return Err("first conversation preparation shell fence changed".into());
        }
        let (mount, editor, close) = draft
            .composer
            .as_ref()
            .ok_or("first conversation preparation mount is missing")?;
        let controller = self
            .controller
            .as_ref()
            .ok_or("first conversation preparation controller is missing")?;
        if close.selection().binding().home_id() != home
            || close.selection().binding().home_generation() != generation
            || controller.composer_mount.as_ref() != Some(mount)
            || mount.read(cx).fresh_recovery_ticket() != Some(*close)
            || !mount
                .read(cx)
                .contribution()
                .is_some_and(|resident| resident.entity_id() == *editor)
            || !matches!(&controller.content, ShellContent::Selected { window, selection, .. }
                if window.selected_thread() == Some(close.selection().claim()) && *selection == close.selection())
        {
            return Err("first conversation preparation mount identity changed".into());
        }
        let ready = mount.update(cx, |mount, cx| {
            mount.advance_fresh_recovery(*close, window, cx)
        })?;
        cx.notify();
        Ok(ready)
    }

    pub(crate) fn detach_first_conversation_shell(
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
            return Err("first conversation cleanup shell fence changed".into());
        }
        if let Some((mount, _, close)) = &draft.composer {
            if close.selection().binding().home_id() != home
                || close.selection().binding().home_generation() != generation
            {
                return Err("first conversation cleanup candidate identity changed".into());
            }
            if !mount.update(cx, |mount, cx| {
                mount.detach_fresh_recovery(*close, window, cx)
            })? {
                return Ok(false);
            }
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or("first conversation cleanup controller is missing")?;
        controller.retire_construction()?;
        if let ShellContent::Retired { threadless, .. } = &mut controller.content {
            *threadless = true;
        }
        controller.composer_mount = None;
        draft.composer = None;
        self.composer_observer = None;
        self.running_threads
            .transcript
            .update(cx, |panel, _| panel.retire());
        self.running_threads.transcript_claim = None;
        cx.notify();
        Ok(true)
    }
}
