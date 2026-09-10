use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Duration;

use super::*;

pub struct IdleSessionElectionPause {
    entered: Receiver<()>,
    release: SyncSender<()>,
}

pub(super) struct IdleSessionElectionHook {
    thread_id: SyndicThreadId,
    entered: SyncSender<()>,
    release: Receiver<()>,
}

impl IdleSessionElectionPause {
    pub fn wait(&self, timeout: Duration) {
        self.entered
            .recv_timeout(timeout)
            .expect("idle election was not reached");
    }

    pub fn release(self) {
        let _ = self.release.try_send(());
    }
}

impl Drop for IdleSessionElectionPause {
    fn drop(&mut self) {
        let _ = self.release.try_send(());
    }
}

impl ScheduledExecutionSessions {
    pub fn install_idle_election_pause_for_test(
        &self,
        thread_id: SyndicThreadId,
    ) -> IdleSessionElectionPause {
        let (entered, observed) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let mut state = self.lock();
        assert!(state.idle_election_pause.is_none());
        state.idle_election_pause = Some(IdleSessionElectionHook {
            thread_id,
            entered,
            release: released,
        });
        IdleSessionElectionPause {
            entered: observed,
            release,
        }
    }

    pub(super) fn pause_idle_election_for_test(&self, thread_id: SyndicThreadId) {
        let hook = {
            let mut state = self.lock();
            if !state
                .idle_election_pause
                .as_ref()
                .is_some_and(|hook| hook.thread_id == thread_id)
            {
                return;
            }
            state.idle_election_pause.take().unwrap()
        };
        let _ = hook.entered.try_send(());
        let _ = hook.release.recv();
    }
}
