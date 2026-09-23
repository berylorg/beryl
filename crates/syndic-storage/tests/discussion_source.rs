#![cfg(feature = "test-faults")]

mod support;

#[path = "discussion_source/unicode.rs"]
mod unicode;

#[path = "discussion_source/ineligible.rs"]
mod ineligible;

use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::ProjectionRevision;
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};
use support::populated::{source_item, source_projection, source_turn};
use support::{TestHome, batch, commit, id, open, seed_populated};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

struct Fixture {
    store: HomeStore,
    storage: SyndicStorage,
    state: BerylState,
    _home: TestHome,
}

impl Fixture {
    fn new() -> Self {
        let home = TestHome::new("discussion-source");
        let mut candidate = open(home.path());
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(
                SyndicStorage::required_domains()
                    .unwrap()
                    .merge(BerylState::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        seed_populated(&store, storage.clone());
        Self {
            store,
            storage,
            state,
            _home: home,
        }
    }

    fn selection(
        &self,
    ) -> (
        DiscussionContextSource,
        SelectedPathProof,
        CurrentTranscriptEntryProof,
    ) {
        let thread = self
            .storage
            .thread(&self.store, id(30), limit())
            .unwrap()
            .unwrap();
        let head = self
            .storage
            .transcript_view_head(&self.store, id(30), limit())
            .unwrap()
            .unwrap();
        let entries = self
            .storage
            .transcript_entries(
                &self.store,
                id(30),
                head.generation(),
                None,
                CursorReadLimits::new(64, 1_000_000).unwrap(),
            )
            .unwrap();
        let entry = entries
            .records()
            .iter()
            .find(|entry| entry.projection_id() == source_projection())
            .unwrap();
        (
            DiscussionContextSource::new(
                id(30),
                source_turn(),
                source_item(),
                entry.projection_id(),
                entry.projection_revision(),
                DiscussionContextRange::new(0, 9).unwrap(),
            ),
            thread.selected_path(),
            CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
        )
    }

    fn prepare(&self) -> PreparedDiscussionSource {
        let (source, path, entry) = self.selection();
        self.storage
            .prepare_discussion_source(
                &self.store,
                source,
                path,
                entry,
                DiscussionContextText::new("assistant").unwrap(),
            )
            .unwrap()
    }

    fn validate(&self, proof: PreparedDiscussionSource) -> CommandOutcome {
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command
            .add(
                self.state.settings().apply(
                    self.state.settings().revision(&self.store).unwrap(),
                    ApplySettings::new(vec![SettingUpdate::new(
                        SettingKey::ActiveThemeId,
                        ExpectedSettingRevision::Absent,
                        SettingValue::active_theme_id("source-validated").unwrap(),
                    )])
                    .unwrap(),
                ),
            )
            .unwrap();
        command.add_validation(proof.validation()).unwrap();
        self.store.execute(command)
    }
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

#[test]
fn exact_selection_and_partial_range_validate_without_mutating_source() {
    let fixture = Fixture::new();
    let (source, path, entry) = fixture.selection();
    let partial = DiscussionContextSource::new(
        source.thread_id(),
        source.turn_id(),
        source.item_id(),
        source.projection_id(),
        source.projection_revision(),
        DiscussionContextRange::new(2, 6).unwrap(),
    );
    let proof = fixture
        .storage
        .prepare_discussion_source(
            &fixture.store,
            partial,
            path,
            entry,
            DiscussionContextText::new("sist").unwrap(),
        )
        .unwrap();
    assert_eq!(proof.source(), partial);
    assert_eq!(proof.text().as_str(), "sist");
    let before = proof.domain_revision();
    assert!(matches!(
        fixture.validate(proof),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), before);
}

#[test]
fn supplied_text_range_projection_and_transcript_coordinates_are_not_authority() {
    let fixture = Fixture::new();
    let (source, path, entry) = fixture.selection();
    let changed = |thread, turn, item, projection, revision, start, end| {
        DiscussionContextSource::new(
            thread,
            turn,
            item,
            projection,
            revision,
            DiscussionContextRange::new(start, end).unwrap(),
        )
    };
    let cases = [
        (source, entry, "ASSISTANT"),
        (source, entry, "short"),
        (
            changed(
                id(40),
                source.turn_id(),
                source.item_id(),
                source.projection_id(),
                source.projection_revision(),
                0,
                9,
            ),
            entry,
            "assistant",
        ),
        (
            changed(
                source.thread_id(),
                support::populated::active_turn(),
                source.item_id(),
                source.projection_id(),
                source.projection_revision(),
                0,
                9,
            ),
            entry,
            "assistant",
        ),
        (
            changed(
                source.thread_id(),
                source.turn_id(),
                source.item_id(),
                source.projection_id(),
                ProjectionRevision::new(999).unwrap(),
                0,
                9,
            ),
            entry,
            "assistant",
        ),
        (
            changed(
                source.thread_id(),
                source.turn_id(),
                source.item_id(),
                source.projection_id(),
                source.projection_revision(),
                1,
                10,
            ),
            entry,
            "assistant",
        ),
        (
            source,
            CurrentTranscriptEntryProof::new(
                TranscriptGeneration::new(999).unwrap(),
                entry.position(),
            ),
            "assistant",
        ),
        (
            source,
            CurrentTranscriptEntryProof::new(
                entry.generation(),
                TranscriptPosition::new(999).unwrap(),
            ),
            "assistant",
        ),
    ];
    for (source, entry, text) in cases {
        assert!(
            fixture
                .storage
                .prepare_discussion_source(
                    &fixture.store,
                    source,
                    path,
                    entry,
                    DiscussionContextText::new(text).unwrap()
                )
                .is_err()
        );
    }
}

#[test]
fn prepared_witness_cannot_be_rebound_to_another_home() {
    let first = Fixture::new();
    let second = Fixture::new();
    assert!(matches!(
        second.validate(first.prepare()),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(
        second
            .state
            .settings()
            .setting(&second.store, SettingKey::ActiveThemeId)
            .unwrap()
            .is_none()
    );
}

#[test]
fn writer_rejects_a_witness_after_domain_revision_changes() {
    let fixture = Fixture::new();
    let proof = fixture.prepare();
    let head = fixture
        .storage
        .transcript_view_head(&fixture.store, id(30), limit())
        .unwrap()
        .unwrap();
    commit(
        &fixture.store,
        fixture.storage.clone(),
        batch([FixtureRecord::TranscriptViewHead(head)]),
    );
    assert!(matches!(
        fixture.validate(proof),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(
        fixture
            .state
            .settings()
            .setting(&fixture.store, SettingKey::ActiveThemeId)
            .unwrap()
            .is_none()
    );
}

#[test]
fn reopened_home_rejects_prior_generation_witness() {
    let fixture = Fixture::new();
    let proof = fixture.prepare();
    let Fixture {
        store,
        storage: _,
        state: _,
        _home: home,
    } = fixture;
    store.close().unwrap();
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(
            SyndicStorage::required_domains()
                .unwrap()
                .merge(BerylState::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let reopened = Fixture {
        store,
        storage,
        state,
        _home: home,
    };
    assert!(matches!(
        reopened.validate(proof),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        reopened.validate(reopened.prepare()),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn selection_size_is_bounded_before_source_reads() {
    assert!(DiscussionContextText::new("a".repeat(DISCUSSION_CONTEXT_MAX_BYTES)).is_ok());
    assert!(DiscussionContextText::new("a".repeat(DISCUSSION_CONTEXT_MAX_BYTES + 1)).is_err());
    assert!(DiscussionContextRange::new(0, DISCUSSION_CONTEXT_MAX_BYTES as u64 + 1).is_err());
    assert!(DiscussionContextRange::new(9, 9).is_err());
}
