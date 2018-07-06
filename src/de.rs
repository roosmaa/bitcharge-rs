use std::str::FromStr;
use bigdecimal::{BigDecimal, FromPrimitive};
use serde::de::{self, Deserialize, Deserializer};

#[derive(Deserialize)]
#[serde(untagged)]
enum BigDecimalValue {
    String(String),
    Number(f64),
}

pub fn deserialize_big_decimal<'de, D>(deserializer: D) -> Result<BigDecimal, D::Error>
    where D: Deserializer<'de>
{
    let v: BigDecimalValue = Deserialize::deserialize(deserializer)?;
    match v {
        BigDecimalValue::String(s) => BigDecimal::from_str(s.as_str())
            .map_err(de::Error::custom),
        BigDecimalValue::Number(n) => BigDecimal::from_f64(n)
            .ok_or(de::Error::custom("Unable to represent f64 as BigDecimal"))
    }
}
