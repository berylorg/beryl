use std::{fmt, time::Duration};

use beryl_backend::{
    FreshLoadedThreadSession, ManagedBackendSession, ThreadStartOptions, ThreadUnsubscribeResponse,
    TurnStartOptions, TurnStartResponse,
};

pub(crate) trait ThreadTitleBackend {
    type Error: fmt::Display;

    fn start_thread_with_options(
        &mut self,
        cwd: &std::path::Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, Self::Error>;

    fn start_turn_with_options(
        &mut self,
        thread_id: &str,
        text: &str,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> Result<TurnStartResponse, Self::Error>;

    fn unsubscribe_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadUnsubscribeResponse, Self::Error>;
}

impl ThreadTitleBackend for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn start_thread_with_options(
        &mut self,
        cwd: &std::path::Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, Self::Error> {
        ManagedBackendSession::start_thread_with_options(self, cwd, options, timeout)
    }

    fn start_turn_with_options(
        &mut self,
        thread_id: &str,
        text: &str,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> Result<TurnStartResponse, Self::Error> {
        ManagedBackendSession::start_turn_with_options(self, thread_id, text, options, timeout)
    }

    fn unsubscribe_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadUnsubscribeResponse, Self::Error> {
        ManagedBackendSession::unsubscribe_thread(self, thread_id, timeout)
    }
}
