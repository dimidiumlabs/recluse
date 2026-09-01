// SPDX-FileCopyrightText: 2026 Nikolay Govorov
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::{HeaderValue, Method, Request, Response, StatusCode, header};
use axum::{extract, routing};
use maud::{Markup, Render};
use serde::Deserialize;
use tower::{Layer, Service};
use tracing::error;

use base::ContentType;
use repos::{GoBackend, ZigBackend};

use crate::ui::{
    STYLESHEET,
    components::Document,
    pages::{
        GoRelease, GoReleaseFile, IndexPage, LicenseEntry as PageLicenseEntry,
        LicenseOverview as PageLicenseOverview, LicenseUse, LicensesPage, ZigRelease,
        ZigReleaseFile,
    },
};

fn get_asset(path: &str) -> Option<(&'static [u8], ContentType)> {
    Some(match path {
        "base.css" => (STYLESHEET, ContentType::TextCss),
        "favicon.svg" => (
            include_bytes!("assets/favicon.svg"),
            ContentType::ImageSvgXml,
        ),
        "favicon.ico" => (
            include_bytes!("assets/favicon.ico"),
            ContentType::ImageXIcon,
        ),
        "favicon-192.png" | "apple-touch-icon.png" => (
            include_bytes!("assets/favicon-192.png"),
            ContentType::ImagePng,
        ),
        "favicon-512.png" => (
            include_bytes!("assets/favicon-512.png"),
            ContentType::ImagePng,
        ),
        "manifest.webmanifest" => (
            include_bytes!("assets/manifest.webmanifest"),
            ContentType::ApplicationManifestJson,
        ),
        "jetbrainsmono/JetBrainsMono[wght].woff2" => (
            include_bytes!("assets/jetbrainsmono/JetBrainsMono[wght].woff2"),
            ContentType::FontWoff2,
        ),
        "jetbrainsmono/JetBrainsMono-Italic[wght].woff2" => (
            include_bytes!("assets/jetbrainsmono/JetBrainsMono-Italic[wght].woff2"),
            ContentType::FontWoff2,
        ),
        "robots.txt" => (include_bytes!("assets/robots.txt"), ContentType::TextPlain),
        _ => return None,
    })
}

const CSP: &str = "default-src 'self'; base-uri 'none'; img-src 'self'; font-src 'self'; style-src 'self'; script-src 'self'; object-src 'none'; frame-ancestors 'none'";

#[derive(Deserialize)]
struct LicenseList {
    overview: Vec<LicenseOverview>,
    licenses: Vec<LicenseEntry>,
}

#[derive(Deserialize)]
struct LicenseOverview {
    id: String,
    name: String,
    count: usize,
}

#[derive(Deserialize)]
struct LicenseEntry {
    id: String,
    name: String,
    text: String,
    first_of_kind: bool,
    used_by: Vec<LicenseUsedBy>,
}

