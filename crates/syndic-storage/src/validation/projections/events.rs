use beryl_home_store::DomainReader;

use crate::validation::scan::{point, require, scan};
use crate::{codec::*, domain::SyndicDomain, error::SyndicValidationError};

use super::{invariant, items::validate_cas_turn_source};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut current_turn = None;
    let mut expected = 1_u64;
    let mut observed = 0_u64;
    let mut latest_end_status = None;
    let mut first_issue = None;
    scan::<SourceEventsFamily>(reader, |key, event| {
        if current_turn != Some(key.owner) {
            finish_turn_events(
                reader,
                current_turn,
                observed,
                latest_end_status,
                first_issue,
            )?;
            current_turn = Some(key.owner);
            expected = 1;
            observed = 0;
            latest_end_status = None;
            first_issue = None;
        }
        if key.owner != event.turn_id()
            || key.ordinal != event.sequence()
            || event.sequence().get() != expected
        {
            return invariant("source-event key or contiguous sequence disagrees");
        }
        require::<TurnsFamily>(
            reader,
            &event.turn_id(),
            "source event owner turn is missing",
        )?;
        if event.source().is_none()
            && !matches!(
                event.payload(),
                crate::SourceEventPayload::TurnEnded(status)
                    if status.outcome() != crate::TurnTerminalOutcome::Complete
            )
        {
            return invariant("source-less event claims external turn activity");
        }
        if let Some(source) = event.source() {
            validate_cas_turn_source(
                reader,
                event.turn_id(),
                source.thread_id(),
                source.turn_id(),
                "source event CAS-turn index is missing",
                "source event CAS-turn correlation disagrees",
            )?;
        }
        match event.payload() {
            crate::SourceEventPayload::TurnActivated => latest_end_status = None,
            crate::SourceEventPayload::ProviderObservationIssue(issue) => {
                if event.source() != Some(issue.source()) {
                    return invariant("provider-observation issue route disagrees with its event");
                }
                validate_issue_evidence(reader, issue)?;
                let conflict = classify_issue(reader, event.turn_id(), event.sequence(), issue)?;
                if conflict != Some(issue.reason()) {
                    return invariant(
                        "provider-observation issue reason disagrees with its source prefix",
                    );
                }
                first_issue = first_issue.or(Some(issue.reason()));
            }
            crate::SourceEventPayload::TurnEnded(status) => {
                if status.outcome() == crate::TurnTerminalOutcome::Complete {
                    crate::validation::compaction::reject_ordinary_complete_terminal(
                        reader,
                        event.turn_id(),
                    )?;
                }
                if first_issue.is_some()
                    && event.source().is_some()
                    && status.incomplete_reason()
                        != Some(crate::TurnIncompleteReason::CompletionMismatch)
                {
                    return invariant(
                        "source-backed terminal disagrees with provider-observation issue",
                    );
                }
                latest_end_status = Some(*status);
            }
            _ => {}
        }
        expected = expected
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "source-event sequence exhausted",
            ))?;
        observed += 1;
        Ok(())
    })?;
    finish_turn_events(
        reader,
        current_turn,
        observed,
        latest_end_status,
        first_issue,
    )?;
    scan::<TurnStatesFamily>(reader, |_, state| {
        let key = TurnEventKey {
            owner: state.turn_id(),
            ordinal: crate::SourceEventSequence::FIRST,
        };
        if (state.source_event_count() == 0) == point::<SourceEventsFamily>(reader, &key)?.is_some()
        {
            return invariant("turn source-event zero frontier disagrees");
        }
        let has_compaction_terminal_authority =
            crate::validation::compaction::validate_complete_terminal_authority(reader, &state)?;
        if state.lifecycle() == crate::TurnLifecycle::Complete
            && state.source_event_count() == 0
            && !has_compaction_terminal_authority
        {
            return invariant("successful turn completion lacks exact source authority");
        }
        if state.lifecycle().is_proven_terminal() && state.source_event_count() != 0 {
            let sequence =
                crate::SourceEventSequence::new(state.source_event_count()).map_err(|_| {
                    SyndicValidationError::Invariant("terminal event frontier is invalid")
                })?;
            let event = require::<SourceEventsFamily>(
                reader,
                &TurnEventKey {
                    owner: state.turn_id(),
                    ordinal: sequence,
                },
                "terminal turn final source event is missing",
            )?;
            if !matches!(
                event.payload(),
                crate::SourceEventPayload::TurnEnded(status)
                    if state.end_status() == Some(*status)
            ) {
                return invariant(
                    "proven-terminal turn does not end with its terminal source event",
                );
            }
        }
        Ok(())
    })
}

fn finish_turn_events(
    reader: &DomainReader<'_, SyndicDomain>,
    turn: Option<beryl_model::SyndicTurnId>,
    observed: u64,
    latest_end_status: Option<crate::TurnEndStatus>,
    first_issue: Option<crate::ProviderObservationIssueReason>,
) -> Result<(), SyndicValidationError> {
    let Some(turn) = turn else {
        return Ok(());
    };
    let state = require::<TurnStatesFamily>(reader, &turn, "event owner state is missing")?;
    if state.source_event_count() != observed
        || state.end_status() != latest_end_status
        || state.provider_observation_issue() != first_issue
    {
        return invariant("turn source-event frontier, issue, or exact end status disagrees");
    }
    Ok(())
}

fn validate_issue_evidence(
    reader: &DomainReader<'_, SyndicDomain>,
    issue: &crate::ProviderObservationIssue,
) -> Result<(), SyndicValidationError> {
    crate::validate_provider_observation_issue_evidence(reader, issue).map_err(issue_error)
}

fn classify_issue(
    reader: &DomainReader<'_, SyndicDomain>,
    turn: beryl_model::SyndicTurnId,
    sequence: crate::SourceEventSequence,
    issue: &crate::ProviderObservationIssue,
) -> Result<Option<crate::ProviderObservationIssueReason>, SyndicValidationError> {
    crate::classify_provider_observation_issue(reader, turn, sequence, issue).map_err(issue_error)
}

fn issue_error(error: crate::ProviderObservationIssueEvidenceError) -> SyndicValidationError {
    match error {
        crate::ProviderObservationIssueEvidenceError::Read(error) => error.into(),
        _ => SyndicValidationError::Invariant(
            "provider-observation issue evidence is not exact sealed authority",
        ),
    }
}
