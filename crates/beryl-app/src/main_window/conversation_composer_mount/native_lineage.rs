use gpui::prelude::*;
use gpui::{Context, FocusHandle, IntoElement, Render, Window, div};
use gpui_text_input::{RangePrepublicationCleanupToken, RangePrepublicationSessionGeneration};

use super::*;
use crate::cas_projection::{
    NativeLineageRecoveryControl, NativeLineageRecoveryKey, NativeLineageRecoverySnapshot,
    NativeLineageRecoveryStatus,
};
use crate::main_window::{
    MainWindowNativeLineagePrepublicationSource, MainWindowNativeLineageSourceRetentionError,
};

mod prompt;
mod realization;

pub(super) enum NativeLineageHostResult {
    Settled {
        route: NativeLineageRecoveryKey,
        selection: MainWindowComposerSelectionIdentity,
        generation: RangePrepublicationSessionGeneration,
        token: RangePrepublicationCleanupToken,
        source: Arc<MainWindowNativeLineagePrepublicationSource>,
    },
}

impl MainWindowConversationComposerMount {
    pub fn attach_native_lineage_recovery(
        &mut self,
        control: NativeLineageRecoveryControl,
        cx: &mut Context<Self>,
    ) {
        self.native_lineage_recovery = Some(control);
        cx.notify();
    }

    pub fn set_native_lineage_pending_focus(&mut self, focus: FocusHandle, cx: &mut Context<Self>) {
        self.native_lineage_pending_focus = Some(focus);
        cx.notify();
    }

    pub fn native_lineage_recovery_snapshot(&self) -> Option<NativeLineageRecoverySnapshot> {
        self.native_lineage_snapshot
    }

    #[cfg(feature = "test-faults")]
    pub fn test_native_lineage_mount_diagnostics(
        &self,
    ) -> super::MainWindowNativeLineageMountDiagnostics {
        super::MainWindowNativeLineageMountDiagnostics {
            snapshot_present: self.native_lineage_snapshot.is_some(),
            selection_current: self.native_lineage_selection == self.service.selected_identity(),
            seed_present: self.native_lineage_seed.is_some(),
            config_present: self.native_lineage_config.is_some(),
            validation_task_present: self.native_lineage_validation_task.is_some(),
            validation_result_present: self.native_lineage_validation.is_some(),
            prompt_published: self.native_lineage_prompt_published,
            failure_present: self.native_lineage_failure.is_some(),
            capacity_blocked: self.native_lineage_capacity_blocked_epoch.is_some(),
            disposal_active: self.native_lineage_disposal_active,
        }
    }

