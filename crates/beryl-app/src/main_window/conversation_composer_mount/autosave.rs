use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use beryl_home_store::CommandCancellation;
use beryl_model::AssetReferenceSetId;
use beryl_state::AssetReferenceSetStagingAuthority;
use gpui::{Context, Window};
use syndic_storage::{DraftMarkerSealOperationIdV1, DraftPieceOperationIdV1, SyndicTimestamp};

use super::super::{
    MainWindowComposerAutosaveCaptureRequirement, MainWindowComposerSelectionIdentity,
    MainWindowConversationComposerMount,
};
use crate::composer_host::{
    ComposerHostAutosaveAdvance, ComposerHostAutosaveCapture, ComposerHostAutosaveInterval,
    ComposerHostAutosaveSettingsCompletion, ComposerHostAutosaveTimer,
    ComposerHostMarkerSealAuthority, ComposerHostPublicationTicket,
};

mod model;

pub(super) use model::{AutosaveState, MainWindowConversationComposerAutosave};
pub use model::{
    MainWindowConversationComposerAutosaveDiagnostics, MainWindowConversationComposerAutosavePhase,
};

impl MainWindowConversationComposerMount {
    pub fn autosave_diagnostics(&self) -> MainWindowConversationComposerAutosaveDiagnostics {
        self.autosave.diagnostics()
    }

    pub fn publish_autosave_interval(
        &mut self,
        settings_generation: u64,
        interval: ComposerHostAutosaveInterval,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<ComposerHostAutosaveSettingsCompletion, String> {
        let selection = self
            .service
            .selected_identity()
            .ok_or_else(|| "conversation composer autosave has no selected slot".to_owned())?;
        let completion =
            self.service
                .publish_autosave_interval(selection, settings_generation, interval)?;
        if matches!(
            completion,
            ComposerHostAutosaveSettingsCompletion::Published(_)
        ) {
            self.autosave.settings = Some((settings_generation, interval));
        }
        self.refresh_autosave(window, cx)?;
        Ok(completion)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_hold_next_autosave_ready(&mut self) {
        assert!(!self.autosave.hold_ready_once);
        self.autosave.hold_ready_once = true;
    }

    pub(super) fn initialize_autosave(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some((settings_generation, interval)) = self.autosave.settings
            && let Some(selection) = self.service.selected_identity()
        {
            let _ =
                self.service
                    .publish_autosave_interval(selection, settings_generation, interval)?;
        }
        self.refresh_autosave(window, cx)
    }

    pub(super) fn suspend_autosave(&mut self) -> Result<(), String> {
        self.autosave.suspend()
    }

    pub(super) fn autosave_selection_advanced(
        &mut self,
        previous: MainWindowComposerSelectionIdentity,
        current: MainWindowComposerSelectionIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.service.selected_identity() != Some(current) {
            return Ok(());
        }
        if self.autosave.selection_advanced(previous, current) {
            return Ok(());
        }
        self.refresh_autosave(window, cx)
    }

    pub(super) fn refresh_autosave(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.autosave.suspend()?;
        self.autosave.fenced = false;
        let Some(selection) = self.service.selected_identity() else {
            return Ok(());
        };
        if let Some(ticket) = self.service.selected_autosave_publication(selection)? {
            self.autosave.state = AutosaveState::Publishing { selection, ticket };
            return self.schedule_autosave_advance(window, cx);
        }
        let Some(timer) = self.service.selected_autosave_timer(selection)? else {
            return Ok(());
        };
        self.schedule_autosave_timer(selection, timer, window, cx)
    }

    fn schedule_autosave_timer(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        timer: ComposerHostAutosaveTimer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let generation = self.autosave.advance_generation()?;
        self.autosave.state = AutosaveState::Waiting { selection, timer };
        let delay = timer
            .deadline()
            .checked_duration_since(Instant::now())
            .unwrap_or(Duration::ZERO);
        let sleeper = cx.background_executor().timer(delay);
        self.autosave.task = Some(cx.spawn_in(window, async move |this, cx| {
            sleeper.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Err(error) = this.fire_autosave(generation, timer, window, cx) {
                    this.autosave.last_error = Some(error);
                }
            });
        }));
        Ok(())
    }

