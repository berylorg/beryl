use std::path::Path;

use beryl_app::bootstrap::HomeOpenOutcome;
use beryl_home_store::{
    CommandCancellation, HomeOpenCandidate, HomeOpenError, HomeOpenOptions, HomeSchemaVersion,
};
use beryl_state::BerylState;
use syndic_storage::SyndicStorage;

pub(crate) fn open(path: &Path, cancellation: CommandCancellation) -> HomeOpenOutcome {
    if cancellation.is_cancelled() {
        return failed("Home opening was cancelled".into(), None);
    }
    let mut candidate =
        match HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)) {
            Ok(candidate) => candidate,
            Err(HomeOpenError::Busy { .. }) => return HomeOpenOutcome::Busy,
            Err(error) => return failed(error.to_string(), None),
        };
    let registration = (|| -> anyhow::Result<_> {
        anyhow::ensure!(!cancellation.is_cancelled(), "Home opening was cancelled");
        let state = BerylState::register(&mut candidate)?;
        anyhow::ensure!(!cancellation.is_cancelled(), "Home opening was cancelled");
        let syndic = SyndicStorage::register(&mut candidate)?;
        let requirements =
            BerylState::required_domains()?.merge(SyndicStorage::required_domains()?)?;
        anyhow::ensure!(!cancellation.is_cancelled(), "Home opening was cancelled");
        Ok((state, syndic, requirements))
    })();
    let (state, syndic, requirements) = match registration {
        Ok(registered) => registered,
        Err(error) => return failed(error.to_string(), candidate.close().err()),
    };
    match candidate.prepare_publication(requirements) {
        Ok(candidate) if cancellation.is_cancelled() => {
            drop((state, syndic));
            failed("Home opening was cancelled".into(), candidate.close().err())
        }
        Ok(candidate) => HomeOpenOutcome::Ready {
            candidate,
            state,
            syndic,
        },
        Err(failure) => {
            let (error, candidate) = failure.into_parts();
            drop((state, syndic));
            failed(error.to_string(), candidate.close().err())
        }
    }
}

fn failed(detail: String, retained: Option<beryl_home_store::HomeCloseError>) -> HomeOpenOutcome {
    HomeOpenOutcome::Failed { detail, retained }
}
