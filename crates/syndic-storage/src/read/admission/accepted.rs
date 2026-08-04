use beryl_home_store::HomeStore;

use crate::{
    AcceptedInputAdmission, AcceptedInputRecord, AcceptedOrderIndexRecord,
    AcceptedRouteGenerationRecord, AcceptedRouteLeafRecord, InputAdmissionStatus, SyndicReadError,
    codec::*, domain::SyndicStorage,
};

use super::{super::SyndicPointReadLimit, draft_matches_admission};

impl SyndicStorage {
    /// Reconciles one non-idle admission through its durable immutable receipt.
    ///
    /// An accepted result is stable across later route, gate, lifecycle, and replacement-draft
    /// descendants because only the complete admission receipt and immutable order, leaf identity,
    /// and generation membership are authoritative. When no receipt exists, the read stabilizes
    /// the source draft natural identity and retries at most once before reporting concurrent
    /// change.
    pub fn accepted_input_status(
        &self,
        store: &HomeStore,
        admission: &AcceptedInputAdmission,
        limit: SyndicPointReadLimit,
    ) -> Result<InputAdmissionStatus, SyndicReadError> {
        if let Some(input) =
            self.point::<AcceptedInputsFamily>(store, admission.accepted_input_id(), limit)?
        {
            return self.reconcile_accepted_input(store, admission, &input, limit);
        }

        for attempt in 0..=1 {
            let draft_before = self.point::<DraftsFamily>(store, admission.draft_id(), limit)?;
            let turn =
                self.point::<TurnsFamily>(store, admission.draft_id().submitted_turn_id(), limit)?;
            let leaf = self.point::<AcceptedRouteLeavesFamily>(
                store,
                admission.accepted_input_id(),
                limit,
            )?;
            let draft_after = self.point::<DraftsFamily>(store, admission.draft_id(), limit)?;

            // The accepted-input natural identity is the commit marker. Read it last so an
            // admission consuming the source anchor resolves through its immutable receipt.
            if let Some(input) =
                self.point::<AcceptedInputsFamily>(store, admission.accepted_input_id(), limit)?
            {
                return self.reconcile_accepted_input(store, admission, &input, limit);
            }
            if draft_before != draft_after {
                if attempt == 0 {
                    continue;
                }
                return Err(SyndicReadError::ConcurrentChange {
                    operation: "accepted-input source reconciliation",
                });
            }
            return Ok(
                if draft_matches_admission(draft_after.as_ref(), admission)
                    && turn.is_none()
                    && leaf.is_none()
                {
                    InputAdmissionStatus::Absent
                } else {
                    InputAdmissionStatus::Collision
                },
            );
        }
        unreachable!("bounded accepted-input reconciliation loop returns")
    }

    fn reconcile_accepted_input(
        &self,
        store: &HomeStore,
        admission: &AcceptedInputAdmission,
        input: &AcceptedInputRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<InputAdmissionStatus, SyndicReadError> {
        let source_draft = self.point::<DraftsFamily>(store, admission.draft_id(), limit)?;
        let source_turn =
            self.point::<TurnsFamily>(store, admission.draft_id().submitted_turn_id(), limit)?;
        let leaf =
            self.point::<AcceptedRouteLeavesFamily>(store, admission.accepted_input_id(), limit)?;
        let order = self.point::<AcceptedOrderFamily>(
            store,
            ThreadAcceptedKey {
                owner: admission.thread_id(),
                ordinal: input.ordinal(),
            },
            limit,
        )?;
        let generation = self.point::<AcceptedRouteGenerationsFamily>(
            store,
            ThreadRouteKey {
                thread: admission.thread_id(),
                generation: input.route_generation(),
            },
            limit,
        )?;

        Ok(
            if source_draft.is_none()
                && source_turn.is_none()
                && exact_input_matches(admission, input)
                && order
                    .as_ref()
                    .is_some_and(|order| order_matches(input, order))
                && leaf
                    .as_ref()
                    .is_some_and(|leaf| leaf_identity_matches(input, leaf))
                && generation
                    .as_ref()
                    .is_some_and(|generation| generation_contains(input, generation))
            {
                InputAdmissionStatus::ExactAccepted
            } else {
                InputAdmissionStatus::Collision
            },
        )
    }
}

fn exact_input_matches(admission: &AcceptedInputAdmission, input: &AcceptedInputRecord) -> bool {
    let proof = input.admission();
    input.id() == admission.accepted_input_id()
        && input.thread_id() == admission.thread_id()
        && proof.expected_thread_revision() == admission.expected_thread_revision()
        && proof.source_draft_id() == admission.draft_id()
        && proof.expected_draft_revision() == admission.expected_draft_revision()
        && proof.expected_gate_revision() == admission.expected_gate_revision()
        && proof.replacement_draft_id() == admission.next_draft_id()
        && input.content() == admission.expected_content()
        && input.asset_reference_set() == admission.asset_reference_set()
        && input.admitted_at() == admission.admitted_at()
}

fn order_matches(input: &AcceptedInputRecord, order: &AcceptedOrderIndexRecord) -> bool {
    order
        == &AcceptedOrderIndexRecord::new(
            input.thread_id(),
            input.ordinal(),
            input.id(),
            input.route_generation(),
        )
}

fn leaf_identity_matches(input: &AcceptedInputRecord, leaf: &AcceptedRouteLeafRecord) -> bool {
    leaf.input_id() == input.id()
        && leaf.thread_id() == input.thread_id()
        && leaf.generation() == input.route_generation()
        && leaf.ordinal() == input.ordinal()
}

fn generation_contains(
    input: &AcceptedInputRecord,
    generation: &AcceptedRouteGenerationRecord,
) -> bool {
    generation.thread_id() == input.thread_id()
        && generation.generation() == input.route_generation()
        && generation
            .first_ordinal()
            .zip(generation.last_ordinal())
            .is_some_and(|(first, last)| {
                first.get() <= input.ordinal().get() && input.ordinal().get() <= last.get()
            })
}