    fn ensure_native_lineage_refresh_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.native_lineage_recovery.is_none() || self.native_lineage_refresh_task.is_some() {
            return;
        }
        let executor = cx.background_executor().clone();
        self.native_lineage_refresh_task = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                executor.timer(Duration::from_millis(100)).await;
                let refreshed = this.update_in(cx, |this, window, cx| {
                    let result = this.refresh_native_lineage_recovery(window, cx);
                    cx.notify();
                    result
                });
                if !matches!(refreshed, Ok(Ok(_))) {
                    break;
                }
            }
        }));
    }

    pub fn refresh_native_lineage_recovery(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if let Err(error) = self.finish_native_lineage_host_result(window, cx) {
            return self
                .fail_native_lineage_realization(
                    format!("Composer restoration host service failed: {error}"),
                    window,
                    cx,
                )
                .map(|()| true);
        }
        self.service_native_lineage_cleanup(window, cx)?;

        if self.native_lineage_disposal_active {
            return Ok(true);
        }
        if let Some(blocked_epoch) = self.native_lineage_capacity_blocked_epoch {
            if self.service.native_lineage_capacity_epoch() == blocked_epoch {
                return Ok(true);
            }
            self.native_lineage_capacity_blocked_epoch = None;
            self.native_lineage_failure = None;
            cx.notify();
        }

        let Some(control) = self.native_lineage_recovery.clone() else {
            return Ok(false);
        };
        let current_selection = self.service.selected_identity();
        if let Some(expected) = self.native_lineage_selection
            && current_selection != Some(expected)
        {
            self.finish_native_lineage_route_loss(window, cx)?;
            return Ok(false);
        }
        if self.native_lineage_snapshot.is_none() {
            let Some(selection) = current_selection else {
                return Ok(false);
            };
            let Some(snapshot) = control.snapshot_for_thread(selection.claim().thread_id()) else {
                return Ok(false);
            };
            self.native_lineage_selection = Some(selection);
            self.native_lineage_snapshot = Some(snapshot);
            self.suspend_autosave()?;
            let contribution = self.native_lineage_contribution(selection, cx)?;
            contribution.update(cx, |composer, composer_cx| {
                composer.begin_native_lineage_release_fence(window, composer_cx)
            })?;
            cx.notify();
        } else {
            let selection = self
                .native_lineage_selection
                .ok_or_else(|| "native lineage recovery selection is unavailable".to_owned())?;
            let Some(snapshot) = control.snapshot_for_thread(selection.claim().thread_id()) else {
                self.finish_native_lineage_route_loss(window, cx)?;
                return Ok(false);
            };
            let previous = self.native_lineage_snapshot;
            self.native_lineage_snapshot = Some(snapshot);
            if self.native_lineage_prompt_published
                && previous.map(NativeLineageRecoverySnapshot::status) != Some(snapshot.status())
                && matches!(
                    snapshot.status(),
                    NativeLineageRecoveryStatus::Ready { .. }
                        | NativeLineageRecoveryStatus::Failed { .. }
                )
            {
                self.native_lineage_retry_focus.focus(window);
            }
        }

        if self.native_lineage_seed.is_none() {
            self.finish_or_start_native_lineage_seed_validation(window, cx)?;
            return Ok(true);
        }
        if self.native_lineage_failure.is_some() {
            return Ok(true);
        }
        let snapshot = self
            .native_lineage_snapshot
            .ok_or_else(|| "native lineage recovery snapshot is unavailable".to_owned())?;
        if let NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues,
        } = snapshot.status()
        {
            self.finish_native_lineage_leaving(
                &control,
                snapshot,
                pending_turn_continues,
                window,
                cx,
            )?;
        } else if self.native_lineage_route_lost {
            self.drive_native_lineage_realization(None, window, cx)?;
        }
        Ok(true)
    }

    fn native_lineage_contribution(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        cx: &App,
    ) -> Result<Entity<MainWindowConversationComposer>, String> {
        self.contribution
            .as_ref()
            .filter(|contribution| contribution.read(cx).selection_identity() == selection)
            .cloned()
            .ok_or_else(|| "selected composer contribution is unavailable".to_owned())
    }

    pub(super) fn advance_native_lineage_fenced_selection(
        &mut self,
        previous: MainWindowComposerSelectionIdentity,
        current: MainWindowComposerSelectionIdentity,
    ) {
        if self.native_lineage_snapshot.is_none() {
            return;
        }
        if self.native_lineage_prompt_published
            || self.native_lineage_seed.is_some()
            || self.native_lineage_selection != Some(previous)
            || self.service.selected_identity() != Some(current)
        {
            return;
        }
        if !Self::native_lineage_successor_is_exact(previous, current) {
            return;
        }
        self.native_lineage_selection = Some(current);
    }

    pub(super) fn native_lineage_successor_is_exact(
        previous: MainWindowComposerSelectionIdentity,
        current: MainWindowComposerSelectionIdentity,
    ) -> bool {
        let previous_claim = previous.claim();
        let current_claim = current.claim();
        let previous_binding = previous.binding();
        let current_binding = current.binding();
        previous.window_id() == current.window_id()
            && previous_claim.thread_id() == current_claim.thread_id()
            && previous_claim.generation() == current_claim.generation()
            && previous_binding.home_id() == current_binding.home_id()
            && previous_binding.home_generation() == current_binding.home_generation()
            && previous_binding.host_generation() == current_binding.host_generation()
            && previous_binding.root() == current_binding.root()
    }

    fn finish_or_start_native_lineage_seed_validation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some((key, selection, seed, result)) = self.native_lineage_validation.take() {
            self.native_lineage_validation_task = None;
            let current = self
                .native_lineage_snapshot
                .ok_or_else(|| "native lineage recovery snapshot is unavailable".to_owned())?;
            if current.key() != key || self.native_lineage_selection != Some(selection) {
                return self.fail_native_lineage_mount(
                    "Recovery validation completed for an obsolete selection.".to_owned(),
                    window,
                    cx,
                );
            }
            if let Err(error) = result {
                return self.fail_native_lineage_mount(
                    format!("The composer recovery seed could not be validated: {error}"),
                    window,
                    cx,
                );
            }
            let config = (self.configurator)(selection)?;
            self.service
                .begin_native_lineage_suspension(selection, seed)?;
            let contribution = self.native_lineage_contribution(selection, cx)?;
            let release = contribution.update(cx, |composer, composer_cx| {
                composer.release_widget(window, composer_cx)
            })?;
            self.native_lineage_widget_release = Some(release);
            self.contribution_subscription = None;
            self.contribution = None;
            self.native_lineage_seed = Some(seed);
            self.native_lineage_config = Some(config);
            self.native_lineage_prompt_published = true;
            self.native_lineage_retry_focus.focus(window);
            cx.notify();
            return Ok(());
        }
        if self.native_lineage_validation_task.is_some() {
            return Ok(());
        }
        let selection = self
            .native_lineage_selection
            .ok_or_else(|| "native lineage recovery selection is unavailable".to_owned())?;
        let contribution = self.native_lineage_contribution(selection, cx)?;
        if !contribution.update(cx, |composer, composer_cx| {
            composer.native_lineage_release_ready(composer_cx)
        }) {
            return Ok(());
        }
        let seed = contribution.update(cx, |composer, composer_cx| {
            composer.export_native_lineage_restoration(composer_cx)
        })?;
        let key = self
            .native_lineage_snapshot
            .expect("active native lineage snapshot")
            .key();
        self.start_native_lineage_validation(key, selection, seed, window, cx);
        Ok(())
    }

    fn start_native_lineage_validation(
        &mut self,
        key: NativeLineageRecoveryKey,
        selection: MainWindowComposerSelectionIdentity,
        seed: gpui_text_input::RangeRestorationSeed,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let service = self.service.clone();
        let validation = cx
            .background_executor()
            .spawn(async move { service.validate_native_lineage_restoration(selection, seed) });
        self.native_lineage_validation_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = validation.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.native_lineage_validation = Some((key, selection, seed, result));
                let _ = this.refresh_native_lineage_recovery(window, cx);
                cx.notify();
            });
        }));
    }

    fn finish_native_lineage_leaving(
        &mut self,
        control: &NativeLineageRecoveryControl,
        snapshot: NativeLineageRecoverySnapshot,
        pending_turn_continues: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let selection = self
            .native_lineage_selection
            .ok_or_else(|| "native lineage recovery selection is unavailable".to_owned())?;
        if pending_turn_continues {
            let Some(focus) = self.native_lineage_pending_focus.clone() else {
                return Ok(());
            };
            self.cancel_native_lineage_realization();
            self.service
                .complete_native_lineage_restoration(selection)?;
            control.acknowledge_leaving(snapshot.key()).map_err(|_| {
                "native lineage recovery route changed before continuation".to_owned()
            })?;
            focus.focus(window);
            self.clear_native_lineage_mount_state();
            cx.notify();
            return Ok(());
        }
        match self.drive_native_lineage_realization(Some((control, snapshot.key())), window, cx) {
            Ok(()) => Ok(()),
            Err(error) => self.fail_native_lineage_realization(
                format!("Composer restoration could not start: {error}"),
                window,
                cx,
            ),
        }
    }

    fn finish_native_lineage_route_loss(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(selection) = self.native_lineage_selection else {
            self.clear_native_lineage_mount_state();
            return Ok(());
        };
        if !self.native_lineage_prompt_published {
            if self.service.selected_identity() == Some(selection)
                && let Ok(contribution) = self.native_lineage_contribution(selection, cx)
            {
                contribution.update(cx, |composer, composer_cx| {
                    composer.resume_after_widget_release_fence(window, composer_cx)
                })?;
                self.initialize_autosave(window, cx)?;
            }
            self.clear_native_lineage_mount_state();
            cx.notify();
            return Ok(());
        }
        if !self.native_lineage_route_lost {
            self.native_lineage_route_lost = true;
            self.cancel_native_lineage_realization();
        }
        match self.drive_native_lineage_realization(None, window, cx) {
            Ok(()) => Ok(()),
            Err(error) => self.fail_native_lineage_realization(
                format!("Composer restoration could not start: {error}"),
                window,
                cx,
            ),
        }
    }

    fn fail_native_lineage_mount(
        &mut self,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let (Some(control), Some(snapshot)) = (
            self.native_lineage_recovery.as_ref(),
            self.native_lineage_snapshot,
        ) {
            let _ = control.cancel(snapshot.key());
        }
        if !self.native_lineage_prompt_published {
            if let Some(selection) = self.native_lineage_selection
                && let Ok(contribution) = self.native_lineage_contribution(selection, cx)
            {
                contribution.update(cx, |composer, composer_cx| {
                    composer.resume_after_widget_release_fence(window, composer_cx)
                })?;
                self.initialize_autosave(window, cx)?;
            }
            self.clear_native_lineage_mount_state();
            cx.notify();
            return Ok(());
        }
        self.cancel_native_lineage_realization();
        self.native_lineage_failure = Some(message);
        cx.notify();
        Ok(())
    }

    fn fail_native_lineage_realization(
        &mut self,
        message: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let retirement_epoch = self.service.native_lineage_capacity_epoch();
        self.cancel_native_lineage_realization();
        self.native_lineage_capacity_blocked_epoch = Some(retirement_epoch);
        self.native_lineage_failure = Some(message);
        cx.notify();
        Ok(())
    }

    pub(super) fn cancel_native_lineage_for_lifecycle(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let preserve_prompt = self.native_lineage_prompt_published;
        if let (Some(control), Some(snapshot)) = (
            self.native_lineage_recovery.as_ref(),
            self.native_lineage_snapshot,
        ) {
            let _ = control.cancel(snapshot.key());
        }
        self.cancel_native_lineage_realization();
        if let Some(selection) = self.native_lineage_selection {
            self.service.cancel_native_lineage_suspension(selection)?;
            if !preserve_prompt
                && self.service.selected_identity() == Some(selection)
                && let Ok(contribution) = self.native_lineage_contribution(selection, cx)
            {
                let _ = contribution.update(cx, |composer, composer_cx| {
                    composer.resume_after_widget_release_fence(window, composer_cx)
                });
            }
        }
        if preserve_prompt {
            self.native_lineage_disposal_active = true;
            self.native_lineage_failure = None;
        } else {
            self.clear_native_lineage_mount_state();
        }
        Ok(())
    }

    pub(super) fn preserve_native_lineage_disposal_failure(
        &mut self,
        message: String,
        cx: &mut Context<Self>,
    ) {
        if self.native_lineage_prompt_published {
            self.native_lineage_disposal_active = true;
            self.native_lineage_failure = Some(message);
            cx.notify();
        }
    }

    pub(super) fn cancel_native_lineage_on_drop(&mut self) {
        if let (Some(control), Some(snapshot)) = (
            self.native_lineage_recovery.as_ref(),
            self.native_lineage_snapshot,
        ) {
            let _ = control.cancel(snapshot.key());
        }
        self.cancel_native_lineage_realization();
        if let Some(selection) = self.native_lineage_selection {
            let _ = self.service.cancel_native_lineage_suspension(selection);
        }
        self.native_lineage_validation_task = None;
        self.native_lineage_refresh_task = None;
    }

    fn cancel_native_lineage_realization(&mut self) {
        let source = self.native_lineage_source.take();
        if let Some(session) = self.native_lineage_session.as_mut() {
            session.cancel();
        }
        self.native_lineage_session = None;
        self.native_lineage_candidate = None;
        self.native_lineage_environment = None;
        self.native_lineage_effects.clear();
        self.native_lineage_host_result = None;
        self.native_lineage_cleanup = None;
        if let Some(source) = source {
            source.release_owner();
        }
    }

    pub(super) fn clear_native_lineage_mount_state(&mut self) {
        self.cancel_native_lineage_realization();
        self.native_lineage_snapshot = None;
        self.native_lineage_selection = None;
        self.native_lineage_seed = None;
        self.native_lineage_prompt_published = false;
        self.native_lineage_route_lost = false;
        self.native_lineage_config = None;
        self.native_lineage_validation = None;
        self.native_lineage_validation_task = None;
        self.native_lineage_failure = None;
        self.native_lineage_capacity_blocked_epoch = None;
        self.native_lineage_disposal_active = false;
    }
}

impl Render for MainWindowConversationComposerMount {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_native_lineage_refresh_task(window, cx);
        let _ = self.refresh_native_lineage_recovery(window, cx);
        div()
            .id(("main-window-user-input-contribution", cx.entity_id()))
            .w_full()
            .when(!self.native_lineage_prompt_published, |root| {
                root.children(self.contribution.clone())
            })
            .when(self.native_lineage_prompt_published, |root| {
                root.child(self.render_native_lineage_prompt(cx))
            })
    }
}
