use super::*;

#[test]
fn context_remains_valid_after_source_thread_selects_away_from_source_turn() {
    let thread = id(30);
    let draft = draft_id(31);
    let root = SyndicTurnId::from_bytes([29; 16]);
    let digest = root_turn_chain_digest(root);
    let thread_revision = ThreadRevision::new(2).unwrap();
    let draft_revision = DraftRevision::new(2).unwrap();
    let projection_revision = ProjectionRevision::new(2).unwrap();
    let binding_revision = BindingRevision::new(5).unwrap();
    let selected = SelectedPathProof::new(Some(root), thread_revision, digest);
    let mutation = batch([
        FixtureRecord::Thread(ThreadRecord::new(
            thread,
            selected,
            draft,
            ThreadLineageProof::new(
                None,
                None,
                syndic_storage::ThreadLineageDepth::FIRST,
                syndic_storage::root_thread_lineage_digest(thread),
            ),
            syndic_storage::ThreadImageLabelFrontiers::empty(),
            None,
        )),
        FixtureRecord::Draft(DraftRecord::new(
            draft,
            thread,
            draft_revision,
            DraftSubmissionIntent::Ordinary,
            empty_composer_content(),
            timestamp(1),
            timestamp(1),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            draft,
            draft_revision,
            thread_revision,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::new(2).unwrap(),
            projection_revision,
            0,
            Some(root),
            digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            thread,
            TranscriptGeneration::new(2).unwrap(),
            projection_revision,
            thread_revision,
            Some(root),
            digest,
            1,
            0,
            fixture_transcript_digest_seed(),
            true,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            thread,
            TranscriptGeneration::new(2).unwrap(),
            TurnDepth::FIRST,
            root,
            digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            0,
            0,
            timestamp(2),
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            projection_revision,
            thread_revision,
            Some(root),
            digest,
            true,
            timestamp(2),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_revision,
            selected,
            BindingState::unbound("source moved selection").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_revision,
            BindingLifecycle::Unbound,
            digest,
        )),
    ]);
    validate_and_reopen("context-source-moved", mutation);
}
