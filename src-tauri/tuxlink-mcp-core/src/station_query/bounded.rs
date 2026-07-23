//! Bounded primitive newtypes — the structural floor of the `find_stations`
//! invariant (spec §"Invariant enforcement (by construction)", points 1, 2, 9).
//!
//! Three newtypes carry a compile-time cap in a const generic:
//!
//! - [`BoundedVec<T, N>`] — a `Vec<T>` that can never hold more than `N` items.
//!   `Deserialize` rejects an over-cap payload (runtime enforcement); the manual
//!   `JsonSchema` advertises `maxItems: N` (documentation for the model);
//!   [`BoundedVec::from_capped`] takes at most `N` and reports how many were
//!   dropped (so a subset is always *counted*, never silently lost).
//! - [`BoundedU8<MIN, MAX>`] — a `u8` validated into `[MIN, MAX]`.
//! - [`CappedString<MAX>`] — a `String` capped to at most `MAX` UTF-8 **bytes**,
//!   truncated on a `char` boundary (never emits invalid UTF-8). Byte-capping
//!   (not char-count-capping) is deliberate: it makes the `< 32 KB` whole-result
//!   property test's byte accounting exact rather than 4×-worst-case.
//!
//! The caps here are the *mechanism*; the request/response types compose them so
//! that "silent partial as complete" and "oversized result" are unrepresentable.

use std::borrow::Cow;
use std::fmt;

use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A `Vec` exceeded a [`BoundedVec`]'s cap on a *checked* (non-truncating)
/// construction (`TryFrom`). `from_capped` never produces this — it truncates
/// and reports `omitted` instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapExceeded {
    pub cap: usize,
    pub got: usize,
}

impl fmt::Display for CapExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected at most {} items, got {}", self.cap, self.got)
    }
}

impl std::error::Error for CapExceeded {}

/// A `u8` fell outside a [`BoundedU8`]'s `[MIN, MAX]` range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutOfRange {
    pub min: u8,
    pub max: u8,
    pub got: u8,
}

impl fmt::Display for OutOfRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "value {} out of range [{}, {}]",
            self.got, self.min, self.max
        )
    }
}

impl std::error::Error for OutOfRange {}

// ---------------------------------------------------------------------------
// BoundedVec<T, N>
// ---------------------------------------------------------------------------

/// A `Vec<T>` that structurally holds at most `N` items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedVec<T, const N: usize>(Vec<T>);

impl<T, const N: usize> BoundedVec<T, N> {
    /// Take at most `N` items from `iter`, returning the bounded vec plus the
    /// count of items dropped past the cap. The `omitted` count is what lets a
    /// caller populate a response's mandatory `omitted_*` field — a subset is
    /// always *counted*, never silently complete.
    pub fn from_capped<I: IntoIterator<Item = T>>(iter: I) -> (Self, usize) {
        let mut kept = Vec::new();
        let mut omitted = 0usize;
        for item in iter {
            if kept.len() < N {
                kept.push(item);
            } else {
                omitted += 1;
            }
        }
        (Self(kept), omitted)
    }

    /// An empty bounded vec.
    #[must_use]
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The compile-time cap.
    #[must_use]
    pub const fn cap() -> usize {
        N
    }

    #[must_use]
    pub fn into_inner(self) -> Vec<T> {
        self.0
    }
}

impl<T, const N: usize> Default for BoundedVec<T, N> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<T, const N: usize> TryFrom<Vec<T>> for BoundedVec<T, N> {
    type Error = CapExceeded;

    fn try_from(v: Vec<T>) -> Result<Self, Self::Error> {
        if v.len() > N {
            Err(CapExceeded {
                cap: N,
                got: v.len(),
            })
        } else {
            Ok(Self(v))
        }
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a BoundedVec<T, N> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<T: Serialize, const N: usize> Serialize for BoundedVec<T, N> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for BoundedVec<T, N> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = Vec::<T>::deserialize(deserializer)?;
        if v.len() > N {
            return Err(serde::de::Error::custom(format!(
                "expected at most {N} items, got {}",
                v.len()
            )));
        }
        Ok(Self(v))
    }
}

