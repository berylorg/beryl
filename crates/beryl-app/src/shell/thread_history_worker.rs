use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use beryl_backend::ManagedBackendClientConnector;
use beryl_model::workspace::{BerylWorkspaceId, RuntimeMode};

use super::{
    execution_detail::TranscriptImagePathResolver,
    transcript_history::{
        LoadedTranscriptHistoryPage, TranscriptHistoryPageRequest, load_thread_history_page,
    },
    transcript_image_sources::transcript_image_path_resolver_for_turns,
};
use crate::BerylWorkspacePersistence;

pub(super) enum ThreadHistoryPageUpdate {
    Finished(ThreadHistoryPageOutcome),
}

pub(super) enum ThreadHistoryPageOutcome {
    Loaded {
        thread_id: String,
        request: TranscriptHistoryPageRequest,
        page: LoadedTranscriptHistoryPage,
        image_resolver: TranscriptImagePathResolver,
    },
    Failed {
        thread_id: String,
        message: String,
    },
}

pub(super) fn spawn_older_thread_history_page_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    workspace_id: BerylWorkspaceId,
    runtime_mode: RuntimeMode,
    thread_id: String,
    request: TranscriptHistoryPageRequest,
    timeout: Duration,
) -> Receiver<ThreadHistoryPageUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        run_thread_history_page_worker(
            persistence,
            connector,
            workspace_id,
            runtime_mode,
            thread_id,
            request,
            timeout,
            sender,
        )
    });
    receiver
}

fn run_thread_history_page_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    workspace_id: BerylWorkspaceId,
    runtime_mode: RuntimeMode,
    thread_id: String,
    request: TranscriptHistoryPageRequest,
    timeout: Duration,
    sender: mpsc::Sender<ThreadHistoryPageUpdate>,
) {
    let mut session = match connector.connect_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            let _ = sender.send(ThreadHistoryPageUpdate::Finished(
                ThreadHistoryPageOutcome::Failed {
                    thread_id,
                    message: format!("Beryl could not connect to the managed backend: {error}"),
                },
            ));
            return;
        }
    };

    let outcome =
        match load_thread_history_page(&mut session, &thread_id, request.cursor(), timeout) {
            Ok(page) => {
                let image_resolver = match transcript_image_path_resolver_for_turns(
                    &persistence,
                    &workspace_id,
                    &runtime_mode,
                    &page.turns,
                    &mut session,
                    timeout,
                ) {
                    Ok(resolver) => resolver,
                    Err(_) => TranscriptImagePathResolver::default(),
                };
                ThreadHistoryPageOutcome::Loaded {
                    thread_id,
                    request,
                    page,
                    image_resolver,
                }
            }
            Err(error) => ThreadHistoryPageOutcome::Failed {
                thread_id,
                message: format!("Beryl could not load thread history: {error}"),
            },
        };
    let _ = sender.send(ThreadHistoryPageUpdate::Finished(outcome));
}
