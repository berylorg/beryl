use super::*;
use crate::discussion_presentation::{
    DiscussionPresentationReader, DiscussionPresentationSnapshot, DiscussionRetryDescriptor,
    DiscussionStatus,
};
use crate::main_window::{MainWindowComposerSelectionIdentity, MainWindowConversationComposer};
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, InteractiveElement, StatefulInteractiveElement};

mod render;
pub(super) use render::HEIGHT;
pub(super) use render::render;

#[derive(Default)]
pub(super) struct DiscussionStatusControls {
    pending: Option<RetryFlight>,
    #[cfg(all(test, feature = "test-faults"))]
    refresh_suspended: bool,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) first_frame: Option<DiscussionPresentationSnapshot>,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) recovered_frame: Option<(
        DiscussionPresentationSnapshot,
        bool,
        bool,
        gpui::EntityId,
        bool,
    )>,
}

struct RetryFlight {
    owner: gpui::EntityId,
    selection: MainWindowComposerSelectionIdentity,
    descriptor: DiscussionRetryDescriptor,
    epoch: Arc<()>,
    completed: bool,
    cancellation: beryl_home_store::CommandCancellation,
}

impl Drop for RetryFlight {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

impl RetryFlight {
    fn matches(
        &self,
        owner: gpui::EntityId,
        selection: MainWindowComposerSelectionIdentity,
        epoch: &Arc<()>,
        descriptor: &DiscussionRetryDescriptor,
    ) -> bool {
        self.owner == owner
            && self.selection == selection
            && Arc::ptr_eq(&self.epoch, epoch)
            && self.descriptor == *descriptor
    }
}

pub(crate) type ObservationTarget = (
    Entity<MainWindowConversationComposer>,
    MainWindowComposerSelectionIdentity,
    Arc<()>,
    DiscussionPresentationReader,
);

impl MainWindowShellRoot {
    pub(super) fn sync_discussion_demand(&mut self, cx: &App) {
        if let Some(flight) = &self.discussion_status.pending {
            let current = self.discussion_composer(cx).is_some_and(|composer| {
                composer.entity_id() == flight.owner
                    && composer.read(cx).selection_identity() == flight.selection
                    && Arc::ptr_eq(
                        &composer.read(cx).discussion_publication_epoch(),
                        &flight.epoch,
                    )
            });
            if !current {
                self.discussion_status.pending.take();
            }
        }
    }
    pub(in crate::main_window::shell) fn discussion_composer(
        &self,
        cx: &App,
    ) -> Option<Entity<MainWindowConversationComposer>> {
        self.controller
            .as_ref()?
            .composer_mount
            .as_ref()?
            .read(cx)
            .contribution()
    }

    pub(in crate::main_window::shell) fn selected_discussion_snapshot(
        &self,
        cx: &App,
    ) -> Option<DiscussionPresentationSnapshot> {
        let controller = self.controller.as_ref()?;
        let mount = controller.composer_mount.as_ref()?.read(cx);
        let composer = mount.contribution()?;
        let owner = composer.read(cx);
        let selection = owner.selection_identity();
        let matches = match &controller.content {
            ShellContent::Acquired {
                selection: expected,
                ..
            }
            | ShellContent::Restored {
                selection: expected,
                ..
            }
            | ShellContent::Selected {
                selection: expected,
                ..
            } => {
                selection.window_id() == expected.window_id()
                    && selection.claim() == expected.claim()
            }
            ShellContent::Retired {
                window_id,
                threadless: false,
                ..
            } => selection.window_id() == *window_id,
            _ => false,
        };
        if !matches {
            return None;
        }
        owner.discussion_presentation().cloned()
    }

    pub(super) fn discussion_observation_target(&self, cx: &App) -> Option<ObservationTarget> {
        #[cfg(all(test, feature = "test-faults"))]
        if self.discussion_status.refresh_suspended {
            return None;
        }
        let composer = self.discussion_composer(cx)?;
        let owner = composer.read(cx);
        let selection = owner.selection_identity();
        if self.coherent_viewed_thread(cx) != Some(selection.claim().thread_id())
            && !owner.discussion_recovery_refresh_allowed()
        {
            return None;
        }
        let reader = owner.discussion_presentation_reader()?;
        if !reader.current() {
            return None;
        }
        Some((
            composer.clone(),
            selection,
            owner.discussion_publication_epoch(),
            reader,
        ))
    }

