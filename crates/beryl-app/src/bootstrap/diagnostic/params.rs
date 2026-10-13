use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SwitchThread {
    thread_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    limit: Option<usize>,
}

pub(super) fn switch_thread(value: &Value) -> Result<beryl_model::SyndicThreadId, ()> {
    let arguments: SwitchThread = serde_json::from_value(value.clone()).map_err(|_| ())?;
    if arguments.thread_id.len() > 64 {
        return Err(());
    }
    arguments.thread_id.parse().map_err(|_| ())
}

pub(super) fn observation_limit(value: &Value) -> Result<usize, ()> {
    let arguments: Observation = serde_json::from_value(value.clone()).map_err(|_| ())?;
    let limit = arguments.limit.unwrap_or(32);
    (limit <= 64).then_some(limit).ok_or(())
}

#[cfg(test)]
#[path = "../../../tests/unit/diagnostic_params.rs"]
mod tests;
