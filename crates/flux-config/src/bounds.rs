//! Streaming preflight: reject oversized/deep structures before typed allocation.
use crate::{Result, invalid};
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};

struct Budget {
    values: usize,
    bytes: usize,
}
struct Seed<'a> {
    budget: &'a mut Budget,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, de: D) -> std::result::Result<(), D::Error> {
        self.budget.values += 1;
        if self.depth > 32 || self.budget.values > 100_000 {
            return Err(serde::de::Error::custom(
                "config depth/value budget exceeded",
            ));
        }
        de.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = ();
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("bounded JSON configuration")
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: serde::de::Error>(self, n: f64) -> std::result::Result<(), E> {
        if !n.is_finite() {
            Err(E::custom("nonfinite config number"))
        } else {
            Ok(())
        }
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_str<E: serde::de::Error>(self, s: &str) -> std::result::Result<(), E> {
        self.budget.bytes += s.len();
        if self.budget.bytes > 4 * 1024 * 1024 {
            Err(E::custom("config string budget exceeded"))
        } else {
            Ok(())
        }
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<(), A::Error> {
        while seq
            .next_element_seed(Seed {
                budget: self.budget,
                depth: self.depth + 1,
            })?
            .is_some()
        {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            self.budget.bytes += key.len();
            if self.budget.bytes > 4 * 1024 * 1024 {
                return Err(serde::de::Error::custom("config key budget exceeded"));
            }
            map.next_value_seed(Seed {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
        }
        Ok(())
    }
}
pub(crate) fn preflight(bytes: &[u8]) -> Result<()> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    Seed {
        budget: &mut Budget {
            values: 0,
            bytes: 0,
        },
        depth: 0,
    }
    .deserialize(&mut de)
    .map_err(|e| invalid(e.to_string()))?;
    de.end().map_err(|e| invalid(e.to_string()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checks_depth_values_and_trailing_data_before_schema_decode() {
        assert!(preflight(format!("{}0{}", "[".repeat(33), "]".repeat(33)).as_bytes()).is_err());
        assert!(preflight(format!("[{}0]", "0,".repeat(100_000)).as_bytes()).is_err());
        assert!(preflight(b"{} {}").is_err());
        preflight(br#"{"values":[1,null,true,"string",{}]}"#).unwrap();
    }
}
