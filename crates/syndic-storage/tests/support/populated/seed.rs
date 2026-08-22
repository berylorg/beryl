use super::*;

pub(super) fn provider_command_owned(record: &FixtureRecord) -> bool {
    let source = source_turn();
    let active = active_turn();
    let source_thread = id(30);
    let active_thread = id(40);
    let item = source_item();
    let active_item = active_item();
    let suffix_item = suffix_item();
    let build_item = build_item();
    let activity_item = activity_item();
    let provider_item = |value: SyndicItemId| {
        value == item
            || value == active_item
            || value == suffix_item
            || value == build_item
            || value == activity_item
    };
    let provider_content =
        |value: SyndicContentId| provider_item(SyndicItemId::from_bytes(*value.as_bytes()));
    let provider_projection = |value: SyndicProjectionId| {
        value == source_projection()
            || value == source_resource_projection()
            || value == active_projection()
            || value == suffix_projection()
    };
    match record {
        FixtureRecord::ContentManifest(record) => record.owner().is_some_and(provider_item),
        FixtureRecord::ContentChunk(record) => provider_content(record.content_id()),
        FixtureRecord::ContentByteSpan(record) => provider_content(record.content_id()),
        FixtureRecord::ProviderNarrativeSpan(record) => provider_content(record.content_id()),
        FixtureRecord::TurnState(record) => {
            record.turn_id() == source || record.turn_id() == active
        }
        FixtureRecord::InputGate(record) => {
            record.thread_id() == source_thread || record.thread_id() == active_thread
        }
        FixtureRecord::HistorySummary(record) => {
            record.thread_id() == source_thread || record.thread_id() == active_thread
        }
        FixtureRecord::SourceEvent(record) => {
            record.turn_id() == source || record.turn_id() == active
        }
        FixtureRecord::CanonicalItem(record) => provider_item(record.id()),
        FixtureRecord::TurnItem(record) => record.turn_id() == source || record.turn_id() == active,
        FixtureRecord::ItemSourceEvent(record) => provider_item(record.item_id()),
        FixtureRecord::CasItem(record) => provider_item(record.item_id()),
        FixtureRecord::ItemProjectionHead(record) => provider_item(record.item_id()),
        FixtureRecord::ItemProjectionSet(record) => provider_item(record.item_id()),
        FixtureRecord::ItemProjectionBuild(record) => provider_item(record.item_id()),
        FixtureRecord::StableItemProjection(record) => provider_item(record.item_id()),
        FixtureRecord::ItemProjection(record) => provider_item(record.item_id()),
        FixtureRecord::Projection(record) => provider_item(record.item_id()),
        FixtureRecord::Resource(record) => provider_item(record.item_id()),
        FixtureRecord::ProjectionResource(record) => provider_projection(record.projection_id()),
        FixtureRecord::TranscriptViewHead(record) => {
            record.thread_id() == source_thread || record.thread_id() == active_thread
        }
        FixtureRecord::TranscriptBuild(record) => {
            record.thread_id() == source_thread || record.thread_id() == active_thread
        }
        FixtureRecord::TranscriptPathTurn(record) => {
            record.thread_id() == source_thread || record.thread_id() == active_thread
        }
        FixtureRecord::TranscriptViewEntry(record) => {
            record.thread_id() == source_thread || record.thread_id() == active_thread
        }
        FixtureRecord::ActivityQueryHead(record) => {
            record.thread_id() == source_thread || record.thread_id() == active_thread
        }
        FixtureRecord::ActivityQueryEntry(record) => {
            record.thread_id() == source_thread || record.thread_id() == active_thread
        }
        FixtureRecord::ActivityQuerySource(record) => {
            record.thread_id() == source_thread || record.thread_id() == active_thread
        }
        FixtureRecord::CasThread(record) => record.thread_id() == source_thread,
        FixtureRecord::CasThreadBinding(record) => record.thread_id() == source_thread,
        _ if active_route_fact(record) => true,
        _ => false,
    }
}

pub(super) fn active_route_fact(record: &FixtureRecord) -> bool {
    let thread = id(40);
    match record {
        FixtureRecord::InputGate(record) => record.thread_id() == thread,
        FixtureRecord::AcceptedInput(record) => record.thread_id() == thread,
        FixtureRecord::ImageLabelOriginSpan(record) => record.thread_id() == thread,
        FixtureRecord::AcceptedOrder(record) => record.thread_id() == thread,
        FixtureRecord::AcceptedRouteGenerationHead(record) => record.thread_id() == thread,
        FixtureRecord::AcceptedRouteGeneration(record) => record.thread_id() == thread,
        FixtureRecord::AcceptedReadySource(record) => record.thread_id() == thread,
        FixtureRecord::AcceptedRouteLeaf(record) => record.thread_id() == thread,
        FixtureRecord::AcceptedNextSource(record) => record.thread_id() == thread,
        _ => false,
    }
}

