//! Stable identifiers shared between the library and projects.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identity of a media item in the library. Projects reference media by this
/// id; it never changes even if the file moves.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MediaId(Uuid);

impl MediaId {
    /// Placeholder for clips that show no media (title cards).
    pub const NONE: MediaId = MediaId(Uuid::nil());

    #[must_use]
    pub fn new() -> Self {
        MediaId(Uuid::new_v4())
    }

    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        MediaId(uuid)
    }

    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for MediaId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MediaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.hyphenated())
    }
}

impl FromStr for MediaId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(MediaId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_round_trip_through_text() {
        let a = MediaId::new();
        let b = MediaId::new();
        assert_ne!(a, b);
        let parsed: MediaId = a.to_string().parse().unwrap();
        assert_eq!(parsed, a);
        assert_eq!(a.to_string().len(), 36);
    }

    #[test]
    fn serializes_as_plain_string() {
        let id = MediaId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
    }
}
