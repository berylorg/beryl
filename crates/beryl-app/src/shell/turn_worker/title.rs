use super::super::thread_title::ThreadTitleCandidate;

pub(super) fn automatic_thread_title_candidate(
    thread_id: &str,
    user_input: &str,
    automatic_title_generation_allowed: bool,
    backend_thread_name: Option<&str>,
) -> Option<ThreadTitleCandidate> {
    if !automatic_thread_title_generation_is_eligible(
        automatic_title_generation_allowed,
        backend_thread_name,
    ) {
        return None;
    }

    ThreadTitleCandidate::new(thread_id.to_string(), user_input.to_string())
}

pub(crate) fn automatic_thread_title_generation_is_eligible(
    automatic_title_generation_allowed: bool,
    backend_thread_name: Option<&str>,
) -> bool {
    automatic_title_generation_allowed
        && normalized_backend_thread_name(backend_thread_name).is_none()
}

fn normalized_backend_thread_name(name: Option<&str>) -> Option<&str> {
    let name = name?.trim();
    (!name.is_empty()).then_some(name)
}
