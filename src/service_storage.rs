use std::{io, path, sync};
use tokio::fs;

use super::service_config::ConfigService;

#[derive(Clone)]
pub struct File {
    pub bytes: bytes::Bytes,
}

pub struct StorageService {
    config: sync::Arc<ConfigService>,
}

impl StorageService {
    pub fn new(config: sync::Arc<ConfigService>) -> Self {
        Self { config }
    }

    pub async fn get(&self, filename: &str) -> Result<Option<File>, io::Error> {
        let data_path = self.data_path(filename)?;
        let bytes = match fs::read(&data_path).await {
            Ok(bytes) => bytes,
            Err(err) => {
                return if err.kind() == io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(err)
                };
            }
        };

        Ok(Some(File {
            bytes: bytes.into(),
        }))
    }

    pub async fn put(&self, filename: &str, entry: &File) -> Result<(), io::Error> {
        let data_path = self.data_path(filename)?;
        fs::write(&data_path, &entry.bytes).await?;
        Ok(())
    }

    fn data_path(&self, filename: &str) -> Result<path::PathBuf, io::Error> {
        Ok(self.config.dirname().join(filename))
    }
}
