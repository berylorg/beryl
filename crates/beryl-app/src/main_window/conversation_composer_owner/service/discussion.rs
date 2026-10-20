use super::*;
use crate::discussion_presentation::{
    DiscussionPresentationReader, DiscussionPresentationSnapshot,
};

#[derive(Default)]
pub(super) struct PreparedDiscussionPresentation {
    reader: Option<DiscussionPresentationReader>,
    snapshots: Vec<(
        MainWindowComposerSelectionIdentity,
        DiscussionPresentationSnapshot,
    )>,
}

impl MainWindowConversationComposerService {
    pub fn bind_discussion_presentation(
        &self,
        reader: DiscussionPresentationReader,
    ) -> Result<(), String> {
        if reader.home_id() != self.store.home_id()
            || self.store.health().generation() != Some(reader.home_generation())
        {
            return Err("discussion presentation reader belongs to another home".into());
        }
        let mut state = self
            .discussion
            .lock()
            .map_err(|_| "discussion presentation lock failed")?;
        state.reader = Some(reader);
        state.snapshots.clear();
        Ok(())
    }

    pub fn discussion_presentation_reader(&self) -> Option<DiscussionPresentationReader> {
        self.discussion.lock().ok()?.reader.clone()
    }

    pub fn prepare_discussion_presentation(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        let Some(reader) = self
            .discussion
            .lock()
            .map_err(|_| "discussion presentation lock failed")?
            .reader
            .clone()
        else {
            return Ok(());
        };
        let snapshot = reader
            .observe(selection.claim().thread_id())
            .map_err(|error| error.to_string())?;
        self.retain_discussion_presentation(selection, snapshot)
    }

    pub fn prepare_discussion_presentation_candidate(
        &self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        let Some(reader) = self
            .discussion
            .lock()
            .map_err(|_| "discussion presentation lock failed")?
            .reader
            .clone()
        else {
            return Ok(());
        };
        let snapshot = reader
            .observe_candidate(access, selection.claim().thread_id())
            .map_err(|error| error.to_string())?;
        self.retain_discussion_presentation(selection, snapshot)
    }

    pub(super) fn retain_discussion_presentation(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        snapshot: DiscussionPresentationSnapshot,
    ) -> Result<(), String> {
        if snapshot.home_id() != selection.binding().home_id()
            || snapshot.home_generation() != selection.binding().home_generation()
            || snapshot.thread_id() != selection.claim().thread_id()
        {
            return Err("discussion presentation belongs to another selection".into());
        }
        let mut state = self
            .discussion
            .lock()
            .map_err(|_| "discussion presentation lock failed")?;
        if state
            .reader
            .as_ref()
            .is_none_or(|reader| reader.service_generation() != snapshot.service_generation())
        {
            return Err("discussion presentation reader changed during preparation".into());
        }
        state
            .snapshots
            .retain(|(identity, _)| *identity != selection);
        if state.snapshots.len() == 2 {
            state.snapshots.remove(0);
        }
        state.snapshots.push((selection, snapshot));
        Ok(())
    }

    pub(in crate::main_window) fn install_recovered_discussion(
        &mut self,
        reader: Option<DiscussionPresentationReader>,
        snapshot: Option<DiscussionPresentationSnapshot>,
    ) {
        let selection = self
            .selected_identity()
            .expect("recovered selected identity");
        let state = self.discussion.get_mut().expect("new presentation state");
        state.reader = reader;
        state.snapshots = snapshot
            .map(|snapshot| vec![(selection, snapshot)])
            .unwrap_or_default();
    }

    pub(in crate::main_window) fn prepared_discussion_presentation(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Option<DiscussionPresentationSnapshot> {
        self.discussion
            .lock()
            .ok()?
            .snapshots
            .iter()
            .find(|(identity, _)| *identity == selection)
            .map(|(_, snapshot)| snapshot.clone())
    }
}
