//! Moments in time, written the way browsers write them.

use std::fmt;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

/// A UTC timestamp. It always serialises with milliseconds
/// (`2026-09-30T12:40:00.000Z`), like JavaScript's `toISOString`, so two of
/// them compare the same as strings as they do as times.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, TS)]
#[cfg_attr(
    feature = "openapi",
    derive(utoipa::ToSchema),
    schema(value_type = String, format = DateTime)
)]
#[ts(export)]
pub struct Timestamp(pub DateTime<Utc>);

impl Timestamp {
    pub fn now() -> Self {
        Self(Utc::now())
    }

    pub fn parse(text: &str) -> Option<Self> {
        DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|t| Self(t.with_timezone(&Utc)))
    }
}

impl From<DateTime<Utc>> for Timestamp {
    fn from(time: DateTime<Utc>) -> Self {
        Self(time)
    }
}

impl From<Timestamp> for DateTime<Utc> {
    fn from(time: Timestamp) -> Self {
        time.0
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_rfc3339_opts(SecondsFormat::Millis, true))
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Timestamp::parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("not a timestamp: {text}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_milliseconds_always() {
        let t = Timestamp::parse("2026-09-30T12:40:00Z").expect("parses");
        assert_eq!(t.to_string(), "2026-09-30T12:40:00.000Z");
        assert_eq!(
            serde_json::to_string(&t).expect("serialises"),
            "\"2026-09-30T12:40:00.000Z\""
        );
    }

    #[test]
    fn reads_offsets_as_utc() {
        let t = Timestamp::parse("2026-09-30T13:40:00+01:00").expect("parses");
        assert_eq!(t.to_string(), "2026-09-30T12:40:00.000Z");
    }
}
