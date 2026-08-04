#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadBranchCapabilities {
    thread_fork: bool,
    thread_rollback: bool,
}

impl ThreadBranchCapabilities {
    pub fn new(thread_fork: bool, thread_rollback: bool) -> Self {
        Self {
            thread_fork,
            thread_rollback,
        }
    }

    pub fn thread_fork(&self) -> bool {
        self.thread_fork
    }

    pub fn thread_rollback(&self) -> bool {
        self.thread_rollback
    }

    pub fn thread_branching(&self) -> bool {
        self.thread_fork && self.thread_rollback
    }
}
