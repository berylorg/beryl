#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

use beryl_backend::{TurnError, TurnInfo, TurnItemsView, TurnStatus};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptTurnSkeleton {
    pub(crate) id: String,
    pub(crate) status: TurnStatus,
    pub(crate) items_view: TurnItemsView,
    pub(crate) error: Option<TurnError>,
    history_page_cursor: Option<String>,
    history_page_index: usize,
    history_page_len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptTurnDetailPageLocator {
    cursor: Option<String>,
    limit: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum TranscriptTurnDetailPinKind {
    ActiveContextMenu,
    EditTarget,
    MediaActionTarget,
    ActiveTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptTurnDetailStatus {
    Missing,
    Loading,
    Full,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptTurnDetailLoadTicket {
    thread_id: String,
    turn_id: String,
    generation: u64,
    request_id: u64,
    page_locator: Option<TranscriptTurnDetailPageLocator>,
    coalesced_turn_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptTurnDetailLoadStart {
    Started(TranscriptTurnDetailLoadTicket),
    AlreadyLoading(TranscriptTurnDetailLoadTicket),
    AlreadyFull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptTurnDetailApplyResult {
    Applied,
    Stale,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptTurnDetailReleaseCounts {
    pub(crate) full_detail_turns: usize,
    pub(crate) loading_detail_turns: usize,
    pub(crate) failed_detail_turns: usize,
    pub(crate) retained_item_count: usize,
    pub(crate) released_turn_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptTurnDetailRetainedCounts {
    pub(crate) skeleton_turns: usize,
    pub(crate) missing_detail_turns: usize,
    pub(crate) loading_detail_turns: usize,
    pub(crate) full_detail_turns: usize,
    pub(crate) failed_detail_turns: usize,
    pub(crate) pinned_turns: usize,
    pub(crate) retained_item_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptTurnDetailSchedule {
    pub(crate) retained_turns: usize,
    pub(crate) requested_tickets: Vec<TranscriptTurnDetailLoadTicket>,
    pub(crate) released: TranscriptTurnDetailReleaseCounts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptTurnDetailViewportOrder {
    OldestFirst,
    NewestFirst,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptTurnDetailViewportPlan {
    pub(crate) retained_turn_ids: Vec<String>,
    pub(crate) priority_turn_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptTurnDetailRetention {
    turn_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptTurnDetailCache {
    thread_id: Option<String>,
    generation: u64,
    next_request_id: u64,
    entries: BTreeMap<String, TranscriptTurnDetailEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptTurnDetailEntry {
    skeleton: Option<TranscriptTurnSkeleton>,
    state: TranscriptTurnDetailEntryState,
    pins: BTreeSet<TranscriptTurnDetailPinKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TranscriptTurnDetailEntryState {
    Missing,
    Loading { request_id: u64 },
    Full { item_count: usize },
    Failed,
}

impl TranscriptTurnSkeleton {
    pub(crate) fn from_turn(turn: &TurnInfo) -> Self {
        Self::from_turn_in_history_page(turn, None, 0, 1)
    }

    pub(crate) fn from_turn_in_history_page(
        turn: &TurnInfo,
        history_page_cursor: Option<String>,
        history_page_index: usize,
        history_page_len: usize,
    ) -> Self {
        Self {
            id: turn.id.clone(),
            status: turn.status,
            items_view: turn.items_view,
            error: turn.error.clone(),
            history_page_cursor,
            history_page_index,
            history_page_len,
        }
    }

    fn page_locator(&self) -> TranscriptTurnDetailPageLocator {
        let page_len = self
            .history_page_len
            .max(self.history_page_index.saturating_add(1))
            .max(1);
        let page_index = self.history_page_index.min(page_len.saturating_sub(1));
        let required_desc_prefix_len = page_len.saturating_sub(page_index).max(1);
        let limit = required_desc_prefix_len.min(u32::MAX as usize) as u32;
        TranscriptTurnDetailPageLocator {
            cursor: self.history_page_cursor.clone(),
            limit,
        }
    }
}

impl TranscriptTurnDetailPageLocator {
    pub(crate) fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    pub(crate) fn limit(&self) -> u32 {
        self.limit
    }
}

impl TranscriptTurnDetailLoadTicket {
    pub(crate) fn thread_id(&self) -> &str {
        &self.thread_id
    }

    pub(crate) fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub(crate) fn page_locator(&self) -> Option<&TranscriptTurnDetailPageLocator> {
        self.page_locator.as_ref()
    }

    pub(crate) fn coalesced_turn_ids(&self) -> &[String] {
        &self.coalesced_turn_ids
    }

    fn coalesces_turn(&self, turn_id: &str) -> bool {
        self.coalesced_turn_ids
            .iter()
            .any(|coalesced_turn_id| coalesced_turn_id == turn_id)
    }
}

impl TranscriptTurnDetailLoadStart {
    pub(crate) fn ticket(&self) -> Option<&TranscriptTurnDetailLoadTicket> {
        match self {
            Self::Started(ticket) | Self::AlreadyLoading(ticket) => Some(ticket),
            Self::AlreadyFull => None,
        }
    }
}

impl TranscriptTurnDetailViewportPlan {
    pub(crate) fn from_priority_and_retained<I, S, J, T>(
        priority_turn_ids: I,
        retained_turn_ids: J,
        order: TranscriptTurnDetailViewportOrder,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
        J: IntoIterator<Item = T>,
        T: AsRef<str>,
    {
        let priority_turn_ids = priority_turn_ids
            .into_iter()
            .map(|turn_id| turn_id.as_ref().to_string())
            .collect::<Vec<_>>();
        let retained_turn_ids = retained_turn_ids
            .into_iter()
            .map(|turn_id| turn_id.as_ref().to_string())
            .collect::<Vec<_>>();
        let mut ordered_priority_turn_ids = Vec::new();
        let mut seen = BTreeSet::new();
        push_ordered_unique(
            &mut ordered_priority_turn_ids,
            &mut seen,
            &priority_turn_ids,
            order,
        );

        Self {
            retained_turn_ids,
            priority_turn_ids: ordered_priority_turn_ids,
        }
    }

    pub(crate) fn from_visible_and_retained<I, S, J, T>(
        visible_turn_ids: I,
        retained_turn_ids: J,
        order: TranscriptTurnDetailViewportOrder,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
        J: IntoIterator<Item = T>,
        T: AsRef<str>,
    {
        let visible_turn_ids = visible_turn_ids
            .into_iter()
            .map(|turn_id| turn_id.as_ref().to_string())
            .collect::<Vec<_>>();
        let retained_turn_ids = retained_turn_ids
            .into_iter()
            .map(|turn_id| turn_id.as_ref().to_string())
            .collect::<Vec<_>>();
        let mut priority_turn_ids = Vec::new();
        let mut seen = BTreeSet::new();
        push_ordered_unique(&mut priority_turn_ids, &mut seen, &visible_turn_ids, order);
        push_ordered_unique(&mut priority_turn_ids, &mut seen, &retained_turn_ids, order);

        Self {
            retained_turn_ids,
            priority_turn_ids,
        }
    }
}

impl TranscriptTurnDetailEntry {
    fn missing() -> Self {
        Self {
            skeleton: None,
            state: TranscriptTurnDetailEntryState::Missing,
            pins: BTreeSet::new(),
        }
    }

    fn status(&self) -> TranscriptTurnDetailStatus {
        match &self.state {
            TranscriptTurnDetailEntryState::Missing => TranscriptTurnDetailStatus::Missing,
            TranscriptTurnDetailEntryState::Loading { .. } => TranscriptTurnDetailStatus::Loading,
            TranscriptTurnDetailEntryState::Full { .. } => TranscriptTurnDetailStatus::Full,
            TranscriptTurnDetailEntryState::Failed => TranscriptTurnDetailStatus::Failed,
        }
    }

    fn is_pinned(&self) -> bool {
        !self.pins.is_empty()
    }

    fn cleanup_candidate(&self) -> bool {
        self.skeleton.is_none()
            && self.pins.is_empty()
            && matches!(self.state, TranscriptTurnDetailEntryState::Missing)
    }
}

fn push_ordered_unique(
    destination: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    turn_ids: &[String],
    order: TranscriptTurnDetailViewportOrder,
) {
    match order {
        TranscriptTurnDetailViewportOrder::OldestFirst => {
            for turn_id in turn_ids {
                if seen.insert(turn_id.clone()) {
                    destination.push(turn_id.clone());
                }
            }
        }
        TranscriptTurnDetailViewportOrder::NewestFirst => {
            for turn_id in turn_ids.iter().rev() {
                if seen.insert(turn_id.clone()) {
                    destination.push(turn_id.clone());
                }
            }
        }
    }
}

fn normalize_coalesced_turn_ids(
    target_turn_id: &str,
    turn_ids: Vec<String>,
) -> impl Iterator<Item = String> + '_ {
    let mut seen = BTreeSet::new();
    std::iter::once(target_turn_id.to_string())
        .chain(turn_ids)
        .filter(move |turn_id| seen.insert(turn_id.clone()))
}

impl TranscriptTurnDetailRetention {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn from_turn_ids<I, S>(turn_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut retention = Self::new();
        retention.include_turn_ids(turn_ids);
        retention
    }

    pub(crate) fn from_visible_range(
        ordered_turn_ids: &[String],
        visible_range: Range<usize>,
        overscan: usize,
    ) -> Self {
        let mut retention = Self::new();
        retention.include_visible_range(ordered_turn_ids, visible_range, overscan);
        retention
    }

    pub(crate) fn include_visible_range(
        &mut self,
        ordered_turn_ids: &[String],
        visible_range: Range<usize>,
        overscan: usize,
    ) {
        let start = visible_range.start.saturating_sub(overscan);
        let end = visible_range
            .end
            .saturating_add(overscan)
            .min(ordered_turn_ids.len())
            .max(start.min(ordered_turn_ids.len()));
        for turn_id in &ordered_turn_ids[start.min(ordered_turn_ids.len())..end] {
            self.turn_ids.insert(turn_id.clone());
        }
    }

    pub(crate) fn include_turn_id(&mut self, turn_id: impl Into<String>) {
        self.turn_ids.insert(turn_id.into());
    }

    pub(crate) fn include_turn_ids<I, S>(&mut self, turn_ids: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for turn_id in turn_ids {
            self.turn_ids.insert(turn_id.as_ref().to_string());
        }
    }

    pub(crate) fn contains(&self, turn_id: &str) -> bool {
        self.turn_ids.contains(turn_id)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &str> {
        self.turn_ids.iter().map(String::as_str)
    }

    pub(crate) fn len(&self) -> usize {
        self.turn_ids.len()
    }
}

impl TranscriptTurnDetailCache {
    pub(crate) fn reset_for_thread(&mut self, thread_id: impl Into<String>) {
        self.thread_id = Some(thread_id.into());
        self.generation = self.generation.saturating_add(1);
        self.next_request_id = 0;
        self.entries.clear();
    }

    pub(crate) fn clear(&mut self) {
        self.thread_id = None;
        self.generation = self.generation.saturating_add(1);
        self.next_request_id = 0;
        self.entries.clear();
    }

    pub(crate) fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }

    pub(crate) fn insert_skeleton(&mut self, skeleton: TranscriptTurnSkeleton) {
        let entry = self.entry_mut(&skeleton.id);
        entry.skeleton = Some(skeleton);
    }

    pub(crate) fn insert_skeleton_from_turn(&mut self, turn: &TurnInfo) {
        self.insert_skeleton(TranscriptTurnSkeleton::from_turn(turn));
    }

    pub(crate) fn insert_skeletons_from_history_page(
        &mut self,
        turns: &[TurnInfo],
        history_page_cursor: Option<&str>,
    ) {
        let history_page_len = turns.len();
        for (history_page_index, turn) in turns.iter().enumerate() {
            self.insert_skeleton(TranscriptTurnSkeleton::from_turn_in_history_page(
                turn,
                history_page_cursor.map(str::to_string),
                history_page_index,
                history_page_len,
            ));
        }
    }

    pub(crate) fn insert_skeletons_from_turns<'a>(
        &mut self,
        turns: impl IntoIterator<Item = &'a TurnInfo>,
    ) {
        for turn in turns {
            self.insert_skeleton_from_turn(turn);
        }
    }

    pub(crate) fn begin_loading(
        &mut self,
        thread_id: &str,
        turn_id: &str,
    ) -> Option<TranscriptTurnDetailLoadStart> {
        self.begin_loading_group(thread_id, turn_id, vec![turn_id.to_string()])
    }

    fn begin_loading_group(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        coalesced_turn_ids: Vec<String>,
    ) -> Option<TranscriptTurnDetailLoadStart> {
        if self.thread_id.as_deref() != Some(thread_id) {
            return None;
        }

        let generation = self.generation;
        if let Some(entry) = self.entries.get(turn_id) {
            match &entry.state {
                TranscriptTurnDetailEntryState::Full { .. } => {
                    return Some(TranscriptTurnDetailLoadStart::AlreadyFull);
                }
                TranscriptTurnDetailEntryState::Loading { request_id } => {
                    let page_locator = self.page_locator_for_turn(turn_id);
                    return Some(TranscriptTurnDetailLoadStart::AlreadyLoading(self.ticket(
                        thread_id,
                        turn_id,
                        generation,
                        *request_id,
                        page_locator,
                        vec![turn_id.to_string()],
                    )));
                }
                TranscriptTurnDetailEntryState::Missing
                | TranscriptTurnDetailEntryState::Failed => {}
            }
        }

        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        let page_locator = self.page_locator_for_turn(turn_id);
        let coalesced_turn_ids =
            normalize_coalesced_turn_ids(turn_id, coalesced_turn_ids).collect::<Vec<_>>();
        self.entry_mut(turn_id).state = TranscriptTurnDetailEntryState::Loading { request_id };
        for coalesced_turn_id in &coalesced_turn_ids {
            if coalesced_turn_id == turn_id {
                continue;
            }
            if self.should_request_full_details(coalesced_turn_id) {
                self.entry_mut(coalesced_turn_id).state =
                    TranscriptTurnDetailEntryState::Loading { request_id };
            }
        }
        if !matches!(
            self.entries.get(turn_id).map(|entry| &entry.state),
            Some(TranscriptTurnDetailEntryState::Loading { request_id: current_request_id })
                if *current_request_id == request_id
        ) {
            return None;
        }
        Some(TranscriptTurnDetailLoadStart::Started(self.ticket(
            thread_id,
            turn_id,
            generation,
            request_id,
            page_locator,
            coalesced_turn_ids,
        )))
    }

    pub(crate) fn finish_loading(
        &mut self,
        ticket: &TranscriptTurnDetailLoadTicket,
        item_count: usize,
    ) -> TranscriptTurnDetailApplyResult {
        self.finish_coalesced_loading(ticket, ticket.turn_id(), item_count)
    }

    pub(crate) fn finish_coalesced_loading(
        &mut self,
        ticket: &TranscriptTurnDetailLoadTicket,
        turn_id: &str,
        item_count: usize,
    ) -> TranscriptTurnDetailApplyResult {
        if !self.ticket_matches_cache(ticket) {
            return TranscriptTurnDetailApplyResult::Stale;
        }
        if !ticket.coalesces_turn(turn_id) {
            return TranscriptTurnDetailApplyResult::Stale;
        }

        let Some(entry) = self.entries.get_mut(turn_id) else {
            return TranscriptTurnDetailApplyResult::Stale;
        };
        if !matches!(
            &entry.state,
            TranscriptTurnDetailEntryState::Loading { request_id }
                if *request_id == ticket.request_id
        ) {
            return TranscriptTurnDetailApplyResult::Stale;
        }

        entry.state = TranscriptTurnDetailEntryState::Full { item_count };
        TranscriptTurnDetailApplyResult::Applied
    }

    pub(crate) fn fail_loading(
        &mut self,
        ticket: &TranscriptTurnDetailLoadTicket,
    ) -> TranscriptTurnDetailApplyResult {
        self.fail_coalesced_loading(ticket, ticket.turn_id())
    }

    fn fail_coalesced_loading(
        &mut self,
        ticket: &TranscriptTurnDetailLoadTicket,
        turn_id: &str,
    ) -> TranscriptTurnDetailApplyResult {
        if !self.ticket_matches_cache(ticket) {
            return TranscriptTurnDetailApplyResult::Stale;
        }
        if !ticket.coalesces_turn(turn_id) {
            return TranscriptTurnDetailApplyResult::Stale;
        }

        let Some(entry) = self.entries.get_mut(turn_id) else {
            return TranscriptTurnDetailApplyResult::Stale;
        };
        if !matches!(
            &entry.state,
            TranscriptTurnDetailEntryState::Loading { request_id }
                if *request_id == ticket.request_id
        ) {
            return TranscriptTurnDetailApplyResult::Stale;
        }

        entry.state = TranscriptTurnDetailEntryState::Failed;
        TranscriptTurnDetailApplyResult::Applied
    }

    pub(crate) fn fail_loading_group(
        &mut self,
        ticket: &TranscriptTurnDetailLoadTicket,
    ) -> Vec<String> {
        let mut failed_turn_ids = Vec::new();
        for turn_id in ticket.coalesced_turn_ids() {
            if self.fail_coalesced_loading(ticket, turn_id)
                == TranscriptTurnDetailApplyResult::Applied
            {
                failed_turn_ids.push(turn_id.clone());
            }
        }
        failed_turn_ids
    }

    pub(crate) fn should_start_loading(&self, ticket: &TranscriptTurnDetailLoadTicket) -> bool {
        if !self.ticket_matches_cache(ticket) {
            return false;
        }

        self.entries
            .get(ticket.turn_id.as_str())
            .is_some_and(|entry| {
                matches!(
                    &entry.state,
                    TranscriptTurnDetailEntryState::Loading { request_id }
                        if *request_id == ticket.request_id
                )
            })
    }

    pub(crate) fn current_loading_coalesced_turn_ids(
        &self,
        ticket: &TranscriptTurnDetailLoadTicket,
    ) -> Vec<String> {
        if !self.ticket_matches_cache(ticket) {
            return Vec::new();
        }

        ticket
            .coalesced_turn_ids()
            .iter()
            .filter(|turn_id| {
                self.entries.get(turn_id.as_str()).is_some_and(|entry| {
                    matches!(
                        &entry.state,
                        TranscriptTurnDetailEntryState::Loading { request_id }
                            if *request_id == ticket.request_id
                    )
                })
            })
            .cloned()
            .collect()
    }

    pub(crate) fn skip_loading(
        &mut self,
        ticket: &TranscriptTurnDetailLoadTicket,
    ) -> TranscriptTurnDetailApplyResult {
        let mut skipped_any = false;
        for turn_id in ticket.coalesced_turn_ids() {
            if self.skip_coalesced_loading(ticket, turn_id)
                == TranscriptTurnDetailApplyResult::Applied
            {
                skipped_any = true;
            }
        }
        if skipped_any {
            TranscriptTurnDetailApplyResult::Applied
        } else {
            TranscriptTurnDetailApplyResult::Stale
        }
    }

    pub(crate) fn skip_coalesced_loading(
        &mut self,
        ticket: &TranscriptTurnDetailLoadTicket,
        turn_id: &str,
    ) -> TranscriptTurnDetailApplyResult {
        if !self.ticket_matches_cache(ticket) {
            return TranscriptTurnDetailApplyResult::Stale;
        }
        if !ticket.coalesces_turn(turn_id) {
            return TranscriptTurnDetailApplyResult::Stale;
        }

        let Some(entry) = self.entries.get_mut(turn_id) else {
            return TranscriptTurnDetailApplyResult::Stale;
        };
        if !matches!(
            &entry.state,
            TranscriptTurnDetailEntryState::Loading { request_id }
                if *request_id == ticket.request_id
        ) {
            return TranscriptTurnDetailApplyResult::Stale;
        }

        entry.state = TranscriptTurnDetailEntryState::Missing;
        let should_remove = entry.cleanup_candidate();
        if should_remove {
            self.entries.remove(turn_id);
        }
        TranscriptTurnDetailApplyResult::Applied
    }

    pub(crate) fn status(&self, turn_id: &str) -> TranscriptTurnDetailStatus {
        self.entries
            .get(turn_id)
            .map(TranscriptTurnDetailEntry::status)
            .unwrap_or(TranscriptTurnDetailStatus::Missing)
    }

    pub(crate) fn is_missing_detail_requestable(&self, turn_id: &str) -> bool {
        self.should_request_full_details(turn_id)
    }

    pub(crate) fn full_item_count(&self, turn_id: &str) -> Option<usize> {
        match &self.entries.get(turn_id)?.state {
            TranscriptTurnDetailEntryState::Full { item_count } => Some(*item_count),
            _ => None,
        }
    }

    pub(crate) fn schedule_viewport_full_details(
        &mut self,
        thread_id: &str,
        plan: TranscriptTurnDetailViewportPlan,
        max_requested_tickets: usize,
    ) -> TranscriptTurnDetailSchedule {
        let retention = TranscriptTurnDetailRetention::from_turn_ids(&plan.retained_turn_ids);
        let mut requested_tickets = Vec::new();

        for turn_id in &plan.priority_turn_ids {
            if requested_tickets.len() >= max_requested_tickets {
                break;
            }
            if !retention.contains(turn_id) {
                continue;
            }
            if !self.should_request_full_details(turn_id) {
                continue;
            }
            let coalesced_turn_ids =
                self.coalesced_loading_turn_ids_for(turn_id, &retention, &plan.priority_turn_ids);
            let Some(start) = self.begin_loading_group(thread_id, turn_id, coalesced_turn_ids)
            else {
                continue;
            };
            if let TranscriptTurnDetailLoadStart::Started(ticket) = start {
                requested_tickets.push(ticket);
            }
        }

        let released = self.release_unretained_details(&retention);
        TranscriptTurnDetailSchedule {
            retained_turns: retention.len(),
            requested_tickets,
            released,
        }
    }

    pub(crate) fn pin_turn(&mut self, turn_id: &str, kind: TranscriptTurnDetailPinKind) {
        self.entry_mut(turn_id).pins.insert(kind);
    }

    pub(crate) fn unpin_turn(&mut self, turn_id: &str, kind: TranscriptTurnDetailPinKind) {
        let should_remove = if let Some(entry) = self.entries.get_mut(turn_id) {
            entry.pins.remove(&kind);
            entry.cleanup_candidate()
        } else {
            false
        };
        if should_remove {
            self.entries.remove(turn_id);
        }
    }

    pub(crate) fn replace_pins<I, S>(&mut self, kind: TranscriptTurnDetailPinKind, turn_ids: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut empty_entries = Vec::new();
        for (turn_id, entry) in &mut self.entries {
            entry.pins.remove(&kind);
            if entry.cleanup_candidate() {
                empty_entries.push(turn_id.clone());
            }
        }
        for turn_id in empty_entries {
            self.entries.remove(&turn_id);
        }
        for turn_id in turn_ids {
            self.pin_turn(turn_id.as_ref(), kind);
        }
    }

    pub(crate) fn release_unretained_details(
        &mut self,
        retention: &TranscriptTurnDetailRetention,
    ) -> TranscriptTurnDetailReleaseCounts {
        let mut released = TranscriptTurnDetailReleaseCounts::default();
        let turn_ids = self.entries.keys().cloned().collect::<Vec<_>>();

        for turn_id in turn_ids {
            let Some(entry) = self.entries.get_mut(turn_id.as_str()) else {
                continue;
            };
            if retention.contains(turn_id.as_str()) || entry.is_pinned() {
                continue;
            }

            match std::mem::replace(&mut entry.state, TranscriptTurnDetailEntryState::Missing) {
                TranscriptTurnDetailEntryState::Missing => {}
                TranscriptTurnDetailEntryState::Loading { .. } => {
                    released.loading_detail_turns = released.loading_detail_turns.saturating_add(1);
                }
                TranscriptTurnDetailEntryState::Full { item_count } => {
                    released.full_detail_turns = released.full_detail_turns.saturating_add(1);
                    released.retained_item_count =
                        released.retained_item_count.saturating_add(item_count);
                    released.released_turn_ids.push(turn_id.clone());
                }
                TranscriptTurnDetailEntryState::Failed => {
                    released.failed_detail_turns = released.failed_detail_turns.saturating_add(1);
                    released.released_turn_ids.push(turn_id.clone());
                }
            }
        }

        self.entries.retain(|_, entry| !entry.cleanup_candidate());
        released
    }

    pub(crate) fn prune_skeletons_to_protected_turns<I, S>(
        &mut self,
        protected_turn_ids: I,
    ) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let protected_turn_ids = protected_turn_ids
            .into_iter()
            .map(|turn_id| turn_id.as_ref().to_string())
            .collect::<BTreeSet<_>>();
        let mut pruned = 0usize;

        for (turn_id, entry) in &mut self.entries {
            if protected_turn_ids.contains(turn_id.as_str())
                || entry.is_pinned()
                || !matches!(&entry.state, TranscriptTurnDetailEntryState::Missing)
            {
                continue;
            }

            if entry.skeleton.take().is_some() {
                pruned = pruned.saturating_add(1);
            }
        }

        self.entries.retain(|_, entry| !entry.cleanup_candidate());
        pruned
    }

    pub(crate) fn retained_counts(&self) -> TranscriptTurnDetailRetainedCounts {
        let mut counts = TranscriptTurnDetailRetainedCounts::default();
        for entry in self.entries.values() {
            counts.skeleton_turns = counts
                .skeleton_turns
                .saturating_add(usize::from(entry.skeleton.is_some()));
            counts.pinned_turns = counts
                .pinned_turns
                .saturating_add(usize::from(entry.is_pinned()));
            match &entry.state {
                TranscriptTurnDetailEntryState::Missing => {
                    counts.missing_detail_turns = counts.missing_detail_turns.saturating_add(1);
                }
                TranscriptTurnDetailEntryState::Loading { .. } => {
                    counts.loading_detail_turns = counts.loading_detail_turns.saturating_add(1);
                }
                TranscriptTurnDetailEntryState::Full { item_count } => {
                    counts.full_detail_turns = counts.full_detail_turns.saturating_add(1);
                    counts.retained_item_count =
                        counts.retained_item_count.saturating_add(*item_count);
                }
                TranscriptTurnDetailEntryState::Failed => {
                    counts.failed_detail_turns = counts.failed_detail_turns.saturating_add(1);
                }
            }
        }
        counts
    }

    fn ticket(
        &self,
        thread_id: &str,
        turn_id: &str,
        generation: u64,
        request_id: u64,
        page_locator: Option<TranscriptTurnDetailPageLocator>,
        coalesced_turn_ids: Vec<String>,
    ) -> TranscriptTurnDetailLoadTicket {
        TranscriptTurnDetailLoadTicket {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            generation,
            request_id,
            page_locator,
            coalesced_turn_ids,
        }
    }

    fn ticket_matches_cache(&self, ticket: &TranscriptTurnDetailLoadTicket) -> bool {
        self.thread_id.as_deref() == Some(ticket.thread_id.as_str())
            && self.generation == ticket.generation
    }

    fn should_request_full_details(&self, turn_id: &str) -> bool {
        let Some(entry) = self.entries.get(turn_id) else {
            return false;
        };
        match &entry.state {
            TranscriptTurnDetailEntryState::Full { .. }
            | TranscriptTurnDetailEntryState::Loading { .. }
            | TranscriptTurnDetailEntryState::Failed => false,
            TranscriptTurnDetailEntryState::Missing => entry
                .skeleton
                .as_ref()
                .is_some_and(|skeleton| skeleton.items_view != TurnItemsView::Full),
        }
    }

    fn page_locator_for_turn(&self, turn_id: &str) -> Option<TranscriptTurnDetailPageLocator> {
        self.entries
            .get(turn_id)
            .and_then(|entry| entry.skeleton.as_ref())
            .map(TranscriptTurnSkeleton::page_locator)
    }

    fn coalesced_loading_turn_ids_for(
        &self,
        target_turn_id: &str,
        retention: &TranscriptTurnDetailRetention,
        priority_turn_ids: &[String],
    ) -> Vec<String> {
        let Some(target_skeleton) = self
            .entries
            .get(target_turn_id)
            .and_then(|entry| entry.skeleton.as_ref())
        else {
            return vec![target_turn_id.to_string()];
        };
        if target_skeleton.history_page_len <= 1 {
            return vec![target_turn_id.to_string()];
        };
        let mut turn_ids = vec![target_turn_id.to_string()];
        let mut seen = BTreeSet::from([target_turn_id.to_string()]);
        for turn_id in priority_turn_ids {
            if !seen.insert(turn_id.clone()) {
                continue;
            }
            if !retention.contains(turn_id) || !self.should_request_full_details(turn_id) {
                continue;
            }
            let Some(skeleton) = self
                .entries
                .get(turn_id)
                .and_then(|entry| entry.skeleton.as_ref())
            else {
                continue;
            };
            if skeleton.history_page_cursor == target_skeleton.history_page_cursor
                && skeleton.history_page_len == target_skeleton.history_page_len
                && skeleton.history_page_index >= target_skeleton.history_page_index
            {
                turn_ids.push(turn_id.clone());
            }
        }
        turn_ids
    }

    fn entry_mut(&mut self, turn_id: &str) -> &mut TranscriptTurnDetailEntry {
        self.entries
            .entry(turn_id.to_string())
            .or_insert_with(TranscriptTurnDetailEntry::missing)
    }
}
