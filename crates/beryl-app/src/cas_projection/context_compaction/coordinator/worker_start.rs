use std::{io, thread::JoinHandle};

use super::{COMPACTION_WORKER_CAPACITY, ContextCompactionError, join_all_workers};

pub(super) fn start_workers(
    mut spawn: impl FnMut(usize) -> io::Result<JoinHandle<()>>,
    stop: impl FnOnce(),
) -> Result<Vec<JoinHandle<()>>, ContextCompactionError> {
    let mut workers = Vec::with_capacity(COMPACTION_WORKER_CAPACITY);
    for index in 0..COMPACTION_WORKER_CAPACITY {
        match spawn(index) {
            Ok(worker) => workers.push(worker),
            Err(_) => {
                stop();
                join_all_workers(workers);
                return Err(ContextCompactionError::Unavailable);
            }
        }
    }
    Ok(workers)
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/context_compaction_worker_start.rs"
    ));
}