#[derive(Deserialize)]
struct LicenseUsedBy {
    #[serde(rename = "crate")]
    crate_: LicenseCrate,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct LicenseCrate {
    name: String,
    version: String,
    repository: Option<String>,
}

static LICENSES: LazyLock<LicenseList> = LazyLock::new(|| {
    let json = include_str!(concat!(env!("OUT_DIR"), "/licenses.json"));
    serde_json::from_str(json).expect("failed to parse licenses.json")
});

/// Handles html pages rendering and static files
pub struct WebController {
    zig: Option<Arc<ZigBackend>>,
    go: Option<Arc<GoBackend>>,
}

impl WebController {
    pub fn new(zig: Option<Arc<ZigBackend>>, go: Option<Arc<GoBackend>>) -> Self {
        Self { zig, go }
    }
}

impl WebController {
    pub fn router(self: Arc<Self>) -> axum::Router {
        axum::Router::new()
            .route("/", axum::routing::get(Self::index))
            .route("/{*path}", routing::get(Self::assets))
            .route("/about/licenses", axum::routing::get(Self::licenses))
            .layer(CacheLayer)
            .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static(CSP),
            ))
            .with_state(self.clone())
    }

    async fn assets(extract::Path(path): extract::Path<String>) -> Response<Body> {
        let Some((data, content_type)) = get_asset(&path) else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap();
        };

        let mut builder = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type.as_str());

        if matches!(content_type, ContentType::FontWoff2) {
            builder = builder.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
        }

        builder.body(Body::from(data)).unwrap()
    }

    async fn index(extract::State(ctrl): extract::State<Arc<Self>>) -> Markup {
        let zig = if let Some(ref backend) = ctrl.zig {
            match backend.get_releases().await {
                Ok(releases) => releases
                    .into_iter()
                    .rev()
                    .map(|release| ZigRelease {
                        version: release.version,
                        date: release.meta.date,
                        docs: release.meta.docs,
                        std_docs: release.meta.std_docs,
                        notes: release.meta.notes,
                        files: release
                            .files
                            .into_iter()
                            .map(|file| ZigReleaseFile {
                                filename: file.filename,
                                target: file.meta.target,
                            })
                            .collect(),
                    })
                    .collect(),
                Err(e) => {
                    error!("failed to get zig versions: {e}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        let go = if let Some(ref backend) = ctrl.go {
            match backend.get_releases().await {
                Ok(releases) => releases
                    .into_iter()
                    .rev()
                    .map(|release| GoRelease {
                        version: release.version,
                        stable: release.meta.stable,
                        files: release
                            .files
                            .into_iter()
                            .map(|file| GoReleaseFile {
                                filename: file.filename,
                            })
                            .collect(),
                    })
                    .collect(),
                Err(e) => {
                    error!("failed to get go versions: {e}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        Document {
            title: "Tesor — tiny & opinionated packages mirror",
            content: IndexPage { zig, go }.render(),
        }
        .render()
    }

    async fn licenses() -> Markup {
        let data = &*LICENSES;
        let overview = data
            .overview
            .iter()
            .map(|entry| PageLicenseOverview {
                id: entry.id.clone(),
                name: entry.name.clone(),
                count: entry.count,
            })
            .collect();
        let licenses = data
            .licenses
            .iter()
            .map(|entry| PageLicenseEntry {
                id: entry.id.clone(),
                name: entry.name.clone(),
                text: entry.text.clone(),
                first_of_kind: entry.first_of_kind,
                used_by: entry
                    .used_by
                    .iter()
                    .map(|usage| LicenseUse {
                        name: usage.crate_.name.clone(),
                        version: usage.crate_.version.clone(),
                        repository: usage.crate_.repository.clone(),
                    })
                    .collect(),
            })
            .collect();
        Document {
            title: "Third Party Licenses",
            content: LicensesPage {
                project_license: include_str!("../../../LICENSE"),
                overview,
                licenses,
            }
            .render(),
        }
        .render()
    }
}

#[derive(Clone)]
struct CacheLayer;

impl<S> Layer<S> for CacheLayer {
    type Service = CacheService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        CacheService { inner }
    }
}

#[derive(Clone)]
struct CacheService<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for CacheService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let method = req.method().clone();
        let if_none_match = req.headers().get(header::IF_NONE_MATCH).cloned();

        let mut inner = self.inner.clone();
        Box::pin(async move {
            let resp = inner.call(req).await?;

            if !resp.status().is_success() {
                return Ok(resp);
            }

            let (parts, body) = resp.into_parts();
            let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();

            let mut hasher = crc32fast::Hasher::new();
            hasher.update(&bytes);
            let etag = format!("\"{}\"", hex::encode(hasher.finalize().to_be_bytes()));
            let etag_header = HeaderValue::from_str(&etag).unwrap();
            let cache_control = parts
                .headers
                .get(header::CACHE_CONTROL)
                .cloned()
                .unwrap_or(HeaderValue::from_static("no-cache"));

            if (method == Method::GET || method == Method::HEAD)
                && if_none_match
                    .as_ref()
                    .is_some_and(|v| v.as_bytes() == etag.as_bytes())
            {
                let mut resp = Response::builder()
                    .status(StatusCode::NOT_MODIFIED)
                    .body(Body::empty())
                    .unwrap();
                resp.headers_mut().insert(header::ETAG, etag_header);
                resp.headers_mut()
                    .insert(header::CACHE_CONTROL, cache_control);
                return Ok(resp);
            }

            let mut resp = Response::from_parts(parts, Body::from(bytes));
            resp.headers_mut().insert(header::ETAG, etag_header);
            resp.headers_mut()
                .entry(header::CACHE_CONTROL)
                .or_insert(HeaderValue::from_static("no-cache"));
            Ok(resp)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_stylesheet_is_served_as_base_css() {
        let (data, content_type) = get_asset("base.css").expect("base stylesheet");
        assert_eq!(data, STYLESHEET);
        assert!(matches!(content_type, ContentType::TextCss));

        let stylesheet = std::str::from_utf8(data).expect("UTF-8 stylesheet");
        assert!(stylesheet.contains(":root{"), "missing foundation styles");
        assert!(
            stylesheet.contains("display:block"),
            "missing component styles"
        );
    }

    #[test]
    fn font_asset_keeps_its_immutable_cache_category() {
        let (_, content_type) = get_asset("jetbrainsmono/JetBrainsMono[wght].woff2").expect("font");
        assert!(matches!(content_type, ContentType::FontWoff2));
    }

    #[tokio::test]
    async fn stylesheet_keeps_csp_etag_cache_and_head_behavior() {
        use tower::ServiceExt;

        let router = Arc::new(WebController::new(None, None)).router();
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/base.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], CSP);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
        let etag = response.headers()[header::ETAG].clone();
        let cached = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/base.css")
                    .header(header::IF_NONE_MATCH, &etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cached.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(cached.headers()[header::ETAG], etag);
        assert_eq!(cached.headers()[header::CACHE_CONTROL], "no-cache");
        let head = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::HEAD)
                    .uri("/base.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(head.headers()[header::ETAG], etag);
        let conditional_head = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::HEAD)
                    .uri("/base.css")
                    .header(header::IF_NONE_MATCH, &etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(conditional_head.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            conditional_head.headers()[header::CACHE_CONTROL],
            "no-cache"
        );
        let font = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/jetbrainsmono/JetBrainsMono[wght].woff2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(font.status(), StatusCode::OK);
        assert_eq!(
            font.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
        let font_etag = font.headers()[header::ETAG].clone();
        let cached_font = router
            .oneshot(
                Request::builder()
                    .uri("/jetbrainsmono/JetBrainsMono[wght].woff2")
                    .header(header::IF_NONE_MATCH, &font_etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cached_font.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(cached_font.headers()[header::ETAG], font_etag);
        assert_eq!(
            cached_font.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
    }
}
