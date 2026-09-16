use beryl_home_store::{HomeCandidateRecoveryAccess, HomeStore};
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    HistorySummaryRecord, InputGateRecord, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    TurnStateRecord,
};

#[derive(Clone, Copy)]
pub(super) enum SourceAccess<'a> {
    Ordinary(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}

macro_rules! point_read {
    ($name:ident, $candidate:ident, $key:ty, $value:ty) => {
        pub(super) fn $name(
            self,
            storage: &SyndicStorage,
            key: $key,
            limit: SyndicPointReadLimit,
        ) -> Result<Option<$value>, SyndicReadError> {
            match self {
                Self::Ordinary(store) => storage.$name(store, key, limit),
                Self::Candidate(store) => storage.$candidate(store, key, limit),
            }
        }
    };
}

impl SourceAccess<'_> {
    point_read!(
        turn_state,
        turn_state_candidate,
        SyndicTurnId,
        TurnStateRecord
    );
    point_read!(
        input_gate,
        input_gate_candidate,
        SyndicThreadId,
        InputGateRecord
    );
    point_read!(
        history_summary,
        history_summary_candidate,
        SyndicThreadId,
        HistorySummaryRecord
    );
}
