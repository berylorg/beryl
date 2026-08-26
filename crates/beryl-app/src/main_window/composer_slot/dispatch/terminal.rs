use beryl_home_store::{CommandCancellation, HomeStore};
use gpui_text_input::{
    MutationCommitRequest, MutationIdentity, MutationKey, MutationLane, MutationPageAcceptance,
    MutationStreamFinish, MutationTotals,
};

use crate::composer_host::{ComposerHostMutationOutcome, SyndicComposerHost};

use super::{MainWindowComposerDispatchError, MainWindowComposerDispatcher};

#[derive(Clone)]
pub(super) struct MainWindowComposerEarlyTerminal {
    pub(super) key: MutationKey,
    pub(super) outcome: ComposerHostMutationOutcome,
    source: SyntheticMutationLane,
    proposal: SyntheticMutationLane,
}

#[derive(Clone, Copy)]
struct SyntheticMutationLane {
    next_cursor: gpui_text_input::MutationCursor,
    next_ordinal: u64,
    cumulative_identity: MutationIdentity,
    totals: MutationTotals,
}

pub(super) fn capture_early_terminal(
    host: &mut SyndicComposerHost,
    dispatcher: &mut MainWindowComposerDispatcher,
    store: &HomeStore,
    key: MutationKey,
) -> Result<(), MainWindowComposerDispatchError> {
    let (begin_key, source_cursor, proposal_cursor) = dispatcher
        .mutation_begin
        .filter(|(begin_key, _, _)| *begin_key == key)
        .ok_or(MainWindowComposerDispatchError::Malformed)?;
    let outcome = host.execute_mutation(
        store,
        MutationCommitRequest::new(key, MutationIdentity::ROOT),
        &CommandCancellation::new(),
    )?;
    dispatcher.early_terminal = Some(MainWindowComposerEarlyTerminal {
        key: begin_key,
        outcome,
        source: SyntheticMutationLane::new(source_cursor),
        proposal: SyntheticMutationLane::new(proposal_cursor),
    });
    Ok(())
}

pub(super) fn accept_early_terminal_page(
    dispatcher: &mut MainWindowComposerDispatcher,
    page: &gpui_text_input::MutationPage,
) -> Result<MutationPageAcceptance, MainWindowComposerDispatchError> {
    let terminal = dispatcher
        .early_terminal
        .as_mut()
        .ok_or(MainWindowComposerDispatchError::Malformed)?;
    if terminal.key != page.key().key() {
        return Err(MainWindowComposerDispatchError::Malformed);
    }
    let lane = match page.key().lane() {
        MutationLane::Source => &mut terminal.source,
        MutationLane::Proposal => &mut terminal.proposal,
    };
    lane.accept(page)
}

pub(super) fn validate_early_terminal_finish(
    dispatcher: &MainWindowComposerDispatcher,
    finish: gpui_text_input::MutationFinishInput,
) -> Result<(), MainWindowComposerDispatchError> {
    let terminal = dispatcher
        .early_terminal
        .as_ref()
        .ok_or(MainWindowComposerDispatchError::Malformed)?;
    if terminal.key != finish.key()
        || !terminal.source.matches(finish.source())
        || !terminal.proposal.matches(finish.proposal())
    {
        return Err(MainWindowComposerDispatchError::Malformed);
    }
    Ok(())
}

impl SyntheticMutationLane {
    const fn new(next_cursor: gpui_text_input::MutationCursor) -> Self {
        Self {
            next_cursor,
            next_ordinal: 0,
            cumulative_identity: MutationIdentity::ROOT,
            totals: MutationTotals {
                pages: 0,
                items: 0,
                retained_bytes: 0,
                inserted_bytes: 0,
                inserted_line_breaks: 0,
                objects: 0,
                object_bytes: 0,
                presentation_bytes: 0,
            },
        }
    }

    fn accept(
        &mut self,
        page: &gpui_text_input::MutationPage,
    ) -> Result<MutationPageAcceptance, MainWindowComposerDispatchError> {
        let key = page.key();
        if key.cursor() != self.next_cursor
            || key.ordinal() != self.next_ordinal
            || key.prior() != self.cumulative_identity
        {
            return Err(MainWindowComposerDispatchError::Malformed);
        }
        self.next_cursor = page.next_cursor();
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or(MainWindowComposerDispatchError::Malformed)?;
        self.cumulative_identity = page.cumulative_identity();
        self.totals = add_totals(self.totals, page.totals())?;
        Ok(MutationPageAcceptance::Accepted {
            next_cursor: self.next_cursor,
            next_ordinal: self.next_ordinal,
            cumulative_identity: self.cumulative_identity,
            totals: self.totals,
        })
    }

    fn matches(self, finish: MutationStreamFinish) -> bool {
        self.next_cursor == finish.next_cursor
            && self.next_ordinal == finish.next_ordinal
            && self.cumulative_identity == finish.cumulative_identity
            && self.totals == finish.totals
    }
}

fn add_totals(
    left: MutationTotals,
    right: MutationTotals,
) -> Result<MutationTotals, MainWindowComposerDispatchError> {
    Ok(MutationTotals {
        pages: left
            .pages
            .checked_add(right.pages)
            .ok_or(MainWindowComposerDispatchError::Malformed)?,
        items: left
            .items
            .checked_add(right.items)
            .ok_or(MainWindowComposerDispatchError::Malformed)?,
        retained_bytes: left
            .retained_bytes
            .checked_add(right.retained_bytes)
            .ok_or(MainWindowComposerDispatchError::Malformed)?,
        inserted_bytes: left
            .inserted_bytes
            .checked_add(right.inserted_bytes)
            .ok_or(MainWindowComposerDispatchError::Malformed)?,
        inserted_line_breaks: left
            .inserted_line_breaks
            .checked_add(right.inserted_line_breaks)
            .ok_or(MainWindowComposerDispatchError::Malformed)?,
        objects: left
            .objects
            .checked_add(right.objects)
            .ok_or(MainWindowComposerDispatchError::Malformed)?,
        object_bytes: left
            .object_bytes
            .checked_add(right.object_bytes)
            .ok_or(MainWindowComposerDispatchError::Malformed)?,
        presentation_bytes: left
            .presentation_bytes
            .checked_add(right.presentation_bytes)
            .ok_or(MainWindowComposerDispatchError::Malformed)?,
    })
}

pub(super) fn finish_for(
    dispatcher: &MainWindowComposerDispatcher,
    key: MutationKey,
) -> Result<MutationIdentity, MainWindowComposerDispatchError> {
    dispatcher
        .mutation_finish
        .filter(|(retained_key, _)| *retained_key == key)
        .map(|(_, finish)| finish)
        .ok_or(MainWindowComposerDispatchError::Malformed)
}