impl<T: JsonSchema, const N: usize> JsonSchema for BoundedVec<T, N> {
    fn schema_name() -> Cow<'static, str> {
        format!("BoundedVec_{}_max{N}", T::schema_name()).into()
    }

    // Inline so the `maxItems` cap embeds directly in the parent tool schema the
    // model reads, rather than hiding behind a `$ref`.
    fn inline_schema() -> bool {
        true
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "array",
            "items": serde_json::Value::from(generator.subschema_for::<T>()),
            "maxItems": N,
        })
    }
}

// ---------------------------------------------------------------------------
// BoundedU8<MIN, MAX>
// ---------------------------------------------------------------------------

/// A `u8` validated into the inclusive range `[MIN, MAX]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedU8<const MIN: u8, const MAX: u8>(u8);

impl<const MIN: u8, const MAX: u8> BoundedU8<MIN, MAX> {
    /// Validate `value` into `[MIN, MAX]`.
    pub fn new(value: u8) -> Result<Self, OutOfRange> {
        if value < MIN || value > MAX {
            Err(OutOfRange {
                min: MIN,
                max: MAX,
                got: value,
            })
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub fn get(self) -> u8 {
        self.0
    }
}

impl<const MIN: u8, const MAX: u8> Serialize for BoundedU8<MIN, MAX> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, const MIN: u8, const MAX: u8> Deserialize<'de> for BoundedU8<MIN, MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u8::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl<const MIN: u8, const MAX: u8> JsonSchema for BoundedU8<MIN, MAX> {
    fn schema_name() -> Cow<'static, str> {
        format!("BoundedU8_{MIN}_{MAX}").into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "integer",
            "minimum": MIN,
            "maximum": MAX,
        })
    }
}

// ---------------------------------------------------------------------------
// CappedString<MAX>
// ---------------------------------------------------------------------------

/// A `String` capped to at most `MAX` UTF-8 bytes, truncated on a `char`
/// boundary. Truncation never splits a `char` (output is always valid UTF-8); it
/// may split an extended grapheme cluster, which is acceptable for the curated
/// identifier / reason-code fields these cap.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CappedString<const MAX: usize>(String);

impl<const MAX: usize> CappedString<MAX> {
    /// Build from `s`, truncating to the largest `char` boundary `<= MAX` bytes.
    #[must_use]
    pub fn from_truncated(s: &str) -> Self {
        if s.len() <= MAX {
            return Self(s.to_string());
        }
        let mut end = MAX;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        Self(s[..end].to_string())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl<const MAX: usize> Serialize for CappedString<MAX> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for CappedString<MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Lenient on input: an over-long string is truncated (on a char
        // boundary), never rejected. These fields carry callsign prefixes and
        // app-minted ids that are already within bound; truncation keeps a weak
        // model from tripping over a length error it can't diagnose.
        let s = String::deserialize(deserializer)?;
        Ok(Self::from_truncated(&s))
    }
}

impl<const MAX: usize> JsonSchema for CappedString<MAX> {
    fn schema_name() -> Cow<'static, str> {
        format!("CappedString_{MAX}").into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "maxLength": MAX,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_vec_from_capped_reports_omitted() {
        let (bv, omitted) = BoundedVec::<u32, 3>::from_capped(vec![1, 2, 3, 4, 5]);
        assert_eq!(bv.as_slice(), &[1, 2, 3]);
        assert_eq!(omitted, 2);
    }

    #[test]
    fn bounded_vec_from_capped_under_cap_omits_zero() {
        let (bv, omitted) = BoundedVec::<u32, 8>::from_capped(vec![1, 2]);
        assert_eq!(bv.as_slice(), &[1, 2]);
        assert_eq!(omitted, 0);
    }

