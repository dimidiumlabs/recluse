// SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::{Path, PathBuf};

pub struct ConfigService {
    addr: String,
    dirname: PathBuf,
}

impl ConfigService {
    pub fn new() -> Self {
        Self {
            addr: "0.0.0.0:3000".to_string(),
            dirname: PathBuf::from("./zorian-storage"),
        }
    }

    pub fn addr(&self) -> &str {
        self.addr.as_str()
    }

    pub fn dirname(&self) -> &Path {
        self.dirname.as_path()
    }
}
