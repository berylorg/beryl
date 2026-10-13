use beryl_home_store::CommandCancellation;

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::main_window) enum ComposerQuoteFeedbackKind {
    TooLarge,
    CapacityUnavailable,
    Storage,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::main_window) struct ComposerQuoteFeedback {
    pub request: u64,
    pub kind: ComposerQuoteFeedbackKind,
}

pub(super) struct ComposerQuoteInsertion {
    key: gpui_text_input::MutationKey,
    source: WeakEntity<crate::syndic_transcript::SyndicTranscriptPanel>,
    request: u64,
    activation: u64,
    presentation: u64,
    pub(super) cancellation: CommandCancellation,
    invalidated: bool,
    dispatched: bool,
    _subscription: Subscription,
}

impl MainWindowConversationComposer {
    pub(in crate::main_window) fn insert_quote(
        &mut self,
        markdown: String,
        source: &Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
        request: u64,
        activation: u64,
        presentation: u64,
        cancellation: CommandCancellation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<gpui_text_input::MutationKey, String> {
        self.quote_feedback = None;
        if !self.is_live()
            || self.mutation_gated()
            || self.quote_insertion.is_some()
            || self.pending_marker_metadata.is_some()
            || self.active_flight.is_some()
            || self.pending_dispatch.is_some()
            || self.mutation_evidence.is_some()
            || markdown.is_empty()
        {
            self.quote_feedback = Some(ComposerQuoteFeedback {
                request,
                kind: ComposerQuoteFeedbackKind::Unavailable,
            });
            cx.notify();
            return Err("Quote insertion is unavailable while the composer is busy.".into());
        }
        if cancellation.is_cancelled()
            || !source
                .read(cx)
                .quote_request_current(request, activation, presentation, window)
        {
            return Err("The selected transcript text is no longer available for Quote.".into());
        }
        let exceeds_page = markdown.len() > self.mutation_limits.max_page_bytes();
        let key = match self
            .input
            .update(cx, |input, cx| input.insert_text_at_caret(markdown, cx))
        {
            Ok(key) => key,
            Err(error) => {
                let kind = if exceeds_page {
                    ComposerQuoteFeedbackKind::TooLarge
                } else if matches!(error, gpui_text_input::RangeTextInputError::SurfaceCapacity) {
                    ComposerQuoteFeedbackKind::CapacityUnavailable
                } else {
                    ComposerQuoteFeedbackKind::Unavailable
                };
                self.quote_feedback = Some(ComposerQuoteFeedback { request, kind });
                cx.notify();
                return Err(format!("Quote insertion was rejected: {error}"));
            }
        };
        let subscription = cx.observe_in(source, window, |this, _, window, cx| {
            this.cancel_obsolete_quote(window, cx);
            this.schedule_pump(window, cx);
        });
        self.quote_insertion = Some(ComposerQuoteInsertion {
            key,
            source: source.downgrade(),
            request,
            activation,
            presentation,
            cancellation,
            invalidated: false,
            dispatched: false,
            _subscription: subscription,
        });
        self.schedule_pump(window, cx);
        Ok(key)
    }

    pub(in crate::main_window) fn quote_feedback(&self) -> Option<ComposerQuoteFeedback> {
        self.quote_feedback
    }

    pub(super) fn cancel_obsolete_quote(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(quote) = &self.quote_insertion else {
            return;
        };
        if quote.invalidated {
            return;
        }
        let current = quote.source.upgrade().is_some_and(|source| {
            source.read(cx).quote_request_current(
                quote.request,
                quote.activation,
                quote.presentation,
                window,
            )
        });
        if current && !quote.cancellation.is_cancelled() {
            return;
        }
        let key = quote.key;
        quote.cancellation.cancel();
        self.quote_insertion.as_mut().unwrap().invalidated = true;
        self.mark_quote_evidence_cancelled(key);
        self.observe_mutation_cancellation(key);
        let _ = self
            .input
            .update(cx, |input, cx| input.cancel_mutation(key, cx));
    }

    pub(super) fn quote_cancellation(
        &self,
        key: gpui_text_input::MutationKey,
    ) -> Option<CommandCancellation> {
        self.quote_insertion
            .as_ref()
            .filter(|quote| quote.key == key)
            .map(|quote| quote.cancellation.clone())
    }

    pub(super) fn clear_quote_insertion(&mut self, key: gpui_text_input::MutationKey) {
        if self
            .quote_insertion
            .as_ref()
            .is_some_and(|quote| quote.key == key)
        {
            self.quote_insertion = None;
        }
    }

    pub(super) fn quote_evidence_began(&mut self, key: gpui_text_input::MutationKey) {
        if let Some(quote) = self
            .quote_insertion
            .as_mut()
            .filter(|quote| quote.key == key)
        {
            quote.dispatched = true;
        }
    }

    pub(super) fn record_quote_refusal(
        &mut self,
        key: gpui_text_input::MutationKey,
        failure: &crate::composer_host::ComposerHostMutationAdmissionFailure,
    ) {
        use crate::composer_host::ComposerHostMutationAdmissionFailure as Failure;
        let Some(quote) = self
            .quote_insertion
            .as_ref()
            .filter(|quote| quote.key == key)
        else {
            return;
        };
        let kind = match failure {
            Failure::Cancelled => return,
            Failure::OperationTooLarge => ComposerQuoteFeedbackKind::TooLarge,
            Failure::CapacityUnavailable => ComposerQuoteFeedbackKind::CapacityUnavailable,
            Failure::Storage(_) => ComposerQuoteFeedbackKind::Storage,
            _ => ComposerQuoteFeedbackKind::Unavailable,
        };
        self.quote_feedback = Some(ComposerQuoteFeedback {
            request: quote.request,
            kind,
        });
    }

    pub(super) fn settle_unstarted_quote_cancellation(
        &mut self,
        request: &RangeTextInputRequest,
    ) -> bool {
        let RangeTextInputRequest::CancelMutation(cancel) = request else {
            return false;
        };
        if self
            .quote_insertion
            .as_ref()
            .is_some_and(|quote| quote.key == cancel.key() && !quote.dispatched)
        {
            self.clear_quote_insertion(cancel.key());
            return true;
        }
        false
    }
}
