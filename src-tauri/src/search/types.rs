//! DTO types for the find-messages Tauri command surface.
//!
//! These mirror the TypeScript-side types in Task 11 and form the wire format
//! across the Tauri command boundary.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FilterKey {
    Folder,
    From,
    To,
    DateRange,
    FormType,
    HasForm,
    HasAttach,
    ReadState,
    Transport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum FilterValue {
    /// Folder filter: "inbox" | "outbox" | "sent" | "archive" | "all"
    Folder(String),
    /// Free-form address glob, e.g. "KX5DD" or "*@KX5DD".
    Addr(String),
    /// Date range, both bounds optional (unix epoch seconds, UTC).
    DateRange { from: Option<i64>, to: Option<i64> },
    /// Form-type id, e.g. "ICS-213". Empty string never appears (use chip omission instead).
    FormType(String),
    /// Boolean toggle (`has-form`, `has-attach`).
    Bool(bool),
    /// Read-state tri-state mapped to two-state at the chip layer (only `Read` or `Unread`).
    ReadState(ReadState),
    /// Transport id, e.g. "telnet" | "packet" | "vara-hf" | "vara-fm" | "ardop".
    Transport(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReadState {
    Read,
    Unread,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    DateDesc,
    DateAsc,
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::DateDesc
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRequest {
    pub page_size: u32,
    pub offset: u32,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            page_size: 200,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct QuerySpec {
    /// Free-text portion, mapped to FTS5 `MATCH`. `None` → no FTS clause.
    pub free_text: Option<String>,
    /// Active chip state, keyed by `FilterKey` (BTreeMap so command serialization is deterministic).
    pub filters: BTreeMap<FilterKey, FilterValue>,
    pub sort: SortOrder,
    pub page: PageRequest,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(non_snake_case)]
    #[test]
    fn querySpec_serde_roundtrip_for_typical_active_query() {
        let mut filters = BTreeMap::new();
        filters.insert(FilterKey::From, FilterValue::Addr("KX5DD".into()));
        filters.insert(
            FilterKey::DateRange,
            FilterValue::DateRange {
                from: Some(1_700_000_000),
                to: None,
            },
        );
        filters.insert(FilterKey::FormType, FilterValue::FormType("ICS-213".into()));

        let spec = QuerySpec {
            free_text: Some("damage".into()),
            filters,
            sort: SortOrder::DateDesc,
            page: PageRequest::default(),
        };

        let json = serde_json::to_string(&spec).unwrap();
        let back: QuerySpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }

    #[allow(non_snake_case)]
    #[test]
    fn filterValue_kind_tag_matches_kebab_case_keys() {
        let v = FilterValue::Addr("KX5DD".into());
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains(r#""kind":"addr""#), "got {json}");
        assert!(json.contains(r#""value":"KX5DD""#), "got {json}");
    }
}
