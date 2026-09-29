use super::*;
use gpui_text_input::RangePrepublicationCleanupToken;

impl<C> MainWindowComposerCandidateCustody<C> {
    pub fn settle_undispatched(
        &mut self,
        effect: RangePrepublicationEffect,
    ) -> Result<(), (RangePrepublicationEffect, String)> {
        let state = self.state.borrow();
        if !state.cancelled || Some(generation(&effect)) != state.generation {
            return Err((
                effect,
                "candidate preparation is not cancelled or the effect is stale".into(),
            ));
        }
        let cleanup = state.cleanup.as_ref().unwrap();
        if let Err(error) = cleanup.begin(copy_effect(&effect)) {
            return Err((effect, error));
        }
        cleanup.finish_delivery(token(&effect));
        Ok(())
    }
    pub fn drive_cleanup(&self, limit: usize) {
        let state = self.state.borrow();
        // A returned payload must be delivered or discarded before its ledger can acknowledge release.
        if state.completion.is_none() {
            if let Some(cleanup) = &state.cleanup {
                cleanup.drive_cleanup(limit);
            }
        }
    }

    pub fn cleanup_drained(&self) -> bool {
        let state = self.state.borrow();
        !state.pending
            && state.completion.is_none()
            && state
                .cleanup
                .as_ref()
                .is_none_or(|cleanup| cleanup.drained())
    }
}

pub(super) fn generation(
    effect: &RangePrepublicationEffect,
) -> RangePrepublicationSessionGeneration {
    match effect {
        RangePrepublicationEffect::ValidateOwner(request) => request.key.generation,
        RangePrepublicationEffect::Page { generation, .. }
        | RangePrepublicationEffect::ObjectPage { generation, .. } => *generation,
    }
}

pub(super) fn token(effect: &RangePrepublicationEffect) -> RangePrepublicationCleanupToken {
    match effect {
        RangePrepublicationEffect::ValidateOwner(request) => request.cleanup,
        RangePrepublicationEffect::Page { cleanup, .. }
        | RangePrepublicationEffect::ObjectPage { cleanup, .. } => *cleanup,
    }
}

pub(super) fn copy_effect(effect: &RangePrepublicationEffect) -> RangePrepublicationEffect {
    match effect {
        RangePrepublicationEffect::ValidateOwner(request) => {
            RangePrepublicationEffect::ValidateOwner(*request)
        }
        RangePrepublicationEffect::Page {
            cleanup,
            generation,
            request,
        } => RangePrepublicationEffect::Page {
            cleanup: *cleanup,
            generation: *generation,
            request: *request,
        },
        RangePrepublicationEffect::ObjectPage {
            cleanup,
            generation,
            request,
        } => RangePrepublicationEffect::ObjectPage {
            cleanup: *cleanup,
            generation: *generation,
            request: *request,
        },
    }
}
