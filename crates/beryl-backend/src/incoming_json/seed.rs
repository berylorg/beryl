use std::fmt;

use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, Visitor};
use serde_json::{Map, Value};

use super::discard::DiscardController;

const ITEM_STARTED_METHOD: &str = "item/started";
const ITEM_COMPLETED_METHOD: &str = "item/completed";
const IMAGE_GENERATION_TYPE: &str = "imageGeneration";

pub(super) struct JsonRpcValueSeed {
    discard: DiscardController,
}

impl JsonRpcValueSeed {
    pub(super) fn new(discard: DiscardController) -> Self {
        Self { discard }
    }
}

impl<'de> DeserializeSeed<'de> for JsonRpcValueSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(JsonRpcObjectVisitor {
            discard: self.discard,
        })
    }
}

struct JsonRpcObjectVisitor {
    discard: DiscardController,
}

impl<'de> Visitor<'de> for JsonRpcObjectVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON-RPC object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();
        let mut method = None;
        let mut params_count = 0_usize;
        let mut id_seen = false;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "method" => {
                    if method.is_some() {
                        return Err(de::Error::custom(
                            "incoming JSON-RPC object had duplicate method fields",
                        ));
                    }
                    let value = map.next_value::<Value>()?;
                    let state = match value.as_str() {
                        Some(ITEM_STARTED_METHOD | ITEM_COMPLETED_METHOD) => Method::ItemLifecycle,
                        _ => Method::Other,
                    };
                    if state == Method::ItemLifecycle && id_seen {
                        return Err(de::Error::custom(
                            "item lifecycle notification had an id before its method",
                        ));
                    }
                    method = Some(state);
                    object.insert(key, value);
                }
                "params" => {
                    let Some(method) = method else {
                        return Err(de::Error::custom(
                            "incoming JSON-RPC params preceded the method discriminator",
                        ));
                    };
                    params_count = params_count.saturating_add(1);
                    let value = if method == Method::ItemLifecycle {
                        if params_count != 1 {
                            return Err(de::Error::custom(
                                "item lifecycle notification had duplicate params fields",
                            ));
                        }
                        map.next_value_seed(ItemLifecycleParamsSeed {
                            discard: self.discard.clone(),
                        })?
                    } else {
                        map.next_value::<Value>()?
                    };
                    object.insert(key, value);
                }
                "id" => {
                    if method == Some(Method::ItemLifecycle) {
                        return Err(de::Error::custom(
                            "item lifecycle notification must not contain an id",
                        ));
                    }
                    id_seen = true;
                    object.insert(key, map.next_value::<Value>()?);
                }
                _ => {
                    object.insert(key, map.next_value::<Value>()?);
                }
            }
        }

        if method == Some(Method::ItemLifecycle) && params_count != 1 {
            return Err(de::Error::custom(
                "item lifecycle notification requires exactly one params object",
            ));
        }
        Ok(Value::Object(object))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Method {
    Other,
    ItemLifecycle,
}

struct ItemLifecycleParamsSeed {
    discard: DiscardController,
}

impl<'de> DeserializeSeed<'de> for ItemLifecycleParamsSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(ItemLifecycleParamsVisitor {
            discard: self.discard,
        })
    }
}

struct ItemLifecycleParamsVisitor {
    discard: DiscardController,
}

impl<'de> Visitor<'de> for ItemLifecycleParamsVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("item lifecycle notification params with item first")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();
        let mut field_count = 0_usize;
        let mut item_seen = false;

        while let Some(key) = map.next_key::<String>()? {
            field_count = field_count.saturating_add(1);
            if field_count == 1 && key != "item" {
                return Err(de::Error::custom(
                    "item lifecycle params did not declare item first",
                ));
            }
            if key == "item" {
                if item_seen {
                    return Err(de::Error::custom(
                        "item lifecycle params had duplicate item fields",
                    ));
                }
                item_seen = true;
                let item = map.next_value_seed(ThreadItemSeed {
                    discard: self.discard.clone(),
                })?;
                object.insert(key, item);
            } else {
                object.insert(key, map.next_value::<Value>()?);
            }
        }

        if !item_seen {
            return Err(de::Error::custom(
                "item lifecycle params did not contain an item",
            ));
        }
        Ok(Value::Object(object))
    }
}

struct ThreadItemSeed {
    discard: DiscardController,
}

impl<'de> DeserializeSeed<'de> for ThreadItemSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(ThreadItemVisitor {
            discard: self.discard,
        })
    }
}

struct ThreadItemVisitor {
    discard: DiscardController,
}

impl<'de> Visitor<'de> for ThreadItemVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a thread item with its type discriminator first")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();
        let mut field_count = 0_usize;
        let mut item_type = None;
        let mut result_count = 0_usize;

        while let Some(key) = map.next_key::<String>()? {
            field_count = field_count.saturating_add(1);
            if field_count == 1 && key != "type" {
                return Err(de::Error::custom(
                    "thread item did not declare its type discriminator first",
                ));
            }
            match key.as_str() {
                "type" => {
                    if item_type.is_some() {
                        return Err(de::Error::custom(
                            "thread item had duplicate type discriminators",
                        ));
                    }
                    let value = map.next_value::<Value>()?;
                    let value_type = value.as_str().ok_or_else(|| {
                        de::Error::custom("thread item type discriminator was not a string")
                    })?;
                    item_type = Some(value_type.to_string());
                    object.insert(key, value);
                }
                "result" if item_type.as_deref() == Some(IMAGE_GENERATION_TYPE) => {
                    result_count = result_count.saturating_add(1);
                    if result_count != 1 {
                        return Err(de::Error::custom(
                            "imageGeneration item had duplicate result fields",
                        ));
                    }
                    self.discard.arm_image_result::<A::Error>()?;
                    map.next_value::<IgnoredAny>()?;
                    self.discard.require_image_result_discarded::<A::Error>()?;
                }
                _ => {
                    object.insert(key, map.next_value::<Value>()?);
                }
            }
        }

        if item_type.as_deref() == Some(IMAGE_GENERATION_TYPE) && result_count != 1 {
            return Err(de::Error::custom(
                "imageGeneration item did not contain exactly one result field",
            ));
        }
        Ok(Value::Object(object))
    }
}