    pub(super) fn apply_discussion_observation(
        &mut self,
        target: ObservationTarget,
        result: Result<
            DiscussionPresentationSnapshot,
            crate::discussion_presentation::DiscussionPresentationError,
        >,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (composer, selection, epoch, reader) = target;
        if (self.coherent_viewed_thread(cx) != Some(selection.claim().thread_id())
            && !composer.read(cx).discussion_recovery_refresh_allowed())
            || self
                .discussion_composer(cx)
                .is_none_or(|current| current != composer)
        {
            return;
        }
        let owner = composer.read(cx);
        if owner.selection_identity() != selection
            || !Arc::ptr_eq(&owner.discussion_publication_epoch(), &epoch)
        {
            return;
        }
        if !reader.current() {
            composer.update(cx, |composer, cx| {
                composer.invalidate_discussion_presentation(cx)
            });
            cx.notify();
            return;
        }
        if let Ok(snapshot) = result {
            let election = snapshot.clone();
            let retire_pending = !snapshot.retry_pending()
                && self
                    .discussion_status
                    .pending
                    .as_ref()
                    .is_some_and(|flight| {
                        flight.completed
                            && flight.owner == composer.entity_id()
                            && flight.selection == selection
                            && Arc::ptr_eq(&flight.epoch, &epoch)
                    });
            let accepted = election.try_elect(|| {
                composer.update(cx, |composer, cx| {
                    composer.apply_discussion_presentation(snapshot, cx)
                })
            });
            if matches!(accepted, Ok(true)) && retire_pending {
                self.discussion_status.pending = None;
            }
            if !matches!(accepted, Ok(true)) {
                composer.update(cx, |composer, cx| {
                    composer.invalidate_discussion_presentation(cx)
                });
            }
            self.sync_notices(window, cx);
            cx.notify();
        } else {
            composer.update(cx, |composer, cx| {
                composer.invalidate_discussion_presentation(cx)
            });
            cx.notify();
        }
    }

