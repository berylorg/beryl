use std::{
    fmt,
    io::Read,
    time::{Duration, Instant},
};

use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

pub(crate) struct SanitizedJsonRpcMessage {
    pub(crate) value: Value,
    pub(crate) stats: ResponseSanitizerStats,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ResponseSanitizerStats {
    pub(crate) turn_count: usize,
    pub(crate) item_count: usize,
    pub(crate) image_result_removed_count: usize,
    pub(crate) total_sanitize: Duration,
    pub(crate) result_sanitize: Duration,
    pub(crate) turn_array_sanitize: Duration,
    pub(crate) item_array_sanitize: Duration,
    pub(crate) image_result_skip: Duration,
    pub(crate) image_result_skip_max: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResponseSanitizerKind {
    ThreadRead,
    ThreadTurnsList,
}

pub(crate) fn response_sanitizer_kind(method: &str) -> Option<ResponseSanitizerKind> {
    match method {
        "thread/read" => Some(ResponseSanitizerKind::ThreadRead),
        "thread/turns/list" => Some(ResponseSanitizerKind::ThreadTurnsList),
        _ => None,
    }
}

pub(crate) fn sanitize_json_rpc_message<R: Read>(
    kind: ResponseSanitizerKind,
    request_id: u64,
    reader: R,
) -> Result<SanitizedJsonRpcMessage, serde_json::Error> {
    let sanitize_started = Instant::now();
    let mut stats = ResponseSanitizerStats::default();
    let mut deserializer = serde_json::Deserializer::from_reader(reader);
    let value = JsonRpcMessageSeed {
        kind,
        request_id,
        stats: &mut stats,
    }
    .deserialize(&mut deserializer)?;
    deserializer.end()?;
    stats.total_sanitize = sanitize_started.elapsed();
    Ok(SanitizedJsonRpcMessage { value, stats })
}

struct JsonRpcMessageSeed<'a> {
    kind: ResponseSanitizerKind,
    request_id: u64,
    stats: &'a mut ResponseSanitizerStats,
}

impl<'a, 'de> DeserializeSeed<'de> for JsonRpcMessageSeed<'a> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for JsonRpcMessageSeed<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON-RPC response, notification, or server request object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();
        let mut response_id = None;
        let mut sanitized_result_before_id = false;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "id" => {
                    let value = map.next_value::<Value>()?;
                    response_id = value.as_u64();
                    object.insert(key, value);
                }
                "result" => {
                    let should_sanitize = response_id.is_none_or(|id| id == self.request_id);
                    let value = if should_sanitize {
                        if response_id.is_none() {
                            sanitized_result_before_id = true;
                        }
                        sanitize_result_value(self.kind, self.stats, &mut map)?
                    } else {
                        map.next_value::<Value>()?
                    };
                    object.insert(key, value);
                }
                _ => {
                    object.insert(key, map.next_value::<Value>()?);
                }
            }
        }

        if sanitized_result_before_id && response_id != Some(self.request_id) {
            return Err(de::Error::custom(format!(
                "streaming sanitizer selected for request id {}, but response id was {:?}",
                self.request_id, response_id
            )));
        }

        Ok(Value::Object(object))
    }
}

fn sanitize_result_value<'de, A>(
    kind: ResponseSanitizerKind,
    stats: &mut ResponseSanitizerStats,
    map: &mut A,
) -> Result<Value, A::Error>
where
    A: MapAccess<'de>,
{
    let sanitize_started = Instant::now();
    match kind {
        ResponseSanitizerKind::ThreadRead => {
            let value = map.next_value_seed(ThreadReadResultSeed { stats })?;
            stats.result_sanitize += sanitize_started.elapsed();
            Ok(value)
        }
        ResponseSanitizerKind::ThreadTurnsList => {
            let value = map.next_value_seed(ThreadTurnsListResultSeed { stats })?;
            stats.result_sanitize += sanitize_started.elapsed();
            Ok(value)
        }
    }
}

struct ThreadTurnsListResultSeed<'a> {
    stats: &'a mut ResponseSanitizerStats,
}

impl<'a, 'de> DeserializeSeed<'de> for ThreadTurnsListResultSeed<'a> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for ThreadTurnsListResultSeed<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a thread/turns/list result object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();
        let mut saw_data = false;

        while let Some(key) = map.next_key::<String>()? {
            let value = if key == "data" {
                saw_data = true;
                map.next_value_seed(TurnArraySeed { stats: self.stats })?
            } else {
                map.next_value::<Value>()?
            };
            object.insert(key, value);
        }

        if !saw_data {
            return Err(de::Error::missing_field("data"));
        }

        Ok(Value::Object(object))
    }
}

struct ThreadReadResultSeed<'a> {
    stats: &'a mut ResponseSanitizerStats,
}

