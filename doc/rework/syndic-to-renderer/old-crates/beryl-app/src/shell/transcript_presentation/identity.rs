use super::super::execution_detail::TurnExecutionRecord;
use super::{TranscriptPresentationRow, TranscriptRowIdentity};

pub(super) fn stable_row_identity(turn: &TurnExecutionRecord) -> Option<TranscriptRowIdentity> {
    match (turn.thread_id.as_deref(), turn.turn_id.as_deref()) {
        (Some(thread_id), Some(turn_id)) => Some(TranscriptRowIdentity(format!(
            "thread:{thread_id}:turn:{turn_id}"
        ))),
        _ => None,
    }
}

pub(super) fn latest_user_prompt_anchor_in_rows(
    rows: &[TranscriptPresentationRow],
) -> Option<(usize, usize, String)> {
    rows.iter().enumerate().rev().find_map(|(index, row)| {
        user_prompt_anchor_text(row.turn.as_ref())
            .map(|(fragment_index, prompt)| (index, fragment_index, prompt))
    })
}

pub(super) fn user_prompt_anchor_text(turn: &TurnExecutionRecord) -> Option<(usize, String)> {
    turn.latest_user_input_fragment()
        .map(|(index, fragment)| (index, fragment.text.clone()))
}
