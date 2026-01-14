// SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;
use std::sync;

use tokio::io::AsyncWriteExt;

use sha2::{Digest, Sha256};
use sqlx::{FromRow, Pool, Sqlite, query, query_as, sqlite};
use thiserror::Error;

use super::service_config::ConfigService;

const SQLITE_POOL_SIZE: u32 = 16;
const INLINE_THRESHOLD: usize = 256 * 1024; // 256 KB

fn hash_blob(scope: &str, filename: &str, bytes: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(scope.as_bytes());
    hasher.update(b"/");
    hasher.update(filename.as_bytes());
    hasher.update(b"\0");
    hasher.update(bytes);
    hasher.finalize().to_vec()
}

#[derive(Debug, Clone)]
pub struct Blob(pub bytes::Bytes);

impl std::ops::Deref for Blob {
    type Target = bytes::Bytes;
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
        Ok(Blob(bytes::Bytes::copy_from_slice(slice)))
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

    #[error("failed to connect index db: {0}")]
    DbError(#[from] sqlx::Error),

    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
}

pub struct StorageService {
    sqlite: Pool<Sqlite>,
    storage_dir: PathBuf,
}

impl StorageService {
    pub async fn new(config: sync::Arc<ConfigService>) -> Result<Self, StorageError> {
        let storage_dir = config.dirname().to_path_buf();
        let connection = format!(
            "sqlite:{}?mode=rwc",
            storage_dir.join("index.sqlite").to_str().unwrap()
        );

        let pool: Pool<Sqlite> = sqlite::SqlitePoolOptions::new()
            .max_connections(SQLITE_POOL_SIZE)
            .connect(&connection)
            .await?;

        let storage = Self {
            sqlite: pool,
            storage_dir,
        };
        storage.msetupdb().await?;

        Ok(storage)
    }

    async fn msetupdb(&self) -> Result<(), StorageError> {
        query("PRAGMA foreign_keys = ON;")
            .execute(&self.sqlite)
            .await?;
        query("PRAGMA journal_mode = WAL;")
            .execute(&self.sqlite)
            .await?;
        query("PRAGMA busy_timeout = 5000;")
            .execute(&self.sqlite)
            .await?;

        query(
            "CREATE TABLE IF NOT EXISTS datafiles(
                id         INTEGER PRIMARY KEY,
                scope      TEXT    NOT NULL,
                created_at TEXT    DEFAULT (datetime('now')),
                file_name  TEXT    NOT NULL,
                file_size  INTEGER NOT NULL,
                file_bytes BLOB,
                inlined    INTEGER NOT NULL,

                UNIQUE (scope, file_name)
            ) STRICT;",
        )
        .execute(&self.sqlite)
        .await?;

        Ok(())
    }

    pub async fn put(
        &self,
        scope: &str,
        filename: &str,
        bytes: &bytes::Bytes,
    ) -> Result<(), StorageError> {
        let inlined = bytes.len() <= INLINE_THRESHOLD;

        let stored_bytes: Vec<u8> = if inlined {
            bytes.to_vec()
        } else {
            self.write_blob(scope, filename, bytes).await?
        };

        let result = query("INSERT INTO datafiles (scope, file_name, file_size, file_bytes, inlined) VALUES (?1, ?2, ?3, ?4, ?5);")
            .bind(scope)
            .bind(filename)
            .bind(bytes.len() as i64)
            .bind(&stored_bytes)
            .bind(inlined)
            .execute(&self.sqlite)
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                if !inlined {
                    let _ = self.delete_blob(&stored_bytes).await;
                }
                match e {
                    sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => Err(
                        StorageError::AlreadyExists(scope.to_string(), filename.to_string()),
                    ),
                    _ => Err(e.into()),
                }
            }
        }
    }

    async fn write_blob(
        &self,
        scope: &str,
        filename: &str,
        bytes: &bytes::Bytes,
    ) -> Result<Vec<u8>, StorageError> {
        let hash = hash_blob(scope, filename, bytes);
        let hash_hex = hex::encode(&hash);

        let blob_dir = self.storage_dir.join(&hash_hex[0..2]).join(&hash_hex[2..4]);
        let blob_path = blob_dir.join(&hash_hex);

        tokio::fs::create_dir_all(&blob_dir).await?;

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&blob_path)
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    StorageError::AlreadyExists(scope.to_string(), filename.to_string())
                } else {
                    StorageError::IoError(e)
                }
            })?;

        file.write_all(bytes).await?;
        file.sync_all().await?;

        Ok(hash)
    }

    fn blob_path(&self, hash: &[u8]) -> PathBuf {
        let hash_hex = hex::encode(hash);
        self.storage_dir
            .join(&hash_hex[0..2])
            .join(&hash_hex[2..4])
            .join(&hash_hex)
    }

    async fn delete_blob(&self, hash: &[u8]) -> Result<(), StorageError> {
        tokio::fs::remove_file(self.blob_path(hash)).await?;
        Ok(())
    }

    pub async fn get(&self, scope: &str, filename: &str) -> Result<Option<File>, StorageError> {
        let file: Option<File> =
            query_as("SELECT * FROM datafiles WHERE scope = ?1 AND file_name = ?2")
                .bind(scope)
                .bind(filename)
                .fetch_optional(&self.sqlite)
                .await?;

        match file {
            None => Ok(None),
            Some(file) if file.inlined => Ok(Some(file)),
            Some(mut file) => {
                let bytes = self.read_blob(scope, filename, &file.file_bytes).await?;
                file.file_bytes = Blob(bytes);
                Ok(Some(file))
            }
        }
    }

    async fn read_blob(
        &self,
        scope: &str,
        filename: &str,
        expected_hash: &[u8],
    ) -> Result<bytes::Bytes, StorageError> {
        let bytes = tokio::fs::read(self.blob_path(expected_hash)).await?;

        let actual_hash = hash_blob(scope, filename, &bytes);
        if actual_hash != expected_hash {
            return Err(StorageError::IntegrityError);
        }

        Ok(bytes::Bytes::from(bytes))
    }
}
