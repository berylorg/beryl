use super::*;

struct MarkerFreeFixture {
    _directory: tempfile::TempDir,
    store: HomeStore,
    state: BerylState,
    syndic: SyndicStorage,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
    turn: SyndicTurnId,
    item: SyndicItemId,
}

impl MarkerFreeFixture {
    fn open(seed: u8, stray: Option<bool>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = BerylState::register(&mut store).unwrap();
        let syndic = SyndicStorage::register(&mut store).unwrap();
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        let draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
        let turn = SyndicTurnId::from_bytes([seed.wrapping_add(2); 16]);
        let item = SyndicItemId::from_bytes([seed.wrapping_add(3); 16]);
        if let Some(on_draft) = stray {
            let owner = if on_draft {
                AssetOwner::CurrentDraft(draft)
            } else {
                AssetOwner::SubmittedTurnItem(item)
            };
            create_historical_asset(
                &mut store,
                state,
                Some(owner),
                SyndicDraftMarkerId::from_bytes([seed.wrapping_add(4); 16]),
                seed.wrapping_add(5),
                b"stray marker-free owner",
            );
        }
        let fixture = replacement_fixture(thread, draft, turn, item, None, None);
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(syndic.fixture_contribution(syndic.revision(&store).unwrap(), fixture))
            .unwrap();
        store.execute(command).unwrap();
        Self {
            _directory: directory,
            store,
            state,
            syndic,
            thread,
            draft,
            turn,
            item,
        }
    }

    fn edit(&self) -> StartReplacementEdit {
        StartReplacementEdit::new(
            self.thread,
            ThreadRevision::new(1).unwrap(),
            self.draft,
            DraftRevision::new(1).unwrap(),
            InputGateRevision::new(1).unwrap(),
            self.turn,
            self.item,
            SelectedPathProof::new(
                Some(self.turn),
                ThreadRevision::new(1).unwrap(),
                root_turn_chain_digest(self.turn),
            ),
            CurrentTranscriptEntryProof::new(
                TranscriptGeneration::FIRST,
                TranscriptPosition::FIRST,
            ),
            None,
            time(2),
        )
    }
}

#[test]
fn replacement_start_validates_both_marker_free_owner_heads_are_absent() {
    let fixture = MarkerFreeFixture::open(60, None);
    let command = start_replacement_edit_command(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        fixture.edit(),
    )
    .unwrap();
    fixture.store.execute(command).unwrap();
    assert!(
        fixture
            .state
            .assets()
            .owner_head(&fixture.store, AssetOwner::SubmittedTurnItem(fixture.item),)
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .state
            .assets()
            .owner_head(&fixture.store, AssetOwner::CurrentDraft(fixture.draft))
            .unwrap()
            .is_none()
    );
}

#[test]
fn replacement_start_rejects_either_marker_free_stray_head_atomically() {
    for (seed, on_draft) in [(70, false), (80, true)] {
        let fixture = MarkerFreeFixture::open(seed, Some(on_draft));
        let command = start_replacement_edit_command(
            &fixture.store,
            fixture.syndic,
            fixture.state.assets(),
            fixture.edit(),
        )
        .unwrap();
        assert!(fixture.store.execute(command).is_err());
        let current = fixture
            .syndic
            .current_draft(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(current.draft().revision(), DraftRevision::new(1).unwrap());
        assert_eq!(
            current.draft().submission_intent(),
            DraftSubmissionIntent::Ordinary
        );
    }
}
