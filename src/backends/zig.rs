// SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mime_guess::mime;
use semver::Version as SemVersion;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use super::{
    Archive, Backend, BackendDelegate, FileKind, IndexError, ResolveError, ResolvedFile,
    VersionType, stable_version,
};
use crate::utils::{deserialize_duration, deserialize_size};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ZigConfig {
    pub enabled: bool,
    pub upstream: Url,
    #[serde(deserialize_with = "deserialize_duration")]
    pub refresh_interval: Duration,
}
impl Default for ZigConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            upstream: Url::parse("https://ziglang.org").unwrap(),
            refresh_interval: Duration::from_secs(60 * 10),
        }
    }
}

/// Wrapper for sort key computation.
enum ZigVersion {
    Master,
    Semver(SemVersion),
}
impl ZigVersion {
    fn parse(s: &str) -> Result<Self, IndexError> {
        if s == "master" {
            return Ok(Self::Master);
        }
        SemVersion::parse(s)
            .map(Self::Semver)
            .map_err(|_| IndexError::Parse(format!("invalid zig version: {s}")))
    }

    fn sort_key(&self) -> i64 {
        match self {
            Self::Master => i64::MAX,
            Self::Semver(v) => {
                let vtype = if v.pre.is_empty() {
                    VersionType::Stable
                } else if v.pre.as_str().starts_with("dev.") {
                    let num = v.pre.as_str()[4..].parse().unwrap_or(0);
                    VersionType::Dev(num)
                } else {
                    VersionType::Stable
                };

                stable_version(v.major, v.minor, v.patch, vtype)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid tarball filename")]
struct ParseError;

/// Describes a single file stored at `ziglang.org/download/`.
///
/// The tarball naming has changed several times. When parsing,
/// we standardize the files, but for the reverse operation
/// (getting a string from a tarball), we preserve the original path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ZigFile<'a> {
    filename: &'a str,
    os: Option<&'a str>,
    arch: Option<&'a str>,
    kind: FileKind,
    minisig: bool,
    archive: Archive,
    version: SemVersion,
    development: bool,
}

impl<'a> ZigFile<'a> {
    pub fn parse(filename: &'a str) -> Result<Self, ParseError> {
        let mut buffer = filename;
        let mut minisig = false;
        let archive;

        // (?:|-bootstrap|-[a-zA-Z0-9_]+-[a-zA-Z0-9_]+)-(
        // \d+\.\d+\.\d+(?:-dev\.\d+\+[0-9a-f]+)?
        // )\.(?:tar\.xz|zip)(?:\.minisig)?
        buffer = buffer.strip_prefix("zig-").ok_or(ParseError)?;

        // (?:|bootstrap|[a-zA-Z0-9_]+-[a-zA-Z0-9_]+)-(
        // \d+\.\d+\.\d+(?:-dev\.\d+\+[0-9a-f]+)?
        // )\.(?:tar\.xz|zip)
        if let Some(it) = buffer.strip_suffix(".minisig") {
            buffer = it;
            minisig = true;
        }

        // (?:|bootstrap|[a-zA-Z0-9_]+-[a-zA-Z0-9_]+)-(
        // \d+\.\d+\.\d+(?:-dev\.\d+\+[0-9a-f]+)?
        // )
        if let Some(it) = buffer.strip_suffix(".zip") {
            buffer = it;
            archive = Archive::Zip;
        } else if let Some(it) = buffer.strip_suffix(".tar.xz") {
            buffer = it;
            archive = Archive::TarXz;
        } else {
            return Err(ParseError);
        }

        if buffer.is_empty() {
            return Err(ParseError);
        }

        let mut it = buffer.rsplit('-');
        let last = it.next().ok_or(ParseError)?;

        let development = last.starts_with("dev");

        let version = if !development {
            SemVersion::parse(last).map_err(|_| ParseError)?
        } else {
            let semver = it.next().ok_or(ParseError)?;
            let devver = last;
            let version_str = format!("{}-{}", semver, devver);
            SemVersion::parse(&version_str).map_err(|_| ParseError)?
        };

        let (os, arch, kind) = if let Some(payload) = it.next() {
            if payload == "bootstrap" {
                (None, None, FileKind::Bootstrap)
            } else {
                // Filename format changed over time:
                // - <= 0.2.0: used zig-win64 for windows and zig-linux-x86_64 for linux ¯\_(ツ)_/¯
                // - 0.2.0 to 0.14.0: zig-OS-ARCH-VERSION (e.g. zig-linux-x86_64-0.13.0)
                // - > 0.14.0: zig-ARCH-OS-VERSION (e.g. zig-x86_64-linux-0.15.0)
                let (os, arch) = if version <= SemVersion::new(0, 2, 0) && payload == "win64" {
                    ("windows", "x86_64")
                } else if version <= SemVersion::new(0, 14, 0) {
                    (it.next().ok_or(ParseError)?, payload)
                } else {
                    (payload, it.next().ok_or(ParseError)?)
                };
                (Some(os), Some(arch), FileKind::Archive)
            }
        } else {
            (None, None, FileKind::Source)
        };

        if it.next().is_some() {
            return Err(ParseError);
        }

        Ok(ZigFile {
            filename,
            os,
            arch,
            kind,
            minisig,
            archive,
            version,
            development,
        })
    }

    /// Builds the upstream URL for this tarball.
    pub fn upstream_url(&self, upstream: &Url, source: &str) -> Result<Url, ()> {
        let mut url = upstream.clone();
        {
            let mut segments = url.path_segments_mut().map_err(|_| ())?;
            segments.pop_if_empty();
            if self.development {
                segments.push("builds");
            } else {
                segments.push("download").push(&self.version.to_string());
            }
            segments.push(self.filename);
        }
        url.query_pairs_mut().append_pair("source", source);
        Ok(url)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZigTarball {
    /// e.g. "zig-x86_64-linux-0.15.2.tar.xz"
    /// Note: upstream API returns full URL in "tarball" field, we extract filename when storing
    #[serde(alias = "tarball")]
    pub filename: String,

    /// e.g. "02aa270f183da276e5b5920b1dac44a63f1a49e55050ebde3aecc9eb82f93239"
    pub shasum: String,

    /// e.g. 53733924
    #[serde(deserialize_with = "deserialize_size")]
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZigRelease {
    /// e.g. "0.15.2" (older releases don't have this field)
    #[serde(default)]
    pub version: String,

    /// e.g. "2025-10-11"
    pub date: Option<String>,

    /// e.g. "https://ziglang.org/documentation/0.15.2/"
    pub docs: Option<String>,

    /// e.g. "https://ziglang.org/documentation/0.15.2/std/"
    #[serde(rename = "stdDocs")]
    pub std_docs: Option<String>,

    /// e.g. "https://ziglang.org/download/0.15.2/release-notes.html"
    pub notes: Option<String>,

    /// Source tarball
    pub src: Option<ZigTarball>,

    /// Bootstrap tarball
    pub bootstrap: Option<ZigTarball>,

    /// Platform-specific files (e.g., "x86_64-linux", "aarch64-macos")
    #[serde(flatten)]
    pub targets: HashMap<String, ZigTarball>,
}

pub struct ZigBackend {
    config: ZigConfig,
    source: String,
    delegate: Arc<dyn BackendDelegate>,
}
impl ZigBackend {
    pub fn new(config: ZigConfig, source: String, delegate: Arc<dyn BackendDelegate>) -> Self {
        Self {
            config,
            source,
            delegate,
        }
    }
}
#[async_trait::async_trait]
impl Backend for ZigBackend {
    const ID: &'static str = "zig";
    type Release = self::ZigRelease;

    fn enabled(&self) -> bool {
        self.config.enabled
    }

    fn refresh_interval(&self) -> std::time::Duration {
        self.config.refresh_interval
    }

    async fn resolve_file(&self, filename: &str) -> Result<ResolvedFile, ResolveError> {
        let file = ZigFile::parse(filename).map_err(|_| ResolveError::NotFound)?;
        let mime = if file.minisig {
            mime::TEXT_PLAIN
        } else {
            mime::APPLICATION_OCTET_STREAM
        };
        let url = file
            .upstream_url(&self.config.upstream, &self.source)
            .map_err(|_| ResolveError::Internal)?;

        // For stable builds, check index
        let base_filename = filename.strip_suffix(".minisig").unwrap_or(filename);
        let row: Option<Option<String>> =
            sqlx::query_scalar("SELECT minisig FROM zig_files WHERE filename = ?1")
                .bind(base_filename)
                .fetch_optional(self.delegate.db())
                .await
                .map_err(|e| {
                    tracing::error!(filename, "failed to query file: {e}");
                    ResolveError::Internal
                })?;

        // File not in index
        if row.is_none() {
            // Dev builds go directly to upstream without index check
            return if file.development {
                Ok(ResolvedFile::Upstream { url, mime })
            } else {
                Err(ResolveError::NotFound)
            };
        }

        // Return cached minisig if available
        if file.minisig
            && let Some(Some(data)) = row
        {
            return Ok(ResolvedFile::Content {
                data: data.into(),
                mime: mime::TEXT_PLAIN,
            });
        }

        Ok(ResolvedFile::Upstream { url, mime })
    }

    async fn migrate(&self) -> Result<(), IndexError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS zig_versions (
                id           INTEGER PRIMARY KEY,
                version      TEXT    NOT NULL UNIQUE,
                date         TEXT,
                docs         TEXT,
                std_docs     TEXT,
                notes        TEXT
            ) STRICT",
        )
        .execute(self.delegate.db())
        .await
        .map_err(|e| IndexError::Database(e.to_string()))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS zig_files (
                version      TEXT    NOT NULL,
                target       TEXT    NOT NULL,
                filename     TEXT    NOT NULL,
                shasum       TEXT    NOT NULL,
                size         INTEGER NOT NULL,
                minisig      TEXT,
                PRIMARY KEY (version, target),
                FOREIGN KEY (version) REFERENCES zig_versions(version)
            ) STRICT",
        )
        .execute(self.delegate.db())
        .await
        .map_err(|e| IndexError::Database(e.to_string()))?;

        // Migration: add minisig column to existing tables
        match sqlx::query("ALTER TABLE zig_files ADD COLUMN minisig TEXT")
            .execute(self.delegate.db())
            .await
        {
            Ok(_) => {}
            Err(sqlx::Error::Database(e)) if e.message().contains("duplicate column") => {}
            Err(e) => return Err(IndexError::Database(e.to_string())),
        }

        Ok(())
    }

    async fn fetch_index(&self) -> Result<(), IndexError> {
        let mut url = self.config.upstream.clone();
        url.path_segments_mut()
            .map_err(|_| IndexError::Parse("cannot-be-a-base URL".into()))?
            .pop_if_empty()
            .extend(["download", "index.json"]);
        let bytes = self.delegate.http_get(&url).await?;

        let index: HashMap<String, ZigRelease> =
            serde_json::from_slice(&bytes).map_err(|e| IndexError::Parse(e.to_string()))?;

        for (version_str, version) in index {
            if let Err(e) = self.insert_version(&version_str, &version).await {
                tracing::error!(version = version_str, "failed to index version: {e}");
            }
        }

        self.fetch_minisigs().await?;

        Ok(())
    }

    async fn get_versions(&self) -> Result<Vec<Self::Release>, IndexError> {
        use futures::{StreamExt, TryStreamExt};

        #[derive(Deserialize)]
        struct FileRow {
            target: String,
            filename: String,
            shasum: String,
            size: u64,
        }

        sqlx::query_as("
            SELECT
                v.version, v.date, v.docs, v.std_docs, v.notes,
                COALESCE(
                    json_group_array(json_object('target', f.target, 'filename', f.filename, 'shasum', f.shasum, 'size', f.size)) FILTER (WHERE f.version IS NOT NULL),
                    '[]'
                )
            FROM zig_versions v
            LEFT JOIN zig_files f ON v.version = f.version
            GROUP BY v.version
            ORDER BY v.id ASC
        ")
        .fetch(self.delegate.db())
        .map(|row| {
            let (version, date, docs, std_docs, notes, files_json):
                (String, Option<String>, Option<String>, Option<String>, Option<String>, String) =
                row.map_err(|e| IndexError::Database(e.to_string()))?;

            let file_rows: Vec<FileRow> = serde_json::from_str(&files_json)
                .map_err(|e| IndexError::Parse(e.to_string()))?;

            let mut src = None;
            let mut bootstrap = None;
            let mut targets = HashMap::new();

            for f in file_rows {
                let file = ZigTarball { filename: f.filename, shasum: f.shasum, size: f.size };
                match f.target.as_str() {
                    "src" => src = Some(file),
                    "bootstrap" => bootstrap = Some(file),
                    _ => { targets.insert(f.target, file); }
                }
            }

            Ok(ZigRelease { version, date, docs, std_docs, notes, src, bootstrap, targets })
        })
        .try_collect()
        .await
    }
}
impl ZigBackend {
    async fn insert_version(
        &self,
        version_str: &str,
        version: &ZigRelease,
    ) -> Result<(), IndexError> {
        let id = ZigVersion::parse(version_str)?.sort_key();

        let mut tx = self
            .delegate
            .db()
            .begin()
            .await
            .map_err(|e| IndexError::Database(e.to_string()))?;

        sqlx::query(
            "INSERT INTO zig_versions (id, version, date, docs, std_docs, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(version) DO UPDATE SET
                 id = excluded.id,
                 date = excluded.date, docs = excluded.docs,
                 std_docs = excluded.std_docs, notes = excluded.notes
             WHERE id IS NOT excluded.id OR date IS NOT excluded.date
                OR docs IS NOT excluded.docs OR std_docs IS NOT excluded.std_docs
                OR notes IS NOT excluded.notes",
        )
        .bind(id)
        .bind(version_str)
        .bind(&version.date)
        .bind(&version.docs)
        .bind(&version.std_docs)
        .bind(&version.notes)
        .execute(&mut *tx)
        .await
        .map_err(|e| IndexError::Database(e.to_string()))?;

        if let Some(ref file) = version.src {
            Self::insert_file(&mut tx, version_str, "src", file).await?;
        }
        if let Some(ref file) = version.bootstrap {
            Self::insert_file(&mut tx, version_str, "bootstrap", file).await?;
        }
        for (target, file) in &version.targets {
            Self::insert_file(&mut tx, version_str, target, file).await?;
        }

        tx.commit()
            .await
            .map_err(|e| IndexError::Database(e.to_string()))?;
        Ok(())
    }

    async fn insert_file(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        version: &str,
        target: &str,
        file: &ZigTarball,
    ) -> Result<(), IndexError> {
        let url = Url::parse(&file.filename)
            .map_err(|e| IndexError::Parse(format!("invalid tarball URL: {e}")))?;
        let filename = url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| IndexError::Parse(format!("no filename in URL: {}", file.filename)))?;

        let exists: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM zig_files WHERE version = ?1 AND target = ?2")
                .bind(version)
                .bind(target)
                .fetch_optional(&mut **tx)
                .await
                .map_err(|e| IndexError::Database(e.to_string()))?;

        let changed: Option<(i32,)> = sqlx::query_as(
            "INSERT INTO zig_files (version, target, filename, shasum, size)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(version, target) DO UPDATE SET
                 filename = excluded.filename, shasum = excluded.shasum, size = excluded.size
             WHERE filename IS NOT excluded.filename
                OR shasum IS NOT excluded.shasum
                OR size IS NOT excluded.size
             RETURNING 1",
        )
        .bind(version)
        .bind(target)
        .bind(filename)
        .bind(&file.shasum)
        .bind(file.size as i64)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| IndexError::Database(e.to_string()))?;

        if exists.is_some() && changed.is_some() {
            tracing::warn!(version, target, "zig index file changed");
        }

        Ok(())
    }

    async fn fetch_minisigs(&self) -> Result<(), IndexError> {
        let files: Vec<(String,)> =
            sqlx::query_as("SELECT filename FROM zig_files WHERE minisig IS NULL")
                .fetch_all(self.delegate.db())
                .await
                .map_err(|e| IndexError::Database(e.to_string()))?;

        for (filename,) in files {
            if let Err(e) = self.fetch_minisig(&filename).await {
                tracing::debug!(filename, "failed to fetch minisig: {e}");
            }
        }

        Ok(())
    }

    async fn fetch_minisig(&self, filename: &str) -> Result<(), IndexError> {
        let minisig_filename = format!("{}.minisig", filename);
        let file = ZigFile::parse(&minisig_filename)
            .map_err(|_| IndexError::Parse(format!("invalid filename: {}", filename)))?;

        let url = file
            .upstream_url(&self.config.upstream, &self.source)
            .map_err(|_| IndexError::Parse("cannot build URL".into()))?;

        let bytes = self.delegate.http_get(&url).await?;
        let minisig =
            String::from_utf8(bytes.to_vec()).map_err(|e| IndexError::Parse(e.to_string()))?;

        sqlx::query("UPDATE zig_files SET minisig = ?1 WHERE filename = ?2")
            .bind(&minisig)
            .bind(filename)
            .execute(self.delegate.db())
            .await
            .map_err(|e| IndexError::Database(e.to_string()))?;

        tracing::debug!(filename, "cached minisig");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_old_combined_platform() {
        // <= 0.2.0: zig-PLATFORM-VERSION (only Windows used this format)
        let file = ZigFile::parse("zig-win64-0.1.1.zip").unwrap();
        assert_eq!(file.os, Some("windows"));
        assert_eq!(file.arch, Some("x86_64"));
        assert_eq!(file.version, SemVersion::new(0, 1, 1));
        assert_eq!(file.archive, Archive::Zip);
        assert_eq!(file.kind, FileKind::Archive);

        let file = ZigFile::parse("zig-win64-0.2.0.zip").unwrap();
        assert_eq!(file.os, Some("windows"));
        assert_eq!(file.arch, Some("x86_64"));
        assert_eq!(file.version, SemVersion::new(0, 2, 0));
    }

    #[test]
    fn parse_middle_os_arch_format() {
        // 0.2.0 to 0.14.0: zig-OS-ARCH-VERSION
        let file = ZigFile::parse("zig-linux-x86_64-0.13.0.tar.xz").unwrap();
        assert_eq!(file.os, Some("linux"));
        assert_eq!(file.arch, Some("x86_64"));
        assert_eq!(file.version, SemVersion::new(0, 13, 0));
        assert_eq!(file.archive, Archive::TarXz);

        let file = ZigFile::parse("zig-windows-x86_64-0.10.0.zip").unwrap();
        assert_eq!(file.os, Some("windows"));
        assert_eq!(file.arch, Some("x86_64"));
    }

    #[test]
    fn parse_new_arch_os_format() {
        // > 0.14.0: zig-ARCH-OS-VERSION
        let file = ZigFile::parse("zig-x86_64-linux-0.15.0.tar.xz").unwrap();
        assert_eq!(file.os, Some("linux"));
        assert_eq!(file.arch, Some("x86_64"));
        assert_eq!(file.version, SemVersion::new(0, 15, 0));

        let file = ZigFile::parse("zig-aarch64-macos-0.15.0.tar.xz").unwrap();
        assert_eq!(file.os, Some("macos"));
        assert_eq!(file.arch, Some("aarch64"));
    }

    #[test]
    fn parse_dev_version() {
        let file = ZigFile::parse("zig-x86_64-linux-0.14.0-dev.123+abc123.tar.xz").unwrap();
        assert!(file.development);
        assert_eq!(file.version.major, 0);
        assert_eq!(file.version.minor, 14);
        assert_eq!(file.version.patch, 0);
    }

    #[test]
    fn parse_source_tarball() {
        let file = ZigFile::parse("zig-0.13.0.tar.xz").unwrap();
        assert_eq!(file.os, None);
        assert_eq!(file.arch, None);
        assert_eq!(file.kind, FileKind::Source);
    }

    #[test]
    fn parse_bootstrap() {
        let file = ZigFile::parse("zig-bootstrap-0.13.0.tar.xz").unwrap();
        assert_eq!(file.kind, FileKind::Bootstrap);
    }

    #[test]
    fn parse_minisig() {
        let file = ZigFile::parse("zig-win64-0.1.1.zip.minisig").unwrap();
        assert!(file.minisig);
        assert_eq!(file.os, Some("windows"));
        assert_eq!(file.arch, Some("x86_64"));

        let file = ZigFile::parse("zig-x86_64-linux-0.15.0.tar.xz.minisig").unwrap();
        assert!(file.minisig);
        assert_eq!(file.os, Some("linux"));
    }

    #[test]
    fn parse_boundary_version() {
        // 0.14.0 should use OS-ARCH format
        let file = ZigFile::parse("zig-linux-x86_64-0.14.0.tar.xz").unwrap();
        assert_eq!(file.os, Some("linux"));
        assert_eq!(file.arch, Some("x86_64"));

        // 0.14.1 should use ARCH-OS format
        let file = ZigFile::parse("zig-x86_64-linux-0.14.1.tar.xz").unwrap();
        assert_eq!(file.os, Some("linux"));
        assert_eq!(file.arch, Some("x86_64"));
    }
}
