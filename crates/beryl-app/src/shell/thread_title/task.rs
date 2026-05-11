use std::sync::mpsc::{Receiver, TryRecvError};

use super::{ThreadTitleCancellation, ThreadTitleResult, ThreadTitleUpdate};

pub(crate) struct ThreadTitleTask {
    thread_id: String,
    cancellation: ThreadTitleCancellation,
    receiver: Receiver<ThreadTitleUpdate>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ThreadTitleTaskOutcome {
    Finished {
        thread_id: String,
        result: ThreadTitleResult,
    },
    Abandoned {
        thread_id: String,
    },
    Disconnected {
        thread_id: String,
    },
}

impl ThreadTitleTask {
    pub(crate) fn new(
        thread_id: String,
        cancellation: ThreadTitleCancellation,
        receiver: Receiver<ThreadTitleUpdate>,
    ) -> Self {
        Self {
            thread_id,
            cancellation,
            receiver,
        }
    }
}

pub(crate) fn poll_thread_title_tasks(
    tasks: &mut Vec<ThreadTitleTask>,
) -> Vec<ThreadTitleTaskOutcome> {
    let mut outcomes = Vec::new();
    let mut index = 0;
    while index < tasks.len() {
        match tasks[index].receiver.try_recv() {
            Ok(ThreadTitleUpdate::Finished { thread_id, result }) => {
                tasks.remove(index);
                outcomes.push(ThreadTitleTaskOutcome::Finished { thread_id, result });
            }
            Err(TryRecvError::Empty) => {
                index += 1;
            }
            Err(TryRecvError::Disconnected) => {
                let task = tasks.remove(index);
                outcomes.push(ThreadTitleTaskOutcome::Disconnected {
                    thread_id: task.thread_id,
                });
            }
        }
    }

    outcomes
}

pub(crate) fn cancel_all_thread_title_tasks(
    tasks: &mut Vec<ThreadTitleTask>,
) -> Vec<ThreadTitleTaskOutcome> {
    let cancelled = std::mem::take(tasks);
    cancel_thread_title_task_batch(cancelled)
}

pub(crate) fn cancel_thread_title_tasks_for_thread(
    tasks: &mut Vec<ThreadTitleTask>,
    thread_id: &str,
) -> Vec<ThreadTitleTaskOutcome> {
    let mut kept = Vec::with_capacity(tasks.len());
    let mut cancelled = Vec::new();
    for task in tasks.drain(..) {
        if task.thread_id == thread_id {
            cancelled.push(task);
        } else {
            kept.push(task);
        }
    }
    *tasks = kept;

    cancel_thread_title_task_batch(cancelled)
}

pub(crate) fn thread_title_task_active_for_thread(
    tasks: &[ThreadTitleTask],
    thread_id: &str,
) -> bool {
    tasks.iter().any(|task| task.thread_id == thread_id)
}

fn cancel_thread_title_task_batch(tasks: Vec<ThreadTitleTask>) -> Vec<ThreadTitleTaskOutcome> {
    tasks.into_iter().map(cancel_thread_title_task).collect()
}

fn cancel_thread_title_task(task: ThreadTitleTask) -> ThreadTitleTaskOutcome {
    match task.receiver.try_recv() {
        Ok(ThreadTitleUpdate::Finished { thread_id, result }) => {
            ThreadTitleTaskOutcome::Finished { thread_id, result }
        }
        Err(TryRecvError::Empty) => {
            task.cancellation.cancel();
            ThreadTitleTaskOutcome::Abandoned {
                thread_id: task.thread_id,
            }
        }
        Err(TryRecvError::Disconnected) => ThreadTitleTaskOutcome::Disconnected {
            thread_id: task.thread_id,
        },
    }
}
