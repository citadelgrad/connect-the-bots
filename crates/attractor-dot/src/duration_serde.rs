use serde::{self, Deserialize, Deserializer, Serializer};
use std::time::Duration;

pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let ms = duration.as_millis();
    serializer.serialize_str(&format!("{}ms", ms))
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_duration_str(&s).map_err(serde::de::Error::custom)
}

fn parse_duration_str(s: &str) -> Result<Duration, String> {
    if let Some(val) = s.strip_suffix("ms") {
        let n: u64 = val.parse().map_err(|e| format!("invalid duration: {}", e))?;
        Ok(Duration::from_millis(n))
    } else if let Some(val) = s.strip_suffix('s') {
        let n: u64 = val.parse().map_err(|e| format!("invalid duration: {}", e))?;
        Ok(Duration::from_secs(n))
    } else if let Some(val) = s.strip_suffix('m') {
        let n: u64 = val.parse().map_err(|e| format!("invalid duration: {}", e))?;
        Ok(Duration::from_secs(n * 60))
    } else if let Some(val) = s.strip_suffix('h') {
        let n: u64 = val.parse().map_err(|e| format!("invalid duration: {}", e))?;
        Ok(Duration::from_secs(n * 3600))
    } else if let Some(val) = s.strip_suffix('d') {
        let n: u64 = val.parse().map_err(|e| format!("invalid duration: {}", e))?;
        Ok(Duration::from_secs(n * 86400))
    } else {
        Err(format!("unknown duration format: {}", s))
    }
}
