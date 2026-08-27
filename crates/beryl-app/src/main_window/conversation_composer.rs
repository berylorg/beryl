use gpui::{Context, Window};
use gpui_text_input::{
    RangeTextInput, RangeTextInputConfig, RangeTextInputError, TextInputAtomClipboardPolicy,
    TextInputEnterKey, TextInputRichPastePolicy,
};

use super::{MainWindowComposerSelectionIdentity, MainWindowComposerSuccessorProofLimits};
use crate::composer_host::ComposerHostBinding;
use syndic_storage::{
    DraftEditHistoryFrontierReferenceV1, DraftPieceRootReferenceV1, DraftRootHistoryPairV1,
};

pub struct MainWindowConversationComposerConfig {
    selection: MainWindowComposerSelectionIdentity,
    widget: RangeTextInputConfig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerResidencyBound {
    text_pages: usize,
    text_bytes: usize,
    object_pages: usize,
    objects: usize,
    object_bytes: usize,
}

impl MainWindowComposerResidencyBound {
    fn from_widget(widget: &RangeTextInputConfig) -> Option<Self> {
        Some(Self {
            text_pages: widget
                .residency_limits
                .max_resident_pages()
                .checked_add(widget.residency_limits.max_pending_requests())?,
            text_bytes: widget
                .residency_limits
                .max_resident_bytes()
                .checked_add(usize::try_from(widget.residency_limits.max_pending_bytes()).ok()?)?,
            object_pages: widget
                .object_residency_limits
                .max_resident_pages()
                .checked_add(widget.object_residency_limits.max_pending_requests())?,
            objects: widget
                .object_residency_limits
                .max_resident_objects()
                .checked_add(widget.object_residency_limits.max_pending_objects())?,
            object_bytes: widget
                .object_residency_limits
                .max_resident_bytes()
                .checked_add(widget.object_residency_limits.max_pending_bytes())?,
        })
    }

    pub(super) fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            text_pages: self.text_pages.checked_add(other.text_pages)?,
            text_bytes: self.text_bytes.checked_add(other.text_bytes)?,
            object_pages: self.object_pages.checked_add(other.object_pages)?,
            objects: self.objects.checked_add(other.objects)?,
            object_bytes: self.object_bytes.checked_add(other.object_bytes)?,
        })
    }

    pub const fn text_pages(self) -> usize {
        self.text_pages
    }

    pub const fn text_bytes(self) -> usize {
        self.text_bytes
    }

    pub const fn object_pages(self) -> usize {
        self.object_pages
    }

    pub const fn objects(self) -> usize {
        self.objects
    }

    pub const fn object_bytes(self) -> usize {
        self.object_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerActivationResidency {
    bound: MainWindowComposerResidencyBound,
    current_text_pages: usize,
    current_text_bytes: usize,
    current_objects: usize,
    current_object_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MainWindowComposerResidencyUsage {
    text_pages: usize,
    text_bytes: usize,
    objects: usize,
    object_bytes: usize,
}

impl MainWindowComposerResidencyUsage {
    pub(super) fn from_current(
        current: &gpui_text_input::RangeRealizationOwnership,
    ) -> Option<Self> {
        Some(Self {
            text_pages: current
                .resident_pages
                .checked_add(current.pending_page_requests)?,
            text_bytes: current
                .resident_page_bytes
                .checked_add(current.pending_page_bytes)?,
            objects: current
                .resident_objects
                .checked_add(current.pending_object_requests)?,
            object_bytes: current
                .resident_object_bytes
                .checked_add(current.pending_object_bytes)?,
        })
    }

    pub(super) fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            text_pages: self.text_pages.checked_add(other.text_pages)?,
            text_bytes: self.text_bytes.checked_add(other.text_bytes)?,
            objects: self.objects.checked_add(other.objects)?,
            object_bytes: self.object_bytes.checked_add(other.object_bytes)?,
        })
    }

    pub(super) fn admit(
        self,
        bound: MainWindowComposerResidencyBound,
    ) -> Option<MainWindowComposerActivationResidency> {
        (self.text_pages <= bound.text_pages
            && self.text_bytes <= bound.text_bytes
            && self.objects <= bound.objects
            && self.object_bytes <= bound.object_bytes)
            .then_some(MainWindowComposerActivationResidency {
                bound,
                current_text_pages: self.text_pages,
                current_text_bytes: self.text_bytes,
                current_objects: self.objects,
                current_object_bytes: self.object_bytes,
            })
    }
}