    fn fire_autosave(
        &mut self,
        generation: u64,
        timer: ComposerHostAutosaveTimer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.autosave.generation != generation {
            return Ok(());
        }
        let selection = match self.autosave.state {
            AutosaveState::Waiting {
                selection,
                timer: expected,
            } if expected == timer => selection,
            _ => return Ok(()),
        };
        let requirement = self.service.autosave_capture_requirement(selection)?;
        if requirement == MainWindowComposerAutosaveCaptureRequirement::Clean {
            return self.refresh_autosave(window, cx);
        }
        let operation_id = fresh_piece_operation_id()?;
        let marker_authority = match requirement {
            MainWindowComposerAutosaveCaptureRequirement::ChangedMarkers => {
                Some(fresh_marker_authority()?)
            }
            MainWindowComposerAutosaveCaptureRequirement::UnchangedMarkers => None,
            MainWindowComposerAutosaveCaptureRequirement::Clean => unreachable!(),
        };
        let published_at = current_timestamp()?;
        self.autosave.last_error = None;
        let service = self.service.clone();
        let assets = self.autosave.assets.clone();
        let marker_seals = self.autosave.marker_seals.clone();
        let cancellation = CommandCancellation::new();
        let task = cx.background_executor().spawn(async move {
            service.fire_autosave(
                selection,
                timer,
                assets,
                &marker_seals,
                operation_id,
                marker_authority,
                published_at,
                &cancellation,
            )
        });
        self.autosave.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Err(error) =
                    this.finish_autosave_capture(generation, timer, result, window, cx)
                {
                    this.autosave.last_error = Some(error);
                }
            });
        }));
        Ok(())
    }

    fn finish_autosave_capture(
        &mut self,
        generation: u64,
        timer: ComposerHostAutosaveTimer,
        result: Result<ComposerHostAutosaveCapture, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.autosave.generation != generation
            || !matches!(
                self.autosave.state,
                AutosaveState::Waiting {
                    timer: expected,
                    ..
                } if expected == timer
            )
        {
            return Ok(());
        }
        match result {
            Ok(ComposerHostAutosaveCapture::Captured(ticket)) => {
                let selection = self.service.selected_identity().ok_or_else(|| {
                    "conversation composer selection disappeared after autosave capture".to_owned()
                })?;
                self.autosave.state = AutosaveState::Publishing { selection, ticket };
                self.schedule_autosave_advance(window, cx)
            }
            Ok(ComposerHostAutosaveCapture::PublicationPending) => {
                self.refresh_autosave(window, cx)
            }
            Ok(ComposerHostAutosaveCapture::Stale)
            | Ok(ComposerHostAutosaveCapture::Clean)
            | Ok(ComposerHostAutosaveCapture::Cancelled) => self.refresh_autosave(window, cx),
            Err(error) => {
                self.autosave.last_error = Some(error);
                self.refresh_autosave(window, cx)
            }
        }
    }

    fn schedule_autosave_advance(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (selection, ticket) = match self.autosave.state {
            AutosaveState::Publishing { selection, ticket } => (selection, ticket),
            AutosaveState::Ready { selection, ticket } => {
                return self.drive_autosave_ready(selection, ticket, window, cx);
            }
            _ => return Ok(()),
        };
        if !self.autosave.fenced && self.service.autosave_publication_ready(selection, ticket)? {
            self.autosave.state = AutosaveState::Ready { selection, ticket };
            return self.drive_autosave_ready(selection, ticket, window, cx);
        }
        let generation = self.autosave.generation;
        let service = self.service.clone();
        let task = cx
            .background_executor()
            .spawn(async move { service.advance_autosave(selection, ticket) });
        self.autosave.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Err(error) =
                    this.finish_autosave_advance(generation, ticket, result, window, cx)
                {
                    this.autosave.last_error = Some(error);
                }
            });
        }));
        Ok(())
    }

    fn finish_autosave_advance(
        &mut self,
        generation: u64,
        ticket: ComposerHostPublicationTicket,
        result: Result<ComposerHostAutosaveAdvance, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.autosave.generation != generation
            || !matches!(
                self.autosave.state,
                AutosaveState::Publishing { ticket: expected, .. }
                    | AutosaveState::Ready { ticket: expected, .. }
                    if expected == ticket
            )
        {
            return Ok(());
        }
        let current = self.service.selected_identity();
        match result {
            Ok(ComposerHostAutosaveAdvance::Progress)
            | Ok(ComposerHostAutosaveAdvance::ReconciliationPending) => {
                let selection = current.ok_or_else(|| {
                    "conversation composer selection disappeared during autosave".to_owned()
                })?;
                self.autosave.state = AutosaveState::Publishing { selection, ticket };
                self.defer_autosave_advance(generation, window, cx);
                Ok(())
            }
            Ok(ComposerHostAutosaveAdvance::Ready) => {
                let selection = current.ok_or_else(|| {
                    "conversation composer selection disappeared before autosave commit".to_owned()
                })?;
                self.autosave.state = AutosaveState::Ready { selection, ticket };
                self.drive_autosave_ready(selection, ticket, window, cx)
            }
            Ok(ComposerHostAutosaveAdvance::Saved { .. }) => {
                self.autosave.last_error = None;
                self.synchronize_contribution_selection(cx)?;
                self.resume_autosave_fence(window, cx)?;
                self.refresh_autosave(window, cx)
            }
            Ok(ComposerHostAutosaveAdvance::Unsatisfied(_)) => {
                self.autosave.last_error = None;
                self.synchronize_contribution_selection(cx)?;
                self.resume_autosave_fence(window, cx)?;
                self.refresh_autosave(window, cx)
            }
            Ok(ComposerHostAutosaveAdvance::Stale) => {
                let Some(selection) = current else {
                    self.resume_autosave_fence(window, cx)?;
                    return self.refresh_autosave(window, cx);
                };
                if self.service.selected_autosave_publication(selection)? == Some(ticket) {
                    self.autosave.state = if self.autosave.fenced {
                        AutosaveState::Ready { selection, ticket }
                    } else {
                        AutosaveState::Publishing { selection, ticket }
                    };
                    self.defer_autosave_advance(generation, window, cx);
                    Ok(())
                } else {
                    self.resume_autosave_fence(window, cx)?;
                    self.refresh_autosave(window, cx)
                }
            }
            Err(error) => {
                let already_retried = self.autosave.last_error.is_some();
                self.autosave.last_error = Some(error);
                let Some(selection) = current else {
                    self.resume_autosave_fence(window, cx)?;
                    return self.refresh_autosave(window, cx);
                };
                if self.service.selected_autosave_publication(selection)? != Some(ticket) {
                    self.resume_autosave_fence(window, cx)?;
                    return self.refresh_autosave(window, cx);
                }
                if already_retried {
                    self.autosave.state = AutosaveState::Publishing { selection, ticket };
                    self.autosave.task = None;
                    return self.resume_autosave_fence(window, cx);
                }
                self.autosave.state = if self.autosave.fenced {
                    AutosaveState::Ready { selection, ticket }
                } else {
                    AutosaveState::Publishing { selection, ticket }
                };
                self.defer_autosave_advance(generation, window, cx);
                Ok(())
            }
        }
    }

    fn drive_autosave_ready(
        &mut self,
        _selection: MainWindowComposerSelectionIdentity,
        ticket: ComposerHostPublicationTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let selection = match self.autosave.state {
            AutosaveState::Ready {
                selection,
                ticket: expected,
            } if expected == ticket => selection,
            _ => return Ok(()),
        };
        #[cfg(feature = "test-faults")]
        if std::mem::take(&mut self.autosave.hold_ready_once) {
            self.autosave.task = None;
            return Ok(());
        }
        if !self.autosave.fenced {
            let contribution = self
                .contribution
                .as_ref()
                .filter(|contribution| contribution.read(cx).selection_identity() == selection)
                .cloned()
                .ok_or_else(|| {
                    "composer autosave contribution does not match the ready selection".to_owned()
                })?;
            if !contribution.update(cx, |composer, composer_cx| {
                composer.begin_widget_release_fence(window, composer_cx)
            })? {
                let generation = self.autosave.generation;
                cx.defer_in(window, move |this, window, cx| {
                    if this.autosave.generation == generation
                        && let Err(error) = this.drive_autosave_ready(selection, ticket, window, cx)
                    {
                        this.autosave.last_error = Some(error);
                    }
                });
                return Ok(());
            }
            self.autosave.fenced = true;
        }
        self.autosave.state = AutosaveState::Publishing { selection, ticket };
        self.schedule_autosave_advance(window, cx)
    }

    fn defer_autosave_advance(&mut self, generation: u64, window: &Window, cx: &mut Context<Self>) {
        cx.defer_in(window, move |this, window, cx| {
            if this.autosave.generation == generation
                && let Err(error) = this.schedule_autosave_advance(window, cx)
            {
                this.autosave.last_error = Some(error);
            }
        });
    }

    fn resume_autosave_fence(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.autosave.fenced {
            return Ok(());
        }
        self.resume_contribution(window, cx)?;
        self.autosave.fenced = false;
        Ok(())
    }
}

fn fresh_piece_operation_id() -> Result<DraftPieceOperationIdV1, String> {
    Ok(DraftPieceOperationIdV1::from_bytes(fresh_bytes()?))
}

fn fresh_marker_authority() -> Result<ComposerHostMarkerSealAuthority, String> {
    Ok(ComposerHostMarkerSealAuthority::new(
        DraftMarkerSealOperationIdV1::from_bytes(fresh_bytes()?),
        AssetReferenceSetStagingAuthority::new(
            AssetReferenceSetId::from_bytes(fresh_bytes()?),
            fresh_bytes()?,
        ),
    ))
}

fn fresh_bytes<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes)
        .map_err(|_| "conversation composer autosave identity generation failed".to_owned())?;
    Ok(bytes)
}

fn current_timestamp() -> Result<SyndicTimestamp, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "conversation composer autosave clock precedes the Unix epoch".to_owned())?
        .as_millis()
        .try_into()
        .map_err(|_| "conversation composer autosave timestamp overflowed".to_owned())?;
    Ok(SyndicTimestamp::from_unix_millis(millis))
}
