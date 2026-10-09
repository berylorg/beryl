use super::*;
use crate::running_owner::shutdown_session::recovery::settlement::CandidateSettlement;
use std::future::Future;

pub(in crate::running_owner::shutdown_session::recovery) enum AdoptedReturnCustody {
    Captured(Box<crate::main_window::MainWindowFailedResidentAdoptionReturn>),
    Source {
        source: Box<crate::main_window::MainWindowFailedResidentCandidateSource>,
        capture: Box<crate::main_window::MainWindowFailedResidentCapture>,
        capsules: Option<Vec<crate::main_window::MainWindowRetiredPrepublicationCleanup>>,
    },
    Settled {
        capture: Box<crate::main_window::MainWindowFailedResidentCapture>,
        capsules: Vec<crate::main_window::MainWindowRetiredPrepublicationCleanup>,
    },
}

#[inline(never)]
fn return_original_source(
    graph: &mut crate::app_services::recovery_graph::PreparedRecoveryServiceGraph,
    source: Box<crate::main_window::MainWindowFailedResidentCandidateSource>,
) {
    let retired = source.take_cancelled_retirement();
    graph.return_failed_resident_source(*retired);
}

pub(super) async fn return_adopted_resident(
    owner: &impl RecoveryOwnerAccess,
    entry: &mut ResidentRecoveryWindow,
    home: beryl_model::BerylHomeId,
    generation: HomeGeneration,
    cx: &mut AsyncApp,
) -> Result<bool, String> {
    let request = owner
        .recovery_owner()?
        .borrow()
        .interrupted_exit
        .as_ref()
        .ok_or("adopted return original recovery is unavailable")?
        .request
        .clone();
    if entry.adopted_return.is_none() {
        loop {
            let returned = cx
                .update(|app| {
                    let retained = owner.recovery_owner()?;
                    let retained = retained.borrow();
                    if !retained.active_recovery_identity(&request) {
                        return Err("adopted return original request changed".into());
                    }
                    retained
                        .recovery_drafts()?
                        .borrow_mut()
                        .return_adopted_failed_resident(entry.window, home, generation, app)
                })
                .map_err(|error| error.to_string())??;
            match returned {
                None => return Ok(false),
                Some(Some(returned)) => {
                    entry.adopted_return = Some(AdoptedReturnCustody::Captured(returned));
                    break;
                }
                Some(None) => {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(50))
                        .await
                }
            }
        }
    }
    entry.adapters.take();
    let slot = owner
        .recovery_owner()?
        .borrow()
        .interrupted_exit
        .as_ref()
        .ok_or("adopted return original recovery is unavailable")?
        .settlement
        .clone();
    let graph = {
        let mut retained = slot.borrow_mut();
        let candidate_matches = match retained.as_mut() {
            Some(CandidateSettlement::Services(Ok(graph))) => {
                graph.matches_candidate(home, generation)
            }
            _ => false,
        };
        if !candidate_matches {
            return Err("adopted return exact candidate graph is unavailable".into());
        }
        let Some(CandidateSettlement::Services(Ok(graph))) =
            retained.replace(CandidateSettlement::Pending)
        else {
            unreachable!()
        };
        Box::new(graph)
    };
    let custody = entry.adopted_return.take().unwrap();
    let executor = cx.background_executor().clone();
    let (graph, custody, result) = *cx
        .background_executor()
        .spawn(async move { settle_return(graph, custody, executor).await })
        .await;
    *slot.borrow_mut() = Some(CandidateSettlement::Services(Ok(*graph)));
    entry.adopted_return = Some(custody);
    result?;
    let Some(AdoptedReturnCustody::Settled { .. }) = entry.adopted_return.as_ref() else {
        unreachable!()
    };
    let returned = cx
        .update(|_| -> Result<(), String> {
            let retained = owner.recovery_owner()?;
            let retained = retained.borrow();
            if !retained.active_recovery_identity(&request) {
                return Err("adopted return original request changed after cleanup".into());
            }
            let drafts = retained.recovery_drafts()?;
            let mut drafts = drafts.borrow_mut();
            drafts.require_recovery_window(entry.window)?;
            let Some(AdoptedReturnCustody::Settled { capture, capsules }) =
                entry.adopted_return.as_ref()
            else {
                unreachable!()
            };
            drafts.validate_adopted_cleanup_return(entry.window, capture, capsules)?;
            let Some(AdoptedReturnCustody::Settled { capture, capsules }) =
                entry.adopted_return.take()
            else {
                unreachable!()
            };
            drafts.return_adopted_cleanup(entry.window, *capture, capsules);
            Ok(())
        })
        .map_err(|error| error.to_string())?;
    returned?;
    Ok(true)
}

async fn settle_return(
    mut graph: Box<crate::app_services::recovery_graph::PreparedRecoveryServiceGraph>,
    mut custody: AdoptedReturnCustody,
    executor: gpui::BackgroundExecutor,
) -> Box<(
    Box<crate::app_services::recovery_graph::PreparedRecoveryServiceGraph>,
    AdoptedReturnCustody,
    Result<(), String>,
)> {
    if let AdoptedReturnCustody::Captured(returned) = &custody {
        let facts = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            graph.authenticate_adopted_resident_return(returned)
        }))
        .unwrap_or_else(|_| Err("adopted return source authentication unwound".into()));
        let (window, selector) = match facts {
            Ok(facts) => facts,
            Err(error) => return Box::new((graph, custody, Err(error))),
        };
        let AdoptedReturnCustody::Captured(returned) = custody else {
            unreachable!()
        };
        let (source, capture) = graph.prepare_adopted_resident_return(returned, window, selector);
        custody = AdoptedReturnCustody::Source {
            source,
            capture,
            capsules: None,
        };
    }
    if let AdoptedReturnCustody::Source {
        source, capsules, ..
    } = &mut custody
    {
        if capsules.is_none() {
            loop {
                let transferred = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    graph.take_adopted_resident_cleanup(source)
                }))
                .unwrap_or_else(|_| Err("adopted cleanup transfer unwound".into()));
                match transferred {
                    Ok(Some(transferred)) => {
                        *capsules = Some(transferred);
                        break;
                    }
                    Ok(None) => executor.timer(std::time::Duration::from_millis(50)).await,
                    Err(error) => return Box::new((graph, custody, Err(error))),
                }
            }
        }
        let mut work = Box::pin(graph.settle_adopted_resident_return(
            source,
            capsules.as_ref().unwrap(),
            executor,
        ));
        let result = std::future::poll_fn(|cx| {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work.as_mut().poll(cx)))
            {
                Ok(result) => result,
                Err(_) => {
                    std::task::Poll::Ready(Err("adopted return original cleanup unwound".into()))
                }
            }
        })
        .await;
        drop(work);
        if let Err(error) = result {
            return Box::new((graph, custody, Err(error)));
        }
        let AdoptedReturnCustody::Source {
            source,
            capture,
            capsules,
        } = custody
        else {
            unreachable!()
        };
        return_original_source(&mut graph, source);
        custody = AdoptedReturnCustody::Settled {
            capture,
            capsules: capsules.unwrap(),
        };
    }
    Box::new((graph, custody, Ok(())))
}