    fn retry_discussion_handoff(
        &mut self,
        expected: gpui::EntityId,
        selection: MainWindowComposerSelectionIdentity,
        epoch: Arc<()>,
        descriptor: DiscussionRetryDescriptor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.discussion_status.pending.is_some() || self.status_mutation_gate().is_some() {
            return;
        }
        let Some(composer) = self.discussion_composer(cx) else {
            return;
        };
        let owner = composer.read(cx);
        if composer.entity_id() != expected
            || owner.selection_identity() != selection
            || !Arc::ptr_eq(&owner.discussion_publication_epoch(), &epoch)
            || !owner.discussion_presentation_fresh()
            || self.coherent_viewed_thread(cx) != Some(selection.claim().thread_id())
            || owner.discussion_presentation().is_none_or(|snapshot| {
                snapshot.retry_pending()
                    || snapshot.retry() != Some(&descriptor)
                    || !snapshot.current()
            })
        {
            return;
        }
        let Some(reader) = owner
            .discussion_presentation_reader()
            .filter(|reader| reader.current())
        else {
            return;
        };
        let cancellation = beryl_home_store::CommandCancellation::new();
        self.discussion_status.pending = Some(RetryFlight {
            owner: expected,
            selection,
            epoch: epoch.clone(),
            descriptor: descriptor.clone(),
            completed: false,
            cancellation: cancellation.clone(),
        });
        self.clear_discussion_retry_failure();
        let background = cx.background_executor().clone();
        let completion_reader = reader.clone();
        let completion_descriptor = descriptor.clone();
        cx.spawn_in(window, async move |this, cx| {
            let completion = background
                .spawn(async move {
                    let completion = match reader
                        .prepare_retry(&descriptor, cancellation)
                    {
                        Ok(prepared) => {
                            let outcome = prepared.execute();
                            Ok(outcome.observation())
                        }
                        Err(error) => Err(error.to_string()),
                    };
                    (completion, reader.observe(selection.claim().thread_id()))
                })
                .await;
            let _ = this.update_in(cx, |root, window, cx| {
                let current = root
                    .discussion_status
                    .pending
                    .as_ref()
                    .is_some_and(|flight| {
                        flight.matches(expected, selection, &epoch, &completion_descriptor)
                    });
                if !current {
                    return;
                }
                if root.discussion_composer(cx).is_none_or(|current| current.entity_id() != expected || current.read(cx).selection_identity() != selection || !Arc::ptr_eq(&current.read(cx).discussion_publication_epoch(), &epoch)) { root.discussion_status.pending = None; cx.notify(); return; }
                root.discussion_status.pending.as_mut().unwrap().completed = true;
                match completion.0 {
                    Err(detail) => root.retain_discussion_retry_failure(expected, selection, detail, cx),
                    Ok(crate::discussion_presentation::DiscussionRetryObservation::NotCommitted) => root.retain_discussion_retry_failure(expected, selection, "The exact handoff retry did not commit. The accepted resolution and parent binding are unchanged.".into(), cx),
                    _ => {}
                }
                root.apply_discussion_observation(
                    (composer, selection, epoch, completion_reader),
                    completion.1,
                    window,
                    cx,
                );
                root.sync_notices(window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(all(test, feature = "test-faults"))]
impl MainWindowShellRoot {
    pub(crate) fn test_retry_current_discussion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.test_discussion_observation_target(cx) else {
            return;
        };
        self.test_retry_discussion_target(target, window, cx);
    }
    pub(crate) fn test_retry_discussion_target(
        &mut self,
        target: ObservationTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (composer, selection, epoch, _) = target;
        let descriptor = composer
            .read(cx)
            .discussion_presentation()
            .and_then(|snapshot| snapshot.retry())
            .cloned();
        if let Some(descriptor) = descriptor {
            self.retry_discussion_handoff(
                composer.entity_id(),
                selection,
                epoch,
                descriptor,
                window,
                cx,
            );
        }
    }
    pub(crate) fn test_discussion_retry_pending(&self) -> bool {
        self.discussion_status.pending.is_some()
    }
    pub(crate) fn test_suspend_discussion_refresh(&mut self, suspended: bool) {
        self.discussion_status.refresh_suspended = suspended;
    }
    pub(crate) fn test_discussion_observation_target(&self, cx: &App) -> Option<ObservationTarget> {
        let composer = self.discussion_composer(cx)?;
        let owner = composer.read(cx);
        Some((
            composer.clone(),
            owner.selection_identity(),
            owner.discussion_publication_epoch(),
            owner.discussion_presentation_reader()?,
        ))
    }
    pub(crate) fn test_apply_discussion_observation(
        &mut self,
        target: ObservationTarget,
        snapshot: Result<
            DiscussionPresentationSnapshot,
            crate::discussion_presentation::DiscussionPresentationError,
        >,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_discussion_observation(target, snapshot, window, cx);
    }
    pub(crate) fn test_discussion_presentation(
        &self,
        cx: &App,
    ) -> Option<(
        DiscussionPresentationSnapshot,
        bool,
        bool,
        gpui::EntityId,
        bool,
    )> {
        let composer = self.discussion_composer(cx)?;
        let owner = composer.read(cx);
        Some((
            owner.discussion_presentation()?.clone(),
            owner.mutation_gated(),
            owner.gpui_input().read(cx).is_enabled(),
            composer.entity_id(),
            owner.discussion_presentation_fresh(),
        ))
    }
    pub(crate) fn test_initial_discussion_presentation(
        &self,
    ) -> Option<DiscussionPresentationSnapshot> {
        self.discussion_status.first_frame.clone()
    }
    pub(crate) fn test_recovered_discussion_presentation(
        &self,
    ) -> Option<(
        DiscussionPresentationSnapshot,
        bool,
        bool,
        gpui::EntityId,
        bool,
    )> {
        self.discussion_status.recovered_frame.clone()
    }
}
