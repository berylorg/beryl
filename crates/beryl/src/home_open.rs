use std::path::Path;

use beryl_app::bootstrap::HomeOpenOutcome;
use beryl_home_store::{
    CommandCancellation, HomeOpenCandidate, HomeOpenError, HomeOpenOptions, HomeSchemaVersion,
};
use beryl_state::BerylState;
use syndic_storage::SyndicStorage;

pub(crate) fn open(path: &Path, cancellation: CommandCancellation) -> HomeOpenOutcome {
    #[cfg(feature = "test-faults")]
    if std::env::var_os("BERYL_TEST_PANIC_HOME_OPEN").as_deref() == Some(std::ffi::OsStr::new("1"))
    {
        use std::io::{Read, Write};
        let (released, gate) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut byte = [0];
            let result = std::io::stdin().read_exact(&mut byte);
            let _ = released.send(result);
        });
        println!("beryl-home-fault-ready");
        std::io::stdout().flush().unwrap();
        let _ = gate.recv_timeout(std::time::Duration::from_secs(20));
        panic!("isolated application home opener fault");
    }
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
