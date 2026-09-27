//! Preserve JSON member order for import-map normalization. Different authored keys can
//! normalize to the same URL, in which case the last member encountered wins.

use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

#[derive(Debug)]
pub(super) enum JsonNode {
    Object(Vec<(String, Self)>),
    String(String),
    Other,
}

impl JsonNode {
    pub(super) fn parse(source: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(source)
    }

    pub(super) fn object(&self) -> Option<&[(String, Self)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }

    pub(super) fn string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub(super) fn field(&self, name: &str) -> Option<&Self> {
        self.object()?
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }
}

impl<'de> Deserialize<'de> for JsonNode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(JsonNodeVisitor)
    }
}

struct JsonNodeVisitor;

impl<'de> Visitor<'de> for JsonNodeVisitor {
    type Value = JsonNode;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut entries = Vec::new();
        while let Some((name, value)) = map.next_entry()? {
            entries.push((name, value));
        }
        Ok(JsonNode::Object(entries))
    }

    fn visit_seq<S>(self, mut sequence: S) -> Result<Self::Value, S::Error>
    where
        S: SeqAccess<'de>,
    {
        while sequence.next_element::<IgnoredAny>()?.is_some() {}
        Ok(JsonNode::Other)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(JsonNode::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(JsonNode::String(value))
    }

    fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(JsonNode::Other)
    }

    fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(JsonNode::Other)
    }

    fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(JsonNode::Other)
    }

    fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(JsonNode::Other)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(JsonNode::Other)
    }
}