impl MainWindowComposerActivationResidency {
    pub const fn bound(self) -> MainWindowComposerResidencyBound {
        self.bound
    }

    pub const fn current_text_pages(self) -> usize {
        self.current_text_pages
    }

    pub const fn current_text_bytes(self) -> usize {
        self.current_text_bytes
    }

    pub const fn current_objects(self) -> usize {
        self.current_objects
    }

    pub const fn current_object_bytes(self) -> usize {
        self.current_object_bytes
    }
}

impl MainWindowConversationComposerConfig {
    pub fn new(
        selection: MainWindowComposerSelectionIdentity,
        widget: RangeTextInputConfig,
    ) -> Result<Self, MainWindowConversationComposerConfigError> {
        if widget.binding != selection.binding().range_binding() {
            return Err(MainWindowConversationComposerConfigError::BindingMismatch);
        }
        if widget.presentation_generation.get()
            != selection.binding().presentation_generation().get()
        {
            return Err(MainWindowConversationComposerConfigError::PresentationMismatch);
        }
        if widget.enter_key != TextInputEnterKey::Propagate
            || widget.atom_clipboard_policy != TextInputAtomClipboardPolicy::Propagate
            || widget.rich_paste_policy != TextInputRichPastePolicy::Propagate
        {
            return Err(MainWindowConversationComposerConfigError::PolicyMismatch);
        }
        if widget.limits.max_surface_bytes == 0
            || widget.limits.max_surface_items == 0
            || widget.limits.max_realization_work_per_frame == 0
            || widget.limits.max_realized_block_extent <= gpui::Pixels::ZERO
            || !f32::from(widget.limits.max_realized_block_extent).is_finite()
            || widget.limits.page_bytes == 0
            || widget.limits.platform_bytes == 0
            || widget.limits.max_intra_anchor < gpui::Pixels::ZERO
            || !f32::from(widget.limits.max_intra_anchor).is_finite()
            || widget.viewport_extent <= gpui::Pixels::ZERO
            || !f32::from(widget.viewport_extent).is_finite()
            || widget.overscan < gpui::Pixels::ZERO
            || !f32::from(widget.overscan).is_finite()
        {
            return Err(MainWindowConversationComposerConfigError::InvalidRealizationBudget);
        }
        Ok(Self { selection, widget })
    }