fn deferred_context_record(record: &FixtureRecord) -> bool {
    let thread = id(36);
    let owner = DiscussionContextOwnerId::Draft(draft_id(37));
    match record {
        FixtureRecord::Thread(record) => record.id() == thread,
        FixtureRecord::ThreadExecution(record) => record.thread_id() == thread,
        FixtureRecord::ThreadAttributes(record) => record.thread_id() == thread,
        FixtureRecord::ThreadUsage(record) => record.thread_id() == thread,
        FixtureRecord::ThreadCatalogSummary(record) => record.thread_id() == thread,
        FixtureRecord::Draft(record) => record.id() == draft_id(37),
        FixtureRecord::ContextEnvelope(record) => record.owner() == owner,
        FixtureRecord::InputGate(record) => record.thread_id() == thread,
        FixtureRecord::ActivityQueryHead(record) => record.thread_id() == thread,
        FixtureRecord::TranscriptViewHead(record) => record.thread_id() == thread,
        FixtureRecord::TranscriptBuild(record) => record.thread_id() == thread,
        FixtureRecord::HistorySummary(record) => record.thread_id() == thread,
        FixtureRecord::Binding(record) => record.thread_id() == thread,
        FixtureRecord::DraftByThread(record) => record.thread_id() == thread,
        FixtureRecord::ThreadParent(record) => record.child_thread_id() == thread,
        FixtureRecord::BindingHead(record) => record.thread_id() == thread,
        _ => false,
    }
}

fn deferred_context_records() -> Vec<FixtureRecord> {
    context::records()
        .into_iter()
        .filter(deferred_context_record)
        .collect()
}

