use std::{fmt, time::Duration};

use beryl_backend::{
    ManagedBackendSession, ThreadSessionResponse, ThreadStartOptions, ThreadUnsubscribeResponse,
    TurnStartOptions, TurnStartResponse, TurnStreamEvent,
};

pub(crate) trait ThreadTitleBackend {
    type Error: fmt::Display;

    fn start_thread_with_options(
        &mut self,
        cwd: &std::path::Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error>;

    fn start_turn_with_options(
        &mut self,
        thread_id: &str,
        text: &str,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> Result<TurnStartResponse, Self::Error>;

    fn next_turn_stream_event(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<Option<TurnStreamEvent>, Self::Error>;

    fn set_thread_name(
        &mut self,
        thread_id: &str,
        name: &str,
        timeout: Duration,
    ) -> Result<(), Self::Error>;

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
    ) -> Result<ThreadSessionResponse, Self::Error> {
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

    fn next_turn_stream_event(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<Option<TurnStreamEvent>, Self::Error> {
        ManagedBackendSession::next_turn_stream_event(self, idle_timeout)
    }

    fn set_thread_name(
        &mut self,
        thread_id: &str,
        name: &str,
        timeout: Duration,
    ) -> Result<(), Self::Error> {
        ManagedBackendSession::set_thread_name(self, thread_id, name, timeout)
    }

    fn unsubscribe_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadUnsubscribeResponse, Self::Error> {
        ManagedBackendSession::unsubscribe_thread(self, thread_id, timeout)
    }
}
