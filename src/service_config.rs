// SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tokio::fs;

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Parse(toml::de::Error),
}

#[derive(Debug, Deserialize)]
pub struct ConfigService {
    #[serde(default = "ConfigService::default_addr")]
    addr: String,

    #[serde(default = "ConfigService::default_dirname")]
    dirname: PathBuf,
}

impl ConfigService {
    pub async fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path).await.map_err(ConfigError::Io)?;
        let config = toml::from_str(&content).map_err(ConfigError::Parse)?;
        Ok(config)
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }
    fn default_addr() -> String {
        "0.0.0.0:3000".to_string()
    }

    pub fn dirname(&self) -> &Path {
        &self.dirname
    }
    fn default_dirname() -> PathBuf {
        PathBuf::from("./zorian-storage")
    }
}
