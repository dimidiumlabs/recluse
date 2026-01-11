// SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
// SPDX-License-Identifier: AGPL-3.0-or-later

use axum::{Router, body, extract, http, response, routing};
use std::sync::{Arc, OnceLock};

use crate::service_storage;
use crate::service_upstream;

pub struct ZigController {
    storage: Arc<service_storage::StorageService>,
    upstream: Arc<service_upstream::UpstreamService>,
}

impl ZigController {
    pub fn new(
        storage: Arc<service_storage::StorageService>,
        upstream: Arc<service_upstream::UpstreamService>,
    ) -> Self {
        Self { storage, upstream }
    }

    pub fn router(self: Arc<Self>) -> Router {
        Router::new()
            .route("/zig/{filename}", routing::get(Self::download))
            .with_state(self)
    }

    async fn download(
        extract::State(controller): extract::State<Arc<Self>>,
        extract::Path(filename): extract::Path<String>,
    ) -> Result<response::Response, http::StatusCode> {
        let version = Self::parse_version(&filename).ok_or(http::StatusCode::NOT_FOUND)?;
        let url = Self::build_upstream_url(&filename, &version);

        match controller.storage.get(&filename).await {
            Ok(Some(entry)) => {
                return Ok(Self::build_response(http::StatusCode::OK, entry));
            }
            Ok(None) => {}
            Err(_) => {
                return Err(http::StatusCode::INTERNAL_SERVER_ERROR);
            }
        }

        let entry = controller
            .upstream
            .fetch(service_upstream::DownloadRequest { url })
            .await?;
        let cache_entry = service_storage::File {
            bytes: entry.bytes.clone(),
        };

        match controller.storage.put(&filename, &cache_entry).await {
            Ok(()) => {}
            Err(_) => {
                return Err(http::StatusCode::OK);
            }
        }

        Ok(Self::build_response(http::StatusCode::OK, cache_entry))
    }

    fn parse_version(filename: &str) -> Option<String> {
        let re = Self::filename_regex();
        re.captures(filename)
            .and_then(|captures| captures.get(1))
            .map(|match_| match_.as_str().to_string())
    }

    fn filename_regex() -> &'static regex::Regex {
        static REGEX: OnceLock<regex::Regex> = OnceLock::new();
        REGEX.get_or_init(|| {
            regex::Regex::new(
                r"^zig(?:|-bootstrap|-[a-zA-Z0-9_]+-[a-zA-Z0-9_]+)-(\d+\.\d+\.\d+(?:-dev\.\d+\+[0-9a-f]+)?)\.(?:tar\.xz|zip)(?:\.minisig)?$",
            )
            .unwrap()
        })
    }

    fn build_upstream_url(filename: &str, version: &str) -> String {
        if version.contains("-dev.") {
            format!("https://ziglang.org/builds/{filename}")
        } else {
            format!("https://ziglang.org/download/{version}/{filename}")
        }
    }

    fn build_response(
        status: http::StatusCode,
        entry: service_storage::File,
    ) -> response::Response {
        response::Response::builder()
            .status(status)
            .header(http::header::CONTENT_TYPE, "application/octet-stream")
            .body(body::Body::from(entry.bytes))
            .unwrap()
    }
}
