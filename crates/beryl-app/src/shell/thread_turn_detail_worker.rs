use std::{
    collections::{BTreeSet, VecDeque},
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use beryl_backend::{ManagedBackendClientConnector, TurnInfo, TurnItemsView};
use beryl_model::workspace::{BerylWorkspaceId, RuntimeMode};

use super::{
    execution_detail::TranscriptImagePathResolver,
    transcript_history::{
        TranscriptTurnDetailLoadTicket, load_thread_turn_detail_from_history_page,
    },
    transcript_image_sources::transcript_image_path_resolver_for_turns,
};
use crate::{
    BerylWorkspacePersistence,
    diagnostic_dynamic_tools::{TranscriptDetailLoadEvent, diagnostic_duration_micros},
};

pub(super) struct TranscriptTurnDetailTask {
    request_sender: Sender<TranscriptTurnDetailRequest>,
    update_receiver: Receiver<TranscriptTurnDetailUpdate>,
    pending_tickets: VecDeque<TranscriptTurnDetailLoadTicket>,
    in_flight_ticket: Option<TranscriptTurnDetailLoadTicket>,
}

pub(super) enum TranscriptTurnDetailUpdate {
    DetailsLoaded {
        ticket: TranscriptTurnDetailLoadTicket,
        turns: Vec<TurnInfo>,
        diagnostics: TranscriptDetailLoadEvent,
    },
    Finished(TranscriptTurnDetailOutcome),
}

pub(super) enum TranscriptTurnDetailOutcome {
    Loaded {
        ticket: TranscriptTurnDetailLoadTicket,
        turns: Vec<TurnInfo>,
        image_resolver: TranscriptImagePathResolver,
        diagnostics: TranscriptDetailLoadEvent,
    },
    Failed {
        ticket: TranscriptTurnDetailLoadTicket,
        message: String,
        diagnostics: TranscriptDetailLoadEvent,
    },
}

enum TranscriptTurnDetailRequest {
    Load(TranscriptTurnDetailLoadTicket),
    ResolveImages {
        ticket: TranscriptTurnDetailLoadTicket,
        turns: Vec<TurnInfo>,
        diagnostics: TranscriptDetailLoadEvent,
    },
}

pub(super) fn spawn_transcript_turn_detail_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    workspace_id: BerylWorkspaceId,
    runtime_mode: RuntimeMode,
    timeout: Duration,
) -> TranscriptTurnDetailTask {
    let (request_sender, request_receiver) = mpsc::channel();
    let (update_sender, update_receiver) = mpsc::channel();
    thread::spawn(move || {
        run_worker(
            persistence,
            connector,
            workspace_id,
            runtime_mode,
            timeout,
            request_receiver,
            update_sender,
        )
    });
    TranscriptTurnDetailTask {
        request_sender,
        update_receiver,
        pending_tickets: VecDeque::new(),
        in_flight_ticket: None,
    }
}

impl TranscriptTurnDetailTask {
    pub(super) fn request(&mut self, ticket: TranscriptTurnDetailLoadTicket) -> bool {
        self.pending_tickets.push_back(ticket);
        true
    }

    pub(super) fn try_recv(&self) -> Result<TranscriptTurnDetailUpdate, TryRecvError> {
        self.update_receiver.try_recv()
    }

    pub(super) fn pop_pending_ticket(&mut self) -> Option<TranscriptTurnDetailLoadTicket> {
        if self.in_flight_ticket.is_some() {
            return None;
        }
        self.pending_tickets.pop_front()
    }

    pub(super) fn start_ticket(&mut self, ticket: TranscriptTurnDetailLoadTicket) -> bool {
        if self.in_flight_ticket.is_some() {
            self.pending_tickets.push_front(ticket);
            return true;
        }
        let request = TranscriptTurnDetailRequest::Load(ticket.clone());
        if self.request_sender.send(request).is_err() {
            return false;
        }
        self.in_flight_ticket = Some(ticket);
        true
    }

    pub(super) fn resolve_images_for_loaded_turns(
        &mut self,
        ticket: &TranscriptTurnDetailLoadTicket,
        turns: Vec<TurnInfo>,
        diagnostics: TranscriptDetailLoadEvent,
    ) -> bool {
        if self.in_flight_ticket.as_ref() != Some(ticket) {
            return false;
        }
        self.request_sender
            .send(TranscriptTurnDetailRequest::ResolveImages {
                ticket: ticket.clone(),
                turns,
                diagnostics,
            })
            .is_ok()
    }

    pub(super) fn finish_ticket(&mut self, ticket: &TranscriptTurnDetailLoadTicket) {
        if self.in_flight_ticket.as_ref() == Some(ticket) {
            self.in_flight_ticket = None;
        }
        self.pending_tickets
            .retain(|pending_ticket| pending_ticket != ticket);
    }

    pub(super) fn take_active_tickets(&mut self) -> Vec<TranscriptTurnDetailLoadTicket> {
        let mut tickets = Vec::new();
        if let Some(ticket) = self.in_flight_ticket.take() {
            tickets.push(ticket);
        }
        tickets.extend(self.pending_tickets.drain(..));
        tickets
    }

    pub(super) fn has_active_tickets(&self) -> bool {
        self.in_flight_ticket.is_some() || !self.pending_tickets.is_empty()
    }

    pub(super) fn active_ticket_count(&self) -> usize {
        self.pending_tickets
            .len()
            .saturating_add(usize::from(self.in_flight_ticket.is_some()))
    }
}

