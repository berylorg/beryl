use beryl_home_store::DomainReader;

use crate::{codec::*, domain::SyndicDomain, error::SyndicValidationError};

use super::scan::{point, require, scan};

mod activity;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    activity::validate(reader)?;
    validate_image_label_origins(reader)
}

fn validate_image_label_origins(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut owner = None;
    let mut previous_end = 0_u64;
    scan::<ImageLabelOriginSpansFamily>(reader, |key, span| {
        if owner != Some(key.thread) {
            if let Some(previous) = owner {
                finish_span_owner(reader, previous, previous_end)?;
            }
            owner = Some(key.thread);
            let thread = require::<ThreadsFamily>(
                reader,
                &key.thread,
                "image-label origin-span thread is missing",
            )?;
            previous_end = thread.image_label_frontiers().inherited().get();
        }
        if key.thread != span.thread_id()
            || key.end_label != span.end_label()
            || span.start_label().get()
                != previous_end
                    .checked_add(1)
                    .ok_or(SyndicValidationError::Invariant(
                        "image-label origin-span frontier overflowed",
                    ))?
        {
            return invariant("image-label origin-span key or contiguous frontier disagrees");
        }
        validate_span_owner(reader, span)?;
        previous_end = span.end_label().get();
        Ok(())
    })?;
    if let Some(owner) = owner {
        finish_span_owner(reader, owner, previous_end)?;
    }
    scan::<ThreadsFamily>(reader, |_, thread| {
        let frontiers = thread.image_label_frontiers();
        if frontiers.current() > frontiers.inherited() {
            let end_label = crate::ImageLabelOrdinal::new(frontiers.current().get())
                .map_err(|_| SyndicValidationError::Invariant("image-label frontier is invalid"))?;
            if point::<ImageLabelOriginSpansFamily>(
                reader,
                &ImageLabelOriginSpanKey {
                    thread: thread.id(),
                    end_label,
                },
            )?
            .is_none()
            {
                return invariant("thread final image-label origin span is missing");
            }
        }
        Ok(())
    })
}

fn finish_span_owner(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: beryl_model::SyndicThreadId,
    observed_end: u64,
) -> Result<(), SyndicValidationError> {
    let thread =
        require::<ThreadsFamily>(reader, &owner, "image-label origin-span thread is missing")?;
    if observed_end != thread.image_label_frontiers().current().get() {
        return invariant("image-label origin spans disagree with the thread frontier");
    }
    Ok(())
}

fn validate_span_owner(
    reader: &DomainReader<'_, SyndicDomain>,
    span: &crate::ImageLabelOriginSpanRecord,
) -> Result<(), SyndicValidationError> {
    let (owner_thread, proof) = match span.admitted_owner() {
        crate::ImageLabelOriginOwner::AcceptedInput(id) => {
            let input = require::<AcceptedInputsFamily>(
                reader,
                &id,
                "image-label origin-span accepted input is missing",
            )?;
            (input.thread_id(), input.asset_reference_set())
        }
        crate::ImageLabelOriginOwner::CanonicalItem(id) => {
            let item = require::<CanonicalItemsFamily>(
                reader,
                &id,
                "image-label origin-span canonical item is missing",
            )?;
            let turn = require::<TurnsFamily>(
                reader,
                &item.turn_id(),
                "image-label origin-span source turn is missing",
            )?;
            (
                turn.origin_thread_id(),
                item.presentation().asset_reference_set(),
            )
        }
    };
    let proof = proof.ok_or(SyndicValidationError::Invariant(
        "image-label origin-span owner omitted its asset proof",
    ))?;
    if owner_thread != span.thread_id()
        || proof != span.asset_reference_set()
        || proof.summary().maximum_image_label() != Some(span.end_label())
    {
        return invariant("image-label origin-span owner evidence disagrees");
    }
    Ok(())
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
