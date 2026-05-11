pub(crate) enum ThreadTitleUpdate {
    Finished {
        thread_id: String,
        result: ThreadTitleResult,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ThreadTitleResult {
    Applied { title: String },
    Cancelled,
    Failed { message: String },
}
