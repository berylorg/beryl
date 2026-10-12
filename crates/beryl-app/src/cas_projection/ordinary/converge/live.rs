use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{SyndicPointReadLimit, SyndicStorage, TurnItemOrdinal};

use super::{access::HistoryAccess, projection, snapshot, transcript};
use crate::cas_projection::OrdinaryTurnExecutionError;

#[derive(Default)]
pub(in crate::cas_projection::ordinary) struct LiveHistoryProgress {
    stage: Option<Stage>,
}

enum Stage {
    Items(Option<TurnItemOrdinal>),
    Transcript,
}

impl LiveHistoryProgress {
    pub(in crate::cas_projection::ordinary) fn restart(&mut self) {
        self.stage = Some(Stage::Items(None));
    }

    pub(in crate::cas_projection::ordinary) fn pending(&self) -> bool {
        self.stage.is_some()
    }

    pub(in crate::cas_projection::ordinary) fn step(
        &mut self,
        home: &HomeStore,
        storage: &SyndicStorage,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        let access = HistoryAccess::Ordinary(home);
        let Some(stage) = self.stage.as_ref() else {
            return Ok(());
        };
        match stage {
            Stage::Items(after) => {
                let turn_record = access.turn(storage, turn, limit)?.ok_or(
                    OrdinaryTurnExecutionError::Invariant("live history turn disappeared"),
                )?;
                if turn_record.origin_thread_id() != thread {
                    return Err(OrdinaryTurnExecutionError::Invariant(
                        "live history turn changed owner",
                    ));
                }
                let page = access.turn_items(
                    storage,
                    turn,
                    *after,
                    CursorReadLimits::new(1, limit.max_bytes())
                        .expect("point-read byte limit is nonzero"),
                )?;
                let Some(index) = page.records().first() else {
                    self.stage = Some(Stage::Transcript);
                    return Ok(());
                };
                let item = access
                    .canonical_item(storage, index.item_id(), limit)?
                    .ok_or(OrdinaryTurnExecutionError::Invariant(
                        "live history item disappeared",
                    ))?;
                if item.projection_source().is_some() {
                    let current =
                        snapshot::item_projection(access, storage, thread, item.id(), limit)?;
                    if projection::step(access, storage, &current)?
                        == projection::ItemProjectionStep::Advanced
                    {
                        return Ok(());
                    }
                }
                self.stage = Some(Stage::Items(Some(index.ordinal())));
            }
            Stage::Transcript => {
                if transcript::step(access, storage, thread, limit)? {
                    self.stage = None;
                }
            }
        }
        Ok(())
    }
}
