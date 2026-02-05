// SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::net::SocketAddr;
use std::time::Duration;

use serde::de::{self, Deserialize, Visitor};

/// Deserializes a duration from seconds (u64).
pub fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let secs = u64::deserialize(deserializer)?;
    Ok(Duration::from_secs(secs))
}

#[cfg(test)]
mod deserialize_duration_tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct DurationWrapper {
        #[serde(deserialize_with = "deserialize_duration")]
        value: Duration,
    }

    #[test]
    fn test_deserialize_duration() {
        let value: DurationWrapper = serde_json::from_str(r#"{"value": 5}"#).unwrap();
        assert_eq!(value.value, Duration::from_secs(5));
    }

    #[test]
    fn test_deserialize_duration_invalid_type() {
        let err = serde_json::from_str::<DurationWrapper>(r#"{"value": "5"}"#).unwrap_err();
        assert!(err.to_string().contains("invalid type"));
    }
}

/// Deserializes a u64 from either a number or a string.
pub fn deserialize_size<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct SizeVisitor;
    impl<'de> Visitor<'de> for SizeVisitor {
        type Value = u64;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number or string")
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(v)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            v.parse().map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_any(SizeVisitor)
}

#[cfg(test)]
mod deserialize_size_tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct SizeWrapper {
        #[serde(deserialize_with = "deserialize_size")]
        value: u64,
    }

    #[test]
    fn test_deserialize_size_number() {
        let value: SizeWrapper = serde_json::from_str(r#"{"value": 42}"#).unwrap();
        assert_eq!(value.value, 42);
    }

    #[test]
    fn test_deserialize_size_string() {
        let value: SizeWrapper = serde_json::from_str(r#"{"value": "2048"}"#).unwrap();
        assert_eq!(value.value, 2048);
    }

    #[test]
    fn test_deserialize_size_invalid_string() {
        let err = serde_json::from_str::<SizeWrapper>(r#"{"value": "nope"}"#).unwrap_err();
        assert!(err.to_string().contains("invalid digit"));
    }

    #[test]
    fn test_deserialize_size_invalid_type_uses_expectation() {
        let err = serde_json::from_str::<SizeWrapper>(r#"{"value": {"k": 1}}"#).unwrap_err();
        assert!(err.to_string().contains("a number or string"));
    }
}

/// Deserializes a SocketAddr from a string.
pub fn deserialize_listener_addr<'de, D>(deserializer: D) -> Result<SocketAddr, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    let raw = raw.trim();

    // Hostnames are not supported, but localhost shorthands are useful
    if let Some(port) = raw.strip_prefix("localhost:") {
        return port
            .parse::<u16>()
            .map(|port| SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), port))
            .map_err(|err| serde::de::Error::custom(format!("invalid port in '{raw}': {err}")));
    }

    raw.parse::<SocketAddr>()
        .map_err(|err| serde::de::Error::custom(format!("invalid address '{raw}': {err}",)))
}

#[cfg(test)]
mod deserialize_listener_addr_tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct AddrWrapper {
        #[serde(deserialize_with = "deserialize_listener_addr")]
        value: SocketAddr,
    }

    #[test]
    fn test_deserialize_listener_addr_localhost() {
        let value: AddrWrapper = serde_json::from_str(r#"{"value": "localhost:1234"}"#).unwrap();
        assert_eq!(value.value, "127.0.0.1:1234".parse().unwrap());
    }

    #[test]
    fn test_deserialize_listener_addr_trim() {
        let value: AddrWrapper = serde_json::from_str(r#"{"value": "  localhost:4321 "}"#).unwrap();
        assert_eq!(value.value, "127.0.0.1:4321".parse().unwrap());
    }

    #[test]
    fn test_deserialize_listener_addr_ip() {
        let value: AddrWrapper = serde_json::from_str(r#"{"value": "127.0.0.1:80"}"#).unwrap();
        assert_eq!(value.value, "127.0.0.1:80".parse().unwrap());
    }

    #[test]
    fn test_deserialize_listener_addr_invalid() {
        let err = serde_json::from_str::<AddrWrapper>(r#"{"value": "bad"}"#).unwrap_err();
        assert!(err.to_string().contains("invalid address"));
    }

    #[test]
    fn test_deserialize_listener_addr_localhost_invalid_port() {
        let err = serde_json::from_str::<AddrWrapper>(r#"{"value": "localhost:bad"}"#).unwrap_err();
        assert!(err.to_string().contains("invalid port"));
    }

    #[test]
    fn test_deserialize_listener_addr_invalid_type() {
        let err = serde_json::from_str::<AddrWrapper>(r#"{"value": 1234}"#).unwrap_err();
        assert!(err.to_string().contains("invalid type"));
    }
}
