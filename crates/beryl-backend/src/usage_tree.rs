use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::JsonRpcError;

pub(crate) const THREAD_TOKEN_USAGE_TREE_READ_METHOD: &str = "thread/tokenUsageTree/read";

/// An absolute, backend-accounted usage snapshot for one root thread and all
/// of its delegated descendants.
///
/// Counters deliberately use signed `i64` values to preserve the fork's wire
/// domain exactly. Beryl preserves `total_tokens` as supplied rather than
/// deriving it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "UsageTreeSnapshotWire")]
pub struct UsageTreeSnapshot {
    pub schema_version: u32,
    pub root_thread_id: String,
    pub revision: u64,
    #[serde(rename = "self")]
    pub self_usage: UsageTreeSelfUsage,
    pub descendants_total: UsageTreeTokenUsageBreakdown,
    pub tree_total: UsageTreeTokenUsageBreakdown,
    pub accounting_status: UsageTreeAccountingStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTreeSelfUsage {
    pub total: UsageTreeTokenUsageBreakdown,
    pub last: UsageTreeTokenUsageBreakdown,
    pub model_context_window: Option<i64>,
}

impl<'de> Deserialize<'de> for UsageTreeSelfUsage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let usage = UsageTreeSelfUsageWire::deserialize(deserializer)?;
        let model_context_window = match usage.model_context_window {
            RequiredNullableI64::Present(value) => value,
            RequiredNullableI64::Missing => {
                return Err(D::Error::missing_field("modelContextWindow"));
            }
        };
        Ok(Self {
            total: usage.total,
            last: usage.last,
            model_context_window,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageTreeSelfUsageWire {
    total: UsageTreeTokenUsageBreakdown,
    last: UsageTreeTokenUsageBreakdown,
    #[serde(default)]
    model_context_window: RequiredNullableI64,
}

enum RequiredNullableI64 {
    Missing,
    Present(Option<i64>),
}

impl Default for RequiredNullableI64 {
    fn default() -> Self {
        Self::Missing
    }
}

impl<'de> Deserialize<'de> for RequiredNullableI64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<i64>::deserialize(deserializer).map(Self::Present)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTreeTokenUsageBreakdown {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    #[serde(default)]
    pub cache_write_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageTreeAccountingStatus {
    Complete,
    LegacyPartial,
}

/// The optional usage-tree read result.
///
/// `UnsupportedMethod` retains the original JSON-RPC error so a caller can
/// report or log the backend's exact compatibility detail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageTreeReadOutcome {
    Snapshot(UsageTreeSnapshot),
    UnsupportedMethod { error: JsonRpcError },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageTreeSnapshotWire {
    schema_version: u32,
    root_thread_id: String,
    revision: u64,
    #[serde(rename = "self")]
    self_usage: UsageTreeSelfUsage,
    descendants_total: UsageTreeTokenUsageBreakdown,
    tree_total: UsageTreeTokenUsageBreakdown,
    accounting_status: UsageTreeAccountingStatus,
}

impl TryFrom<UsageTreeSnapshotWire> for UsageTreeSnapshot {
    type Error = &'static str;

    fn try_from(snapshot: UsageTreeSnapshotWire) -> Result<Self, Self::Error> {
        if snapshot.schema_version != 1 {
            return Err("unsupported usage-tree schemaVersion");
        }
        if snapshot.root_thread_id.trim().is_empty() {
            return Err("usage-tree rootThreadId must not be blank");
        }

        Ok(Self {
            schema_version: snapshot.schema_version,
            root_thread_id: snapshot.root_thread_id,
            revision: snapshot.revision,
            self_usage: snapshot.self_usage,
            descendants_total: snapshot.descendants_total,
            tree_total: snapshot.tree_total,
            accounting_status: snapshot.accounting_status,
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageTreeReadParams<'a> {
    thread_id: &'a str,
}

impl<'a> UsageTreeReadParams<'a> {
    pub(crate) fn new(thread_id: &'a str) -> Self {
        Self { thread_id }
    }
}

pub(crate) fn is_unsupported_usage_tree_method(error: &JsonRpcError) -> bool {
    error.code == -32601
        || (error.code == -32600
            && error
                .message
                .strip_prefix("Invalid request:")
                .is_some_and(is_generic_unknown_usage_tree_method))
}

fn is_generic_unknown_usage_tree_method(detail: &str) -> bool {
    let detail = detail.trim_start();
    ["unknown method", "unknown variant"].iter().any(|kind| {
        detail
            .get(..kind.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(kind))
            && detail
                .get(kind.len()..)
                .is_some_and(is_exact_usage_tree_method_token)
    })
}

fn is_exact_usage_tree_method_token(remainder: &str) -> bool {
    let remainder = remainder.trim_start();
    let remainder = remainder
        .strip_prefix(':')
        .map(str::trim_start)
        .unwrap_or(remainder);
    let (remainder, quote) = match remainder.chars().next() {
        Some(quote @ ('`' | '\'' | '"')) => (&remainder[quote.len_utf8()..], Some(quote)),
        _ => (remainder, None),
    };
    let Some(after_method) = remainder.strip_prefix(THREAD_TOKEN_USAGE_TREE_READ_METHOD) else {
        return false;
    };

    match quote {
        Some(quote) => after_method.starts_with(quote),
        None => {
            after_method.is_empty()
                || after_method.starts_with(char::is_whitespace)
                || after_method.starts_with(',')
        }
    }
}
