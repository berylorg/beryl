#[path = "failure/common.rs"]
mod common;
#[path = "failure/dispatch.rs"]
mod dispatch;
#[path = "failure/publication.rs"]
mod publication;
#[path = "failure/source.rs"]
mod source;
#[path = "failure/target.rs"]
mod target;

pub fn run() {
    dispatch::cancellation_before_dispatch();
    source::revision_drift();
    source::read_failure();
    dispatch::raw_websocket_byte_cutoff();
    dispatch::checked_user_receiver_loss();
    publication::definitive_terminal_publication_failure();
    publication::ambiguous_terminal_publication();
    target::exact_target_abandonment();
}
