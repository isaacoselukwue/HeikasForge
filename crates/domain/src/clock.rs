use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(OffsetDateTime);

impl Timestamp {
    pub fn from_offset(value: OffsetDateTime) -> Self {
        Self(value.to_offset(time::UtcOffset::UTC))
    }

    pub fn from_unix_nanos(nanos: i128) -> Result<Self, DomainError> {
        OffsetDateTime::from_unix_timestamp_nanos(nanos)
            .map(Self::from_offset)
            .map_err(|_| DomainError::ValueOutOfRange {
                field: "Timestamp",
                value: nanos.to_string(),
            })
    }

    pub fn unix_nanos(&self) -> i128 {
        self.0.unix_timestamp_nanos()
    }

    pub fn as_offset(&self) -> OffsetDateTime {
        self.0
    }

    pub fn duration_since(&self, earlier: Timestamp) -> DurationMs {
        let delta = self.unix_nanos().saturating_sub(earlier.unix_nanos());
        let millis = (delta.max(0) / 1_000_000) as u64;
        DurationMs::from_millis(millis)
    }

    pub fn plus(&self, duration: Duration) -> Timestamp {
        Self::from_offset(self.0 + duration)
    }

    pub fn to_rfc3339(&self) -> String {
        self.0
            .format(&Rfc3339)
            .unwrap_or_else(|_| String::from("1970-01-01T00:00:00Z"))
    }

    pub fn compact_stamp(&self) -> String {
        format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            self.0.year(),
            u8::from(self.0.month()),
            self.0.day(),
            self.0.hour(),
            self.0.minute(),
            self.0.second()
        )
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_rfc3339())
    }
}

impl FromStr for Timestamp {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        OffsetDateTime::parse(value, &Rfc3339)
            .map(Self::from_offset)
            .map_err(|_| DomainError::InvalidIdentifier {
                kind: "Timestamp",
                value: value.to_string(),
            })
    }
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_rfc3339())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Timestamp::from_str(&raw).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for Timestamp {
    fn schema_name() -> String {
        "Timestamp".to_string()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        let mut schema = <String as schemars::JsonSchema>::json_schema(generator).into_object();
        schema.format = Some("date-time".to_string());
        schema.into()
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct DurationMs(u64);

impl DurationMs {
    pub const ZERO: DurationMs = DurationMs(0);

    pub fn from_millis(value: u64) -> Self {
        Self(value)
    }

    pub fn from_seconds(value: u64) -> Self {
        Self(value.saturating_mul(1000))
    }

    pub fn millis(&self) -> u64 {
        self.0
    }

    pub fn as_duration(&self) -> Duration {
        Duration::from_millis(self.0)
    }

    pub fn saturating_add(&self, other: DurationMs) -> DurationMs {
        DurationMs(self.0.saturating_add(other.0))
    }

    pub fn human(&self) -> String {
        let total_seconds = self.0 / 1000;
        let millis = self.0 % 1000;
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        if hours > 0 {
            format!("{hours}h {minutes}m {seconds}s")
        } else if minutes > 0 {
            format!("{minutes}m {seconds}s")
        } else if seconds > 0 {
            format!("{seconds}.{:03}s", millis)
        } else {
            format!("{}ms", self.0)
        }
    }
}

impl schemars::JsonSchema for DurationMs {
    fn schema_name() -> String {
        "DurationMs".to_string()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <u64 as schemars::JsonSchema>::json_schema(generator)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimeoutSeconds(u32);

impl TimeoutSeconds {
    pub const MINIMUM: TimeoutSeconds = TimeoutSeconds(1);

    pub fn clamped(value: u32, upper_bound: u32) -> Self {
        Self(value.clamp(1, upper_bound.max(1)))
    }

    pub fn new(value: u32, upper_bound: u32) -> Result<Self, DomainError> {
        if value == 0 || value > upper_bound {
            return Err(DomainError::ValueOutOfRange {
                field: "TimeoutSeconds",
                value: value.to_string(),
            });
        }
        Ok(Self(value))
    }

    pub fn get(&self) -> u32 {
        self.0
    }

    pub fn as_duration(&self) -> Duration {
        Duration::from_secs(u64::from(self.0))
    }
}

impl schemars::JsonSchema for TimeoutSeconds {
    fn schema_name() -> String {
        "TimeoutSeconds".to_string()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <u32 as schemars::JsonSchema>::json_schema(generator)
    }
}
