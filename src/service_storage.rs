// SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
// SPDX-License-Identifier: AGPL-3.0-or-later

// This class stores uploaded files and associated metadata.
// Files are immutable but can be deleted.
//
// A unique file is defined by a `scope` and a `filename`.
// The filename can be any valid UTF-8 string up to 1KB
// in size and does not have to be a valid filesystem name.
//
// The list of files and their metadata are stored in a single
// SQLite database table. Small files are stored in a BLOB column
// in SQLite; for larger files, a checksum is stored in the database,
// and the file itself is stored on disk.
//
// On disk new files are written in two steps:
// - the file is written to a temporary file in the same directory;
// - a transaction is opened and a new file entry is created
// - temp file is renamed to the final filename
// - the transaction is committed.
//
// This scheme guarantees that if a record exists in the table, the file is written.
// However if the server crashes after rename but before commit, an orphan file will remain on the disk.
// To combat this, when the server starts, we check that there is an entry
// in the database for each file on the disk, and we also delete all temporary (non-renamed) files.

use std::path::{Path, PathBuf};
use std::sync;

use bytes::Bytes;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Pool, Sqlite, query, query_as, sqlite};
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, instrument, warn};

use super::service_config::ConfigService;

const SQLITE_POOL_SIZE: u32 = 16;
const INLINE_THRESHOLD: usize = 256 * 1024; // 256 KB

struct FyleSystem {
    root: PathBuf,
}

impl FyleSystem {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    fn database(&self) -> PathBuf {
        self.root.join("index.sqlite")
    }

    fn blob_root(&self) -> PathBuf {
        self.root.join("blob")
    }

    fn object(&self, scope: &str, file: &str) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(scope.as_bytes());
        hasher.update(b"/");
        hasher.update(file.as_bytes());
        hasher.update(b"\0");

        let hash = hasher.finalize();
        let hash_hex = hex::encode(hash);
        self.blob_root()
            .join(&hash_hex[0..2])
            .join(&hash_hex[2..4])
            .join(&hash_hex)
    }
}

#[derive(Debug, Clone)]
pub struct Blob(pub Bytes);

impl std::ops::Deref for Blob {
    type Target = Bytes;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl sqlx::Type<Sqlite> for Blob {
    fn type_info() -> sqlite::SqliteTypeInfo {
        <Vec<u8> as sqlx::Type<Sqlite>>::type_info() // BLOB
    }

    fn compatible(ty: &sqlite::SqliteTypeInfo) -> bool {
        <Vec<u8> as sqlx::Type<Sqlite>>::compatible(ty)
    }
}

impl<'r> sqlx::decode::Decode<'r, Sqlite> for Blob {
    fn decode(
        value: sqlite::SqliteValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let slice: &'r [u8] = <&'r [u8] as sqlx::decode::Decode<Sqlite>>::decode(value)?;
        Ok(Blob(Bytes::copy_from_slice(slice)))
    }
}

#[allow(unused)]
#[derive(Debug, Clone, FromRow)]
pub struct File {
    pub id: u64,
    pub scope: String,
    pub created_at: chrono::DateTime<chrono::Utc>,

    pub file_name: String,
    pub file_size: i64,
    pub file_bytes: Blob,
    pub inlined: bool,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("file already exists: {0}/{1}")]
    AlreadyExists(String, String),

    #[error("blob integrity check failed")]
    IntegrityError,

    #[error("blob file missing on disk: {0}")]
    BlobNotFound(PathBuf),

    #[error("failed to connect index db: {0}")]
    DbError(#[from] sqlx::Error),

    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
}

pub struct StorageService {
    sqlite: Pool<Sqlite>,
    blobfs: FyleSystem,
}

impl StorageService {
    pub async fn new(config: sync::Arc<ConfigService>) -> Result<Self, StorageError> {
        let blobfs = FyleSystem::new(config.dirname());

        let connection = format!("sqlite:{}?mode=rwc", blobfs.database().to_str().unwrap());
        let sqlite: Pool<Sqlite> = sqlite::SqlitePoolOptions::new()
            .max_connections(SQLITE_POOL_SIZE)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    // Connection-specific PRAGMAs (must be set on each connection)
                    sqlx::query("PRAGMA foreign_keys = ON;")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA busy_timeout = 5000;")
                        .execute(&mut *conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(&connection)
            .await?;

        // WAL mode is database-wide and persists, only needs to be set once
        query("PRAGMA journal_mode = WAL;").execute(&sqlite).await?;

        let storage = Self { sqlite, blobfs };
        storage.migrations().await?;
        storage.doctor().await?;

        Ok(storage)
    }

    /// Synchronously traverses the tree and removes temporary files.
    /// Must run before the application starts.
    async fn doctor(&self) -> Result<(), StorageError> {
        let blob_root = self.blobfs.blob_root();
        if !blob_root.exists() {
            return Ok(());
        }

        let mut stack = vec![blob_root.clone()];
        while let Some(dir) = stack.pop() {
            let mut entries = fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let file_type = entry.file_type().await?;

                if file_type.is_dir() {
                    stack.push(path);
                } else if file_type.is_file() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(".part") {
                        warn!(path = %path.display(), "cleanup: removing temp file");
                        let _ = fs::remove_file(&path).await;
                    }
                }
            }
        }