impl<'a, 'de> DeserializeSeed<'de> for ThreadReadResultSeed<'a> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for ThreadReadResultSeed<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a thread/read result object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();
        let mut saw_thread = false;

        while let Some(key) = map.next_key::<String>()? {
            let value = if key == "thread" {
                saw_thread = true;
                map.next_value_seed(ThreadObjectSeed { stats: self.stats })?
            } else {
                map.next_value::<Value>()?
            };
            object.insert(key, value);
        }

        if !saw_thread {
            return Err(de::Error::missing_field("thread"));
        }

        Ok(Value::Object(object))
    }
}

struct ThreadObjectSeed<'a> {
    stats: &'a mut ResponseSanitizerStats,
}

impl<'a, 'de> DeserializeSeed<'de> for ThreadObjectSeed<'a> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for ThreadObjectSeed<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a thread object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();

        while let Some(key) = map.next_key::<String>()? {
            let value = if key == "turns" {
                map.next_value_seed(TurnArraySeed { stats: self.stats })?
            } else {
                map.next_value::<Value>()?
            };
            object.insert(key, value);
        }

        Ok(Value::Object(object))
    }
}

struct TurnArraySeed<'a> {
    stats: &'a mut ResponseSanitizerStats,
}

impl<'a, 'de> DeserializeSeed<'de> for TurnArraySeed<'a> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for TurnArraySeed<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array of thread turns")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let sanitize_started = Instant::now();
        let mut turns = Vec::new();
        while let Some(turn) = seq.next_element_seed(TurnObjectSeed { stats: self.stats })? {
            self.stats.turn_count += 1;
            turns.push(turn);
        }
        self.stats.turn_array_sanitize += sanitize_started.elapsed();
        Ok(Value::Array(turns))
    }
}

struct TurnObjectSeed<'a> {
    stats: &'a mut ResponseSanitizerStats,
}

impl<'a, 'de> DeserializeSeed<'de> for TurnObjectSeed<'a> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for TurnObjectSeed<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a thread turn object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();

        while let Some(key) = map.next_key::<String>()? {
            let value = if key == "items" {
                map.next_value_seed(ItemArraySeed { stats: self.stats })?
            } else {
                map.next_value::<Value>()?
            };
            object.insert(key, value);
        }

        Ok(Value::Object(object))
    }
}

struct ItemArraySeed<'a> {
    stats: &'a mut ResponseSanitizerStats,
}

impl<'a, 'de> DeserializeSeed<'de> for ItemArraySeed<'a> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for ItemArraySeed<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array of thread items")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let sanitize_started = Instant::now();
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(ItemObjectSeed { stats: self.stats })? {
            self.stats.item_count += 1;
            items.push(item);
        }
        self.stats.item_array_sanitize += sanitize_started.elapsed();
        Ok(Value::Array(items))
    }
}

struct ItemObjectSeed<'a> {
    stats: &'a mut ResponseSanitizerStats,
}

impl<'a, 'de> DeserializeSeed<'de> for ItemObjectSeed<'a> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for ItemObjectSeed<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a thread item object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();
        let mut item_type = None;
        let mut skipped_image_result_before_type = None;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "type" => {
                    if item_type.is_some() {
                        return Err(de::Error::custom("thread item has duplicate type field"));
                    }
                    let value = map.next_value::<Value>()?;
                    let type_value = value
                        .as_str()
                        .ok_or_else(|| de::Error::custom("thread item type field is not a string"))?
                        .to_string();
                    item_type = Some(type_value);
                    object.insert(key, value);
                }
                "result" if item_type.as_deref() == Some("imageGeneration") => {
                    let elapsed = skip_ignored_value(&mut map)?;
                    record_image_result_skip(self.stats, elapsed);
                }
                "result" if item_type.is_none() => {
                    skipped_image_result_before_type = Some(skip_ignored_value(&mut map)?);
                }
                _ => {
                    object.insert(key, map.next_value::<Value>()?);
                }
            }
        }

        let Some(item_type) = item_type else {
            return Err(de::Error::missing_field("type"));
        };

        if skipped_image_result_before_type.is_some() && item_type != "imageGeneration" {
            return Err(de::Error::custom(format!(
                "thread item result appeared before non-imageGeneration type {item_type}"
            )));
        }
        if let Some(elapsed) = skipped_image_result_before_type {
            record_image_result_skip(self.stats, elapsed);
        }

        Ok(Value::Object(object))
    }
}

fn skip_ignored_value<'de, A>(map: &mut A) -> Result<Duration, A::Error>
where
    A: MapAccess<'de>,
{
    let started = Instant::now();
    map.next_value::<IgnoredAny>()?;
    Ok(started.elapsed())
}

fn record_image_result_skip(stats: &mut ResponseSanitizerStats, elapsed: Duration) {
    stats.image_result_removed_count += 1;
    stats.image_result_skip += elapsed;
    if elapsed > stats.image_result_skip_max {
        stats.image_result_skip_max = elapsed;
    }
}