    #[test]
    fn bounded_vec_try_from_rejects_over_cap() {
        let r = BoundedVec::<u32, 2>::try_from(vec![1, 2, 3]);
        assert_eq!(r.unwrap_err(), CapExceeded { cap: 2, got: 3 });
        assert!(BoundedVec::<u32, 3>::try_from(vec![1, 2, 3]).is_ok());
    }

    #[test]
    fn bounded_vec_deserialize_rejects_over_cap() {
        let r: Result<BoundedVec<u32, 2>, _> = serde_json::from_str("[1,2,3]");
        assert!(r.is_err());
        let ok: BoundedVec<u32, 2> = serde_json::from_str("[1,2]").unwrap();
        assert_eq!(ok.as_slice(), &[1, 2]);
    }

    #[test]
    fn bounded_vec_serialize_is_transparent() {
        let (bv, _) = BoundedVec::<u32, 4>::from_capped(vec![7, 8]);
        assert_eq!(serde_json::to_string(&bv).unwrap(), "[7,8]");
    }

    #[test]
    fn bounded_u8_rejects_out_of_range() {
        assert!(BoundedU8::<1, 8>::new(0).is_err());
        assert!(BoundedU8::<1, 8>::new(9).is_err());
        assert_eq!(BoundedU8::<1, 8>::new(3).unwrap().get(), 3);
        assert_eq!(BoundedU8::<1, 8>::new(1).unwrap().get(), 1);
        assert_eq!(BoundedU8::<1, 8>::new(8).unwrap().get(), 8);
    }

    #[test]
    fn bounded_u8_deserialize_range_checks() {
        assert!(serde_json::from_str::<BoundedU8<1, 8>>("9").is_err());
        assert_eq!(
            serde_json::from_str::<BoundedU8<1, 8>>("3").unwrap().get(),
            3
        );
        assert_eq!(serde_json::to_string(&BoundedU8::<1, 8>::new(3).unwrap()).unwrap(), "3");
    }

    #[test]
    fn capped_string_truncates_on_char_boundary() {
        assert_eq!(CappedString::<4>::from_truncated("abcdef").as_str(), "abcd");
        assert_eq!(CappedString::<4>::from_truncated("ab").as_str(), "ab");
    }

    #[test]
    fn capped_string_never_splits_a_char() {
        // "é" is 2 bytes (U+00E9). Capping at 3 bytes would land mid-char after
        // "aé" (1 + 2 = 3 is a boundary) — cap at 2 bytes must back off to "a".
        let s = "aé"; // bytes: 'a'(1) + 'é'(2) = 3
        assert_eq!(CappedString::<2>::from_truncated(s).as_str(), "a");
        assert_eq!(CappedString::<3>::from_truncated(s).as_str(), "aé");
        // Output is always valid UTF-8 (compiles/round-trips as &str).
        assert!(std::str::from_utf8(CappedString::<2>::from_truncated(s).as_str().as_bytes()).is_ok());
    }

    #[test]
    fn capped_string_deserialize_truncates() {
        let s: CappedString<4> = serde_json::from_str("\"abcdef\"").unwrap();
        assert_eq!(s.as_str(), "abcd");
        assert_eq!(serde_json::to_string(&s).unwrap(), "\"abcd\"");
    }

    #[test]
    fn json_schema_advertises_caps() {
        // The types are `inline`, so `schema_for!` embeds the cap at the root of
        // the generated schema (no `$ref` indirection to chase).
        let vec_json = serde_json::Value::from(schemars::schema_for!(BoundedVec<u32, 8>));
        assert_eq!(vec_json["type"], "array");
        assert_eq!(vec_json["maxItems"], 8);

        let u8_json = serde_json::Value::from(schemars::schema_for!(BoundedU8<1, 8>));
        assert_eq!(u8_json["type"], "integer");
        assert_eq!(u8_json["minimum"], 1);
        assert_eq!(u8_json["maximum"], 8);

        let str_json = serde_json::Value::from(schemars::schema_for!(CappedString<24>));
        assert_eq!(str_json["type"], "string");
        assert_eq!(str_json["maxLength"], 24);
    }
}