    pub const fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }

    pub const fn binding(&self) -> ComposerHostBinding {
        self.selection.binding()
    }

    pub(super) const fn successor_proof_limits(&self) -> MainWindowComposerSuccessorProofLimits {
        MainWindowComposerSuccessorProofLimits {
            text: self.widget.residency_limits,
            objects: self.widget.object_residency_limits,
            presentation_generation: self.widget.presentation_generation,
        }
    }

    pub(super) const fn clipboard_limits(&self) -> gpui_text_input::ClipboardLimits {
        self.widget.clipboard_limits
    }

    pub(super) const fn mutation_limits(&self) -> gpui_text_input::MutationLimits {
        self.widget.mutation_limits
    }

    pub(super) fn residency_bound(&self) -> Result<MainWindowComposerResidencyBound, String> {
        MainWindowComposerResidencyBound::from_widget(&self.widget)
            .ok_or_else(|| "composer residency bound overflowed".to_owned())
    }

    pub fn mount(
        self,
        window: &mut Window,
        cx: &mut Context<RangeTextInput>,
    ) -> Result<RangeTextInput, RangeTextInputError> {
        RangeTextInput::new(self.widget, window, cx)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum MainWindowConversationComposerConfigError {
    #[error("composer widget binding does not match the selected host")]
    BindingMismatch,
    #[error("composer widget presentation generation does not match the selected host")]
    PresentationMismatch,
    #[error("composer widget must propagate Enter, atom clipboard, and rich paste")]
    PolicyMismatch,
    #[error("composer widget realization budgets must be nonzero")]
    InvalidRealizationBudget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerPublishedDraft {
    candidate_generation: u64,
    pair: DraftRootHistoryPairV1,
}

impl MainWindowComposerPublishedDraft {
    pub const fn candidate_generation(self) -> u64 {
        self.candidate_generation
    }

    pub const fn root(self) -> DraftPieceRootReferenceV1 {
        self.pair.root()
    }

    pub const fn history(self) -> DraftEditHistoryFrontierReferenceV1 {
        self.pair.history()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerDraftState {
    adopted: ComposerHostBinding,
    published: MainWindowComposerPublishedDraft,
}

impl MainWindowComposerDraftState {
    pub fn new(
        adopted: ComposerHostBinding,
        published_candidate_generation: u64,
        published_pair: DraftRootHistoryPairV1,
    ) -> Result<Self, MainWindowComposerDraftStateError> {
        let published = MainWindowComposerPublishedDraft {
            candidate_generation: published_candidate_generation,
            pair: published_pair,
        };
        if !valid_published_draft(adopted, published) {
            return Err(MainWindowComposerDraftStateError::Stale);
        }
        Ok(Self { adopted, published })
    }

    pub const fn adopted(self) -> ComposerHostBinding {
        self.adopted
    }

    pub const fn published(self) -> MainWindowComposerPublishedDraft {
        self.published
    }

    pub fn is_dirty(self) -> bool {
        self.adopted.candidate().candidate_generation() != self.published.candidate_generation()
            || self.adopted.root() != self.published.root()
            || self.adopted.history() != self.published.history()
    }

    pub fn adopt(
        &mut self,
        predecessor: ComposerHostBinding,
        successor: ComposerHostBinding,
    ) -> Result<(), MainWindowComposerDraftStateError> {
        if self.adopted != predecessor || !same_editor_session(predecessor, successor) {
            return Err(MainWindowComposerDraftStateError::Stale);
        }
        self.adopted = successor;
        Ok(())
    }

    pub fn publish(
        &mut self,
        captured: ComposerHostBinding,
        adopted: ComposerHostBinding,
        published_candidate_generation: u64,
        published_pair: DraftRootHistoryPairV1,
    ) -> Result<(), MainWindowComposerDraftStateError> {
        let published = MainWindowComposerPublishedDraft {
            candidate_generation: published_candidate_generation,
            pair: published_pair,
        };
        if !same_adopted_candidate(self.adopted, adopted)
            || (self.adopted.history() != adopted.history()
                && !(published_candidate_generation == adopted.candidate().candidate_generation()
                    && published.root() == adopted.root()
                    && published.history() == adopted.history()))
            || !same_editor_session(captured, adopted)
            || captured.candidate().candidate_generation() > published_candidate_generation
            || self.published.candidate_generation() > published_candidate_generation
            || !valid_published_draft(adopted, published)
        {
            return Err(MainWindowComposerDraftStateError::Stale);
        }
        self.adopted = adopted;
        self.published = published;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum MainWindowComposerDraftStateError {
    #[error("composer draft state update is stale")]
    Stale,
}

fn same_editor_session(left: ComposerHostBinding, right: ComposerHostBinding) -> bool {
    left.home_id() == right.home_id()
        && left.home_generation() == right.home_generation()
        && left.host_generation() == right.host_generation()
        && left.candidate().draft_id() == right.candidate().draft_id()
        && left.candidate().session_id() == right.candidate().session_id()
}

fn same_adopted_candidate(left: ComposerHostBinding, right: ComposerHostBinding) -> bool {
    same_editor_session(left, right)
        && left.presentation_generation() == right.presentation_generation()
        && left.candidate().candidate_generation() == right.candidate().candidate_generation()
        && left.root() == right.root()
        && left.logical_extent() == right.logical_extent()
}

fn valid_published_draft(
    adopted: ComposerHostBinding,
    published: MainWindowComposerPublishedDraft,
) -> bool {
    published.candidate_generation() <= adopted.candidate().candidate_generation()
        && published.history().candidate_generation() == published.candidate_generation()
        && published.history().root() == published.root()
        && published.root().key().draft_id() == adopted.candidate().draft_id()
        && published.history().key().draft_id() == adopted.candidate().draft_id()
}