fn detail_load_event(
    ticket: &TranscriptTurnDetailLoadTicket,
    returned_turn_count: usize,
    cas_micros: u64,
    response_processing_micros: u64,
    total_micros: u64,
    outcome: &'static str,
) -> TranscriptDetailLoadEvent {
    TranscriptDetailLoadEvent {
        sequence: 0,
        cursor_present: ticket
            .page_locator()
            .and_then(|locator| locator.cursor())
            .is_some(),
        requested_limit: ticket.page_locator().map(|locator| locator.limit()),
        returned_turn_count,
        applied_turn_count: 0,
        skipped_stale_count: 0,
        total_micros,
        cas_micros,
        response_processing_micros,
        image_source_resolution_micros: 0,
        cache_application_micros: 0,
        outcome: outcome.to_string(),
    }
}

fn run_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    workspace_id: BerylWorkspaceId,
    runtime_mode: RuntimeMode,
    timeout: Duration,
    request_receiver: Receiver<TranscriptTurnDetailRequest>,
    update_sender: Sender<TranscriptTurnDetailUpdate>,
) {
    let mut session = None;

    while let Ok(request) = request_receiver.recv() {
        let request_started = Instant::now();

        if session.is_none() {
            match connector.connect_client(timeout) {
                Ok(connected) => {
                    session = Some(connected);
                }
                Err(error) => {
                    let ticket = match request {
                        TranscriptTurnDetailRequest::Load(ticket)
                        | TranscriptTurnDetailRequest::ResolveImages { ticket, .. } => ticket,
                    };
                    let diagnostics = detail_load_event(
                        &ticket,
                        0,
                        0,
                        0,
                        diagnostic_duration_micros(request_started.elapsed()),
                        "failedWorker",
                    );
                    let _ = update_sender.send(TranscriptTurnDetailUpdate::Finished(
                        TranscriptTurnDetailOutcome::Failed {
                            ticket,
                            message: format!(
                                "Beryl could not connect to the managed backend: {error}"
                            ),
                            diagnostics,
                        },
                    ));
                    continue;
                }
            }
        }

        let session = session
            .as_mut()
            .expect("session is connected before loading turn details");

        match request {
            TranscriptTurnDetailRequest::Load(ticket) => {
                let detail_items = match ticket.page_locator().cloned() {
                    Some(page_locator) => load_thread_turn_detail_from_history_page(
                        session,
                        ticket.thread_id(),
                        ticket.turn_id(),
                        &page_locator,
                        timeout,
                    )
                    .map_err(|error| {
                        (
                            format!("Beryl could not load turn details: {error}"),
                            detail_load_event(
                                &ticket,
                                error.returned_turn_count(),
                                error.cas_micros(),
                                error.response_processing_micros(),
                                diagnostic_duration_micros(request_started.elapsed()),
                                "failedWorker",
                            ),
                        )
                    }),
                    None => Err((
                        "Beryl could not load turn details: the visible row is not a history skeleton"
                            .to_string(),
                        detail_load_event(
                            &ticket,
                            0,
                            0,
                            0,
                            diagnostic_duration_micros(request_started.elapsed()),
                            "failedWorker",
                        ),
                    )),
                };
                match detail_items {
                    Ok(detail_load) => {
                        let coalesced_turn_ids = ticket
                            .coalesced_turn_ids()
                            .iter()
                            .map(String::as_str)
                            .collect::<BTreeSet<_>>();
                        let detail_turns = detail_load
                            .turns
                            .into_iter()
                            .filter(|turn| {
                                coalesced_turn_ids.contains(turn.id.as_str())
                                    && turn.items_view == TurnItemsView::Full
                            })
                            .collect::<Vec<_>>();
                        let diagnostics = detail_load_event(
                            &ticket,
                            detail_load.returned_turn_count,
                            detail_load.cas_micros,
                            detail_load.response_processing_micros,
                            diagnostic_duration_micros(request_started.elapsed()),
                            "loadedWorker",
                        );
                        if update_sender
                            .send(TranscriptTurnDetailUpdate::DetailsLoaded {
                                ticket,
                                turns: detail_turns,
                                diagnostics,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err((message, diagnostics)) => {
                        if update_sender
                            .send(TranscriptTurnDetailUpdate::Finished(
                                TranscriptTurnDetailOutcome::Failed {
                                    ticket,
                                    message,
                                    diagnostics,
                                },
                            ))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
            TranscriptTurnDetailRequest::ResolveImages {
                ticket,
                turns,
                mut diagnostics,
            } => {
                let image_resolution_started = Instant::now();
                let image_resolver = transcript_image_path_resolver_for_turns(
                    &persistence,
                    &workspace_id,
                    &runtime_mode,
                    &turns,
                    session,
                    timeout,
                )
                .unwrap_or_default();
                diagnostics.image_source_resolution_micros =
                    diagnostic_duration_micros(image_resolution_started.elapsed());
                diagnostics.total_micros = diagnostics
                    .total_micros
                    .saturating_add(diagnostics.image_source_resolution_micros);
                if update_sender
                    .send(TranscriptTurnDetailUpdate::Finished(
                        TranscriptTurnDetailOutcome::Loaded {
                            ticket,
                            turns,
                            image_resolver,
                            diagnostics,
                        },
                    ))
                    .is_err()
                {
                    return;
                }
            }
        }
    }
}
