use super::super::thread_title::{ThreadTitleCandidate, TurnThreadTitleMode};

pub(super) fn automatic_thread_title_candidate(
    thread_id: &str,
    user_input: &str,
    title_mode: TurnThreadTitleMode,
) -> Option<ThreadTitleCandidate> {
    match title_mode {
        TurnThreadTitleMode::Disabled => None,
        TurnThreadTitleMode::AutomaticIfMissing => {
            if !automatic_thread_title_generation_is_eligible(true) {
                return None;
            }

            ThreadTitleCandidate::new(thread_id.to_string(), user_input.to_string())
        }
        TurnThreadTitleMode::BranchRetitleAfterFirstUserTurn => {
            ThreadTitleCandidate::new(thread_id.to_string(), user_input.to_string())
        }
    }
}

pub(crate) fn automatic_thread_title_generation_is_eligible(
    automatic_title_generation_allowed: bool,
) -> bool {
    automatic_title_generation_allowed
}