pub fn seed_provider_records(store: &beryl_home_store::HomeStore, storage: SyndicStorage) {
    seed_canonical_empty_thread(store, storage, id(30), draft_id(31));
    seed_canonical_empty_thread(store, storage, id(40), draft_id(41));
    seed_canonical_empty_thread(store, storage, id(36), draft_id(37));
    let mut initial_builds = FixtureBatch::new();
    for thread in [id(30), id(40)] {
        initial_builds
            .delete(FixtureDelete::TranscriptBuild {
                thread,
                generation: TranscriptGeneration::FIRST,
            })
            .unwrap();
    }
    commit(store, storage, initial_builds);
    commit(store, storage, batch(pre_command_static_records()));
    commit(store, storage, batch(pre_event_records()));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .expect("pre-event populated fixture must be domain-valid");
    let mut receipts = Vec::new();
    let source_thread = id(30);
    let source_turn = source_turn();
    let source_authority = CasTurnSource::new(source_cas_thread(), source_cas_turn());
    let source_selected = SelectedPathProof::new(
        Some(source_turn),
        ThreadRevision::new(1).unwrap(),
        child_turn_chain_digest(
            source_turn,
            SyndicTurnId::from_bytes([29; 16]),
            root_turn_chain_digest(SyndicTurnId::from_bytes([29; 16])),
        ),
    );
    accept_clean(
        store.execute_current(storage.current_activate_binding(ActivateBinding::new(
            source_thread,
            BindingRevision::new(2).unwrap(),
            InputGateRevision::new(1).unwrap(),
            source_selected,
            source_snapshot(),
            source_turn,
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(1).unwrap(),
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
            timestamp(3),
        ))),
        "source binding activation",
        &mut receipts,
    );
    accept_clean(
        store.execute_current(
            storage.current_publish_active_cas_turn(PublishActiveCasTurn::new(
                source_thread,
                BindingRevision::new(3).unwrap(),
                InputGateRevision::new(2).unwrap(),
                source_snapshot(),
                source_cas_thread(),
                source_cas_turn(),
                timestamp(3),
            )),
        ),
        "source CAS-turn publication",
        &mut receipts,
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .expect("first provider event must begin from domain-valid state");
    let mut source_seed = ProviderSeedTurn {
        thread: source_thread,
        turn: source_turn,
        source: source_authority.clone(),
        state_revision: TurnStateRevision::FIRST,
        gate_revision: InputGateRevision::new(3).unwrap(),
        observed_at: timestamp(4),
    };
    accept_clean(
        store.execute_current(
            storage.current_admit_live_source_event(
                LiveSourceEvent::new(
                    source_thread,
                    source_turn,
                    source_seed.state_revision,
                    source_seed.gate_revision,
                    SourceEventSequence::FIRST,
                    Some(source_authority),
                    SourceEventPayload::TurnActivated,
                    source_seed.observed_at,
                )
                .unwrap(),
            ),
        ),
        "source-turn activation",
        &mut receipts,
    );
    source_seed.state_revision = source_seed.state_revision.checked_next().unwrap();
    source_provider_fixture().seed(store, &storage, &mut source_seed, &mut receipts);
    accept_clean(
        store.execute_current(
            storage.current_admit_live_source_event(
                LiveSourceEvent::new(
                    source_thread,
                    source_turn,
                    source_seed.state_revision,
                    source_seed.gate_revision,
                    SourceEventSequence::new(5).unwrap(),
                    Some(source_seed.source.clone()),
                    SourceEventPayload::TurnEnded(
                        TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                    ),
                    source_seed.observed_at,
                )
                .unwrap(),
            ),
        ),
        "source-turn terminal event",
        &mut receipts,
    );
    source_seed.state_revision = source_seed.state_revision.checked_next().unwrap();
    source_seed.gate_revision = source_seed.gate_revision.checked_next().unwrap();
    provider::converge_transcript(store, &storage, source_thread, &mut receipts);
    accept_clean(
        store.execute_current(
            storage.current_freeze_next_turn_item(FreezeNextTurnItem::new(
                source_thread,
                source_turn,
                source_seed.state_revision,
                TurnItemOrdinal::FIRST,
                source_item(),
                source_seed.observed_at,
            )),
        ),
        "source-item freeze",
        &mut receipts,
    );
    source_seed.state_revision = source_seed.state_revision.checked_next().unwrap();
    provider::converge_item_projection(store, &storage, source_item(), &mut receipts);
    accept_clean(
        store.execute_current(
            storage.current_finalize_next_turn_item(FinalizeNextTurnItem::new(
                source_thread,
                source_turn,
                source_seed.state_revision,
                TurnItemOrdinal::FIRST,
                source_item(),
                source_seed.observed_at,
            )),
        ),
        "source-item finalization",
        &mut receipts,
    );
    provider::converge_transcript(store, &storage, source_thread, &mut receipts);
    commit(store, storage, batch(deferred_context_records()));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .expect("deferred discussion-context fixture must be domain-valid");
    active::seed_provider_records(store, &storage, &mut receipts);
    assert!(
        !receipts.is_empty(),
        "provider fixture issued no durable commands"
    );
}

fn pre_command_static_records() -> Vec<FixtureRecord> {
    let source = source_turn();
    let active = active_turn();
    let root = SyndicTurnId::from_bytes([29; 16]);
    populated_records()
        .into_iter()
        .filter(|record| {
            if deferred_context_record(record) {
                return false;
            }
            match record {
                FixtureRecord::Thread(record) => record.id() != id(30) && record.id() != id(40),
                FixtureRecord::Turn(record) => {
                    record.id() != root && record.id() != source && record.id() != active
                }
                FixtureRecord::TurnState(record) => record.turn_id() != root,
                FixtureRecord::SourceEvent(record) => record.turn_id() != root,
                FixtureRecord::Binding(record) => {
                    (record.thread_id() != id(30) && record.thread_id() != id(40))
                        || record.revision().get() < BindingRevision::new(3).unwrap().get()
                }
                FixtureRecord::ExecutionSnapshot(_)
                | FixtureRecord::ActiveCasTurn(_)
                | FixtureRecord::CasTurn(_) => false,
                FixtureRecord::TranscriptViewHead(record) => {
                    record.thread_id() != id(30) && record.thread_id() != id(40)
                }
                FixtureRecord::BindingHead(record) => {
                    record.thread_id() != id(30) && record.thread_id() != id(40)
                }
                _ => true,
            }
        })
        .collect()
}

pub fn seed_populated(store: &beryl_home_store::HomeStore, storage: SyndicStorage) {
    seed_provider_records(store, storage);
}

fn source_provider_fixture() -> ProviderItemFixture {
    agent_item_fixture(
        source_item(),
        source_turn(),
        CasItemSource::new(
            CasTurnSource::new(source_cas_thread(), source_cas_turn()),
            source_cas_item(),
        ),
        SourceEventSequence::new(2).unwrap(),
        ProviderMessagePhaseV1::FinalAnswer,
        "assistant",
        AgentItemFixtureState::Finalized,
    )
}

fn pre_event_records() -> Vec<FixtureRecord> {
    let source_thread = id(30);
    let active_thread = id(40);
    let root = SyndicTurnId::from_bytes([29; 16]);
    let source = source_turn();
    let active = active_turn();
    let mut records = populated_records()
        .into_iter()
        .filter(|record| match record {
            FixtureRecord::Thread(record) => {
                record.id() == source_thread || record.id() == active_thread
            }
            FixtureRecord::Turn(record) => {
                record.id() == root || record.id() == source || record.id() == active
            }
            FixtureRecord::TurnState(record) => record.turn_id() == root,
            FixtureRecord::SourceEvent(record) => record.turn_id() == root,
            FixtureRecord::Binding(record) => {
                record.thread_id() == active_thread
                    && record.revision() == BindingRevision::new(3).unwrap()
            }
            FixtureRecord::CasTurn(record) => record.thread_id() == active_thread,
            FixtureRecord::ExecutionSnapshot(record) => record.thread_id() == active_thread,
            FixtureRecord::ActiveCasTurn(record) => record.thread_id() == active_thread,
            FixtureRecord::TranscriptViewHead(_) => false,
            FixtureRecord::BindingHead(record) => record.thread_id() == active_thread,
            _ => false,
        })
        .collect::<Vec<_>>();
    records.extend(active::records().into_iter().filter(active_route_fact));
    records.extend([
        FixtureRecord::TurnState(fixture_turn_state(
            source,
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            0,
            timestamp(4),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            active,
            TurnStateRevision::FIRST,
            TurnLifecycle::Active,
            0,
            0,
            timestamp(8),
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                source_thread,
                InputGateRevision::new(1).unwrap(),
                InputGateState::PendingTurn(source),
                0,
                None,
                None,
                0,
                0,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            source_thread,
            TranscriptGeneration::FIRST,
            ProjectionRevision::new(1).unwrap(),
            0,
            Some(source),
            child_turn_chain_digest(
                source,
                SyndicTurnId::from_bytes([29; 16]),
                root_turn_chain_digest(SyndicTurnId::from_bytes([29; 16])),
            ),
            ProjectionLifecycle::Stale,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            active_thread,
            TranscriptGeneration::FIRST,
            ProjectionRevision::new(1).unwrap(),
            0,
            Some(active),
            root_turn_chain_digest(active),
            ProjectionLifecycle::Stale,
        )),
        FixtureRecord::ActivityQueryHead(
            ActivityQueryHeadRecord::new(
                source_thread,
                ActivityWorkPeriod::FIRST,
                Some(ActivityQuerySource::new(source_thread, source)),
                true,
                0,
                ActivityQueryRevision::FIRST,
                1,
                0,
                0,
                0,
                0,
                None,
                ProjectionLifecycle::Current,
            )
            .unwrap(),
        ),
        FixtureRecord::ActivityQuerySource(ActivityQuerySourceRecord::new(
            source_thread,
            ActivityWorkPeriod::FIRST,
            ActivityQuerySource::new(source_thread, source),
            None,
            0,
            true,
            None,
        )),
        FixtureRecord::ActivityQueryHead(
            ActivityQueryHeadRecord::new(
                active_thread,
                ActivityWorkPeriod::FIRST,
                Some(ActivityQuerySource::new(active_thread, active)),
                true,
                0,
                ActivityQueryRevision::FIRST,
                1,
                0,
                0,
                0,
                0,
                None,
                ProjectionLifecycle::Current,
            )
            .unwrap(),
        ),
        FixtureRecord::ActivityQuerySource(ActivityQuerySourceRecord::new(
            active_thread,
            ActivityWorkPeriod::FIRST,
            ActivityQuerySource::new(active_thread, active),
            None,
            0,
            true,
            None,
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            source_thread,
            BindingRevision::new(2).unwrap(),
            BindingLifecycle::Valid,
            child_turn_chain_digest(
                source,
                SyndicTurnId::from_bytes([29; 16]),
                root_turn_chain_digest(SyndicTurnId::from_bytes([29; 16])),
            ),
        )),
        FixtureRecord::CasThread(CasThreadIndexRecord::with_latest(
            source_cas_thread(),
            source_thread,
            BindingRevision::new(2).unwrap(),
            BindingRevision::new(2).unwrap(),
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            source_cas_thread(),
            source_thread,
            BindingRevision::new(2).unwrap(),
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            source_thread,
            ProjectionRevision::new(1).unwrap(),
            ThreadRevision::new(1).unwrap(),
            Some(source),
            child_turn_chain_digest(
                source,
                SyndicTurnId::from_bytes([29; 16]),
                root_turn_chain_digest(SyndicTurnId::from_bytes([29; 16])),
            ),
            false,
            timestamp(4),
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            active_thread,
            ProjectionRevision::new(1).unwrap(),
            ThreadRevision::new(3).unwrap(),
            Some(active),
            root_turn_chain_digest(active),
            false,
            timestamp(8),
        )),
    ]);
    records
}
