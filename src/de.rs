use std::str::FromStr;
use bigdecimal::BigDecimal;
use serde::de::{self, Deserialize, Deserializer};

pub fn deserialize_big_decimal<'de, D>(deserializer: D) -> Result<BigDecimal, D::Error>
    where D: Deserializer<'de>
{
    let s: String = Deserialize::deserialize(deserializer)?;
    BigDecimal::from_str(s.as_str()).map_err(de::Error::custom)
}