        Ok(())
    }

    async fn migrations(&self) -> Result<(), StorageError> {
        query(
            "
            CREATE TABLE IF NOT EXISTS datafiles(
                id         INTEGER PRIMARY KEY,
                scope      TEXT    NOT NULL,
                created_at TEXT    DEFAULT (datetime('now')),
                file_name  TEXT    NOT NULL,
                file_size  INTEGER NOT NULL,
                file_bytes BLOB,
                inlined    INTEGER NOT NULL,

                UNIQUE (scope, file_name)
            ) STRICT;
            ",
        )
        .execute(&self.sqlite)
        .await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get(&self, scope: &str, filename: &str) -> Result<Option<File>, StorageError> {
        let file: Option<File> =
            query_as("SELECT * FROM datafiles WHERE scope = ?1 AND file_name = ?2")
                .bind(scope)
                .bind(filename)
                .fetch_optional(&self.sqlite)
                .await?;

        match file {
            None => {
                debug!("get: file not found");
                Ok(None)
            }
            Some(mut file) if !file.inlined => {
                let obj = self.blobfs.object(scope, filename);
                let bytes = match fs::read(&obj).await {
                    Ok(b) => Bytes::from(b),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        return Err(StorageError::BlobNotFound(obj));
                    }
                    Err(e) => return Err(e.into()),
                };

                let hash = self.blob_hash(scope, filename, &bytes);
                if hash != file.file_bytes.to_vec() {
                    return Err(StorageError::IntegrityError);
                }

                file.file_bytes = Blob(bytes);
                debug!(size = file.file_size, "get: loaded from disk");
                Ok(Some(file))
            }
            Some(file) => {
                debug!(size = file.file_size, "get: loaded inline");
                Ok(Some(file))
            }
        }
    }

    #[instrument(skip(self, bytes), fields(size = bytes.len()))]
    pub async fn put(
        &self,
        scope: &str,
        filename: &str,
        bytes: &Bytes,
    ) -> Result<(), StorageError> {
        let inlined = bytes.len() <= INLINE_THRESHOLD;

        let obj = self.blobfs.object(scope, filename);
        let tmp = obj.with_extension(format!("{}.part", uuid::Uuid::new_v4()));

        // write temp file
        if !inlined {
            if let Some(parent) = obj.parent() {
                fs::create_dir_all(parent).await?;
            }

            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .await?;

            file.write_all(bytes).await?;
            file.sync_all().await?;
        }

        // bytes for small files or hash for large ones
        let payload: Vec<u8> = if inlined {
            bytes.to_vec()
        } else {
            self.blob_hash(scope, filename, bytes)
        };

        let result = async {
            let mut tx = self.sqlite.begin().await?;

            let result = query(
                "
                INSERT INTO datafiles (
                    scope, file_name, file_size, file_bytes, inlined
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5
                );
                ",
            )
            .bind(scope)
            .bind(filename)
            .bind(bytes.len() as i64)
            .bind(&payload)
            .bind(inlined)
            .execute(tx.as_mut())
            .await;

            match result {
                Ok(_) => {
                    if !inlined {
                        fs::rename(&tmp, &obj).await?;
                        if let Some(parent) = obj.parent() {
                            let dir = fs::File::open(parent).await?;
                            dir.sync_all().await?;
                        }
                    }

                    tx.commit().await?;
                    debug!("put: a new file has been commited");
                    Ok(())
                }
                Err(sqlx::Error::Database(ref db_err)) if db_err.is_unique_violation() => {
                    let existing: Option<File> =
                        query_as("SELECT * FROM datafiles WHERE scope = ?1 AND file_name = ?2")
                            .bind(scope)
                            .bind(filename)
                            .fetch_optional(tx.as_mut())
                            .await?;

                    match existing {
                        Some(file) if file.file_bytes.to_vec() == payload => {
                            if !inlined {
                                let _ = fs::remove_file(&tmp).await;
                            }

                            debug!("put: identical file already exists");
                            return Ok(());
                        }
                        Some(_) => {
                            warn!("put: file already exists with different content");
                        }
                        None => {
                            // unique_violation but row doesn't exist - shouldn't happen
                            warn!("corrupted! 'is_unique_violation' received, but data cannot be selected");
                        }
                    }

                    Err(StorageError::AlreadyExists(
                        scope.to_string(),
                        filename.to_string(),
                    ))
                }
                Err(e) => Err(e.into()),
            }
        }.await;

        if result.is_err() && !inlined {
            let _ = fs::remove_file(&tmp).await;
        }

        result
    }

    fn blob_hash(&self, scope: &str, file: &str, blob: &[u8]) -> Vec<u8> {
        let mut hasher = Sha256::new();

        hasher.update(b"/");
        hasher.update(scope.as_bytes());

        hasher.update(b"/");
        hasher.update(file.as_bytes());

        hasher.update(b"/");
        hasher.update(blob);
        hasher.update(b"\0");

        hasher.finalize().to_vec()
    }
}
